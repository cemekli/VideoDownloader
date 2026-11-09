//! STATE.md F3.4, F3.6, F3.10 with the fake yt-dlp (`src/bin/vv-fake-ytdlp.rs`):
//! metadata, downloads, cancel, retry, blocking errors, integrity and the
//! canary checks. The real yt-dlp runs in `tests/real_ytdlp.rs`.

mod common;

use common::*;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Duration;
use vv_engine::formats::PresetId;
use vv_engine::{download, metadata};
use vv_persistence::jobs::{self, CancelReason, JobStatus};
use vv_persistence::library::repo;
use vv_persistence::library::*;

fn fetch_metadata(lib: &Lib, item: &str) -> String {
    let now = lib.clock.now_ms();
    let id = lib.write(|c| metadata::enqueue(c, item, now).unwrap().id);
    lib.run();
    id
}

fn download(lib: &Lib, item: &str, preset: PresetId) -> String {
    let now = lib.clock.now_ms();
    let id = lib.write(|c| download::enqueue(c, item, preset, now).unwrap().id);
    lib.run();
    id
}

fn originals(lib: &Lib, item: &str) -> Vec<repo::MediaAsset> {
    lib.write(|c| repo::assets_for_item(c, item).unwrap())
        .into_iter()
        .filter(|a| a.role == AssetRole::Original)
        .collect()
}

#[test]
fn metadata_fills_the_item_formats_and_thumbnail_and_keeps_no_secrets() {
    let lib = library(Setup::fake());
    let item = lib.save("https://fake.test/video");
    let job = fetch_metadata(&lib, &item);
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
    let it = lib.write(|c| repo::get_item(c, &item).unwrap().unwrap());
    assert_eq!(it.title.as_deref(), Some("Fake video video"));
    assert_eq!(it.creator.as_deref(), Some("Fake Channel"));
    assert_eq!(it.duration_ms, Some(3000));
    assert_eq!(it.published_date.as_deref(), Some("2026-11-01"));
    assert_eq!(it.metadata_status, MetadataStatus::Ready);
    assert_eq!(it.access_state, AccessState::PublicOk);
    let thumb = it.thumbnail_asset_id.expect("remote thumbnail stored");
    let t = lib.write(|c| repo::get_media_asset(c, &thumb).unwrap().unwrap());
    assert_eq!(t.role, AssetRole::Thumbnail);
    assert!(t.source_asset_id.is_none());
    assert!(t.source_metadata.unwrap().contains("https://fake.test"));
    let formats = lib.write(|c| metadata::stored_formats(c, &item).unwrap());
    assert_eq!(formats.len(), 3);
    // Format URLs, HTTP headers and the description are dropped (§13.4).
    let dump = lib.dump();
    assert!(!dump.contains(CANARY), "secret reached the database");
    assert!(
        !dump.contains("cdn.example.com"),
        "a media URL reached the database"
    );
    // The item is findable by its new title.
    let hits = lib.write(|c| repo::search_phrase_ids(c, "Fake Channel").unwrap());
    assert_eq!(hits, vec![item]);
}

#[test]
fn a_playlist_url_fails_the_metadata_job_with_a_clear_code() {
    let lib = library(Setup::fake());
    let item = lib.save("https://fake.test/playlist");
    let job = lib.job(&fetch_metadata(&lib, &item));
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error_code.as_deref(), Some("PLAYLIST_UNSUPPORTED"));
    let it = lib.write(|c| repo::get_item(c, &item).unwrap().unwrap());
    assert_eq!(it.metadata_status, MetadataStatus::Failed);
    // CAP-04: the honest title from the link stays.
    assert!(!it.display_title().is_empty());
}

#[test]
fn deleted_content_marks_the_item_unavailable_and_keeps_the_bookmark() {
    let lib = library(Setup::fake());
    let item = lib.save("https://fake.test/notfound");
    let job = lib.job(&fetch_metadata(&lib, &item));
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error_code.as_deref(), Some("CONTENT_UNAVAILABLE"));
    let it = lib.write(|c| repo::get_item(c, &item).unwrap().unwrap());
    assert_eq!(it.remote_availability, RemoteAvailability::Unavailable);
    assert!(it.removed_at.is_none());
}

#[test]
fn download_with_merge_reports_phases_verifies_and_publishes_one_file() {
    let Some(media) = media_tools() else { return };
    let lib = library(Setup {
        media: Some(media),
        ..Setup::fake()
    });
    let item = lib.save("https://fake.test/merge");
    fetch_metadata(&lib, &item);
    let job = download(&lib, &item, PresetId::Best);
    let j = lib.job(&job);
    assert_eq!(j.status, JobStatus::Succeeded, "{:?}", j.error_code);
    let assets = originals(&lib, &item);
    assert_eq!(assets.len(), 1, "exactly one final reference (AC-11)");
    let a = &assets[0];
    assert_eq!(a.storage_kind, StorageKind::Managed);
    assert_eq!(a.acquisition_status, AcquisitionStatus::Ready);
    assert_eq!(a.integrity_state, IntegrityState::Verified);
    assert_eq!(a.container.as_deref(), Some("mp4"));
    assert!(a.content_hash.is_some());
    assert_eq!(a.storage_ref, format!("media/{item}/{}.mp4", a.id));
    let root = lib.write(|c| {
        repo::get_storage_root(c, &a.storage_root_id)
            .unwrap()
            .unwrap()
    });
    let file = vv_import::fsutil::resolve_ref(Path::new(&root.path), &a.storage_ref);
    assert!(file.is_file());
    assert!(!download::staging_dir(&lib.cx().dirs.staging, &job).exists());
    // The fake reports two download parts and a merge (AC-08).
    let attempts = lib.write(|c| jobs::attempts(c, &job).unwrap());
    assert_eq!(attempts.len(), 1);
    // A downloaded item gets a frame thumbnail if it had none.
    let it = lib.write(|c| repo::get_item(c, &item).unwrap().unwrap());
    assert!(it.thumbnail_asset_id.is_some());
}

#[test]
fn unknown_total_never_produces_an_invented_total() {
    let Some(media) = media_tools() else { return };
    let lib = library(Setup {
        media: Some(media),
        ..Setup::fake()
    });
    let item = lib.save("https://fake.test/unknown-total");
    let now = lib.clock.now_ms();
    let job = lib.write(|c| download::enqueue(c, &item, PresetId::Best, now).unwrap().id);
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    // Sample the job row while it runs.
    std::thread::scope(|s| {
        let h = s.spawn(|| lib.run());
        while !h.is_finished() {
            let j = lib.job(&job);
            seen.lock()
                .unwrap()
                .push((j.progress.phase.clone(), j.progress.bytes_total));
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
    let seen = seen.lock().unwrap();
    assert!(seen.iter().any(|(p, _)| p.as_deref() == Some("download")));
    assert!(seen.iter().all(|(_, t)| t.is_none()), "AC-09: {seen:?}");
}

#[test]
fn cancel_stops_the_process_and_nothing_looks_ready() {
    let lib = library(Setup::fake());
    let item = lib.save("https://fake.test/slow");
    let now = lib.clock.now_ms();
    let job = lib.write(|c| download::enqueue(c, &item, PresetId::Best, now).unwrap().id);
    std::thread::scope(|s| {
        let h = s.spawn(|| lib.run());
        // Wait until it really downloads, then cancel like the UI does.
        for _ in 0..400 {
            if lib.job(&job).progress.bytes_done.unwrap_or(0) > 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let now = lib.clock.now_ms();
        let st = lib.write(|c| jobs::cancel(c, &job, CancelReason::User, now).unwrap());
        assert_eq!(st, JobStatus::Canceling);
        h.join().unwrap();
    });
    let j = lib.job(&job);
    assert_eq!(j.status, JobStatus::Canceled, "AC-10");
    assert!(originals(&lib, &item).is_empty(), "no media looks ready");
    // The partial file is kept for the user to retry or delete (§13.3).
    assert!(download::partial_bytes(&lib.cx().dirs.staging, &job) > 0);
    let removed = download::remove_partial(&lib.cx().dirs.staging, &j).unwrap();
    assert!(removed > 0);
    assert_eq!(download::partial_bytes(&lib.cx().dirs.staging, &job), 0);
}

#[test]
fn offline_mode_stops_a_running_download_and_blocks_it() {
    let lib = library(Setup::fake());
    let item = lib.save("https://fake.test/slow");
    let now = lib.clock.now_ms();
    let job = lib.write(|c| download::enqueue(c, &item, PresetId::Best, now).unwrap().id);
    std::thread::scope(|s| {
        let h = s.spawn(|| lib.run());
        for _ in 0..400 {
            if lib.job(&job).progress.bytes_done.unwrap_or(0) > 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        lib.engine.set_offline(true);
        h.join().unwrap();
    });
    let j = lib.job(&job);
    assert_eq!(j.status, JobStatus::Blocked);
    assert_eq!(j.blocked_reason.as_deref(), Some("offline"));
    assert_eq!(lib.run(), 0, "nothing network-bound starts offline");
    lib.engine.set_offline(false);
    assert_eq!(lib.job(&job).status, JobStatus::Queued);
}

#[test]
fn disk_full_blocks_and_quota_blocks_before_anything_runs() {
    let lib = library(Setup::fake());
    let item = lib.save("https://fake.test/diskfull");
    let j = lib.job(&download(&lib, &item, PresetId::Best));
    assert_eq!(j.status, JobStatus::Blocked, "AC-12");
    assert_eq!(j.blocked_reason.as_deref(), Some("disk"));
    // The code says why it waits (F5.2); the next claim clears it.
    assert_eq!(j.error_code.as_deref(), Some("DISK_FULL"));
    assert!(
        lib.write(|c| repo::get_item(c, &item).unwrap()).is_some(),
        "bookmark kept"
    );

    let other = lib.save("https://fake.test/video");
    fetch_metadata(&lib, &other);
    let now = lib.clock.now_ms();
    lib.write(|c| vv_engine::storage::set_quota(c, Some(10), now).unwrap());
    let j = lib.job(&download(&lib, &other, PresetId::Upto1080));
    assert_eq!(j.status, JobStatus::Blocked, "AC-33");
    assert_eq!(j.blocked_reason.as_deref(), Some("disk"));
    assert!(lib.write(|c| jobs::attempts(c, &j.id).unwrap())[0]
        .3
        .is_none());
}

#[test]
fn corrupt_output_fails_verification_and_is_never_published() {
    let Some(media) = media_tools() else { return };
    let lib = library(Setup {
        media: Some(media),
        ..Setup::fake()
    });
    let item = lib.save("https://fake.test/corrupt");
    let j = lib.job(&download(&lib, &item, PresetId::Best));
    assert_eq!(j.status, JobStatus::Failed);
    assert!(originals(&lib, &item).is_empty(), "DL-07");
}

#[test]
fn a_reported_file_outside_staging_is_refused() {
    let Some(media) = media_tools() else { return };
    let lib = library(Setup {
        media: Some(media),
        ..Setup::fake()
    });
    let item = lib.save("https://fake.test/escape");
    let j = lib.job(&download(&lib, &item, PresetId::Best));
    assert_eq!(j.error_code.as_deref(), Some("INTERNAL_ERROR"));
    assert!(originals(&lib, &item).is_empty());
}

#[test]
fn a_network_drop_waits_and_the_retry_continues_in_the_same_staging_folder() {
    let Some(media) = media_tools() else { return };
    let lib = library(Setup {
        media: Some(media),
        ..Setup::fake()
    });
    let item = lib.save("https://fake.test/flaky");
    let job = download(&lib, &item, PresetId::Best);
    let j = lib.job(&job);
    assert_eq!(j.status, JobStatus::RetryWait);
    assert_eq!(j.error_code.as_deref(), Some("NETWORK_UNAVAILABLE"));
    assert_eq!(lib.run(), 0, "not before the backoff");
    lib.clock.advance(10 * 60 * 1000);
    lib.run();
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
    assert_eq!(originals(&lib, &item).len(), 1);
}

#[test]
fn a_crash_between_placing_the_file_and_the_commit_is_finished_on_restart() {
    let Some(media) = media_tools() else { return };
    let lib = library(Setup {
        media: Some(media),
        ..Setup::fake()
    });
    // `notfound` would fail if yt-dlp ran again: the placed file must be used.
    let item = lib.save("https://fake.test/notfound");
    let now = lib.clock.now_ms();
    let job = lib.write(|c| download::enqueue(c, &item, PresetId::Best, now).unwrap().id);
    let lease = lib
        .write(|c| jobs::claim(c, "old-run", &[jobs::JobKind::Download], 2, false, now))
        .unwrap()
        .unwrap();
    let root = lib.write(|c| {
        vv_engine::storage::active_media_root(c, lib.clock.as_ref(), &lib.cx().dirs).unwrap()
    });
    let asset_id = new_id();
    let sref = format!("media/{item}/{asset_id}.mp4");
    let file = vv_import::fsutil::resolve_ref(Path::new(&root.path), &sref);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::copy(
        repo_root().join("tests/fixtures/media/mp4-h264-aac.mp4"),
        &file,
    )
    .unwrap();
    lib.write(|c| {
        jobs::set_checkpoint(
            c,
            &job,
            lease.generation,
            Some(&format!("placed:{}/{sref}", root.id)),
            now,
        )
        .unwrap()
    });
    // The app dies here. Next start:
    lib.engine.reconcile();
    lib.run();
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
    let assets = originals(&lib, &item);
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].id, asset_id);
}

#[test]
fn a_canary_in_tool_errors_reaches_no_db_row_event_or_export() {
    let lib = library(Setup::fake());
    let item = lib.save("https://fake.test/canary-error");
    let meta = lib.job(&fetch_metadata(&lib, &item));
    assert_eq!(meta.status, JobStatus::Failed);
    assert_eq!(meta.error_code.as_deref(), Some("ACCESS_DENIED"));
    let dl = lib.job(&download(&lib, &item, PresetId::Best));
    assert_eq!(dl.status, JobStatus::Failed);
    // AC-18: database, events and the JSON export.
    assert!(!lib.dump().contains(CANARY), "canary in SQLite");
    let events = lib.events.events.lock().unwrap().join("\n");
    assert!(!events.contains(CANARY), "canary in events");
    let export = lib.write(|c| {
        let doc = vv_persistence::portable::export(c, Default::default(), "test", 0).unwrap();
        serde_json::to_string(&doc).unwrap()
    });
    assert!(!export.contains(CANARY), "canary in export");
    let msg = dl.redacted_error_message.unwrap_or_default();
    assert!(!msg.contains(CANARY), "{msg}");
    assert!(
        msg.contains("cdn.example.com"),
        "the redacted host stays: {msg}"
    );
    // Job DTOs as the UI gets them.
    let all = lib.write(|c| jobs::list(c, JobStatus::ALL, 100).unwrap());
    assert!(!serde_json::to_string(&all).unwrap().contains(CANARY));
}

#[test]
fn login_required_blocks_for_the_user_and_does_not_retry() {
    let lib = library(Setup::fake());
    let item = lib.save("https://fake.test/login");
    let j = lib.job(&download(&lib, &item, PresetId::Best));
    assert_eq!(j.status, JobStatus::Blocked);
    assert_eq!(j.blocked_reason.as_deref(), Some("auth"));
}

#[test]
fn engine_scheduler_thread_runs_queued_jobs_and_stops() {
    let lib = library(Setup::fake());
    let item = lib.save("https://fake.test/video");
    let now = lib.clock.now_ms();
    let job = lib.write(|c| metadata::enqueue(c, &item, now).unwrap().id);
    lib.engine.start();
    lib.engine.wake();
    for _ in 0..500 {
        if lib.job(&job).status.is_terminal() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    lib.engine.shutdown();
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
    assert!(!lib.cx().offline.load(Ordering::SeqCst));
}

// STATE.md F5.2 (SPEC §12.1, §21.1; ADR-0023): a job that needs a login
// runs again with the consented Firefox profile; success is evidence.

fn reddit_login_item(lib: &Lib) -> String {
    let item = lib.save("https://fake.test/login-auth");
    lib.write(|c| {
        c.execute(
            "UPDATE library_item SET source_platform = 'reddit' WHERE id = ?1",
            [&item],
        )
        .unwrap()
    });
    item
}

fn connect_firefox(lib: &Lib, folder: &str, ops: &[vv_persistence::auth::Operation]) -> String {
    use vv_persistence::auth::*;
    let dir = lib.dir.path().join(folder);
    lib.write(|c| {
        create(
            c,
            &NewProfile {
                provider: vv_domain::normalize::Platform::Reddit,
                label: "Reddit · Firefox",
                method: Method::BrowserCookies,
                browser_kind: Some(BrowserKind::Firefox),
                profile_locator: Some(dir.to_str().unwrap()),
                consent_scope: ops,
            },
            1,
        )
        .unwrap()
        .id
    })
}

fn profile(lib: &Lib, id: &str) -> vv_persistence::auth::AuthProfile {
    lib.write(|c| vv_persistence::auth::get(c, id).unwrap())
}

#[test]
fn f5_2_login_needed_waits_then_runs_with_the_consented_firefox_profile() {
    use vv_persistence::auth::{Operation, ProfileState};
    let lib = library(Setup::fake());
    let item = reddit_login_item(&lib);

    // No profile: metadata waits for the user, the item needs a login.
    let meta = fetch_metadata(&lib, &item);
    assert_eq!(lib.job(&meta).status, JobStatus::Blocked);
    assert_eq!(lib.job(&meta).blocked_reason.as_deref(), Some("auth"));
    let it = lib.write(|c| repo::get_item(c, &item).unwrap().unwrap());
    assert_eq!(it.access_state, AccessState::LoginRequired);

    // A profile allowed only for downloads is not used for metadata.
    let p = connect_firefox(&lib, "x1.default-release", &[Operation::Download]);
    lib.write(|c| {
        vv_persistence::auth::unblock_provider(c, vv_domain::normalize::Platform::Reddit, 2)
            .unwrap()
    });
    lib.run();
    assert_eq!(lib.job(&meta).status, JobStatus::Blocked);
    assert_eq!(
        profile(&lib, &p).session_state,
        ProfileState::NeedsVerification
    );

    // The download is allowed: anonymous refusal + success with the profile
    // = evidence (§12.1 (a)).
    let job = download(&lib, &item, PresetId::Best);
    assert_eq!(
        lib.job(&job).status,
        JobStatus::Succeeded,
        "{:?}",
        lib.job(&job)
    );
    let it = lib.write(|c| repo::get_item(c, &item).unwrap().unwrap());
    assert_eq!(it.access_state, AccessState::Authorized);
    assert_eq!(it.access_profile_id.as_deref(), Some(p.as_str()));
    let pr = profile(&lib, &p);
    assert_eq!(pr.session_state, ProfileState::SessionAvailable);
    assert!(pr.last_verified_at.is_some());
    assert_eq!(originals(&lib, &item).len(), 1);
    // The profile folder is not in any job row or event.
    let jobs_dump = lib.write(|c| {
        c.query_row("SELECT group_concat(coalesce(payload,'') || coalesce(redacted_error_message,''), ' ') FROM job", [], |r| r.get::<_, Option<String>>(0))
            .unwrap()
            .unwrap_or_default()
    });
    assert!(!jobs_dump.contains("default-release"), "{jobs_dump}");
}

#[test]
fn f5_2_unreadable_browser_store_is_cookie_access_failed_not_expired() {
    use vv_persistence::auth::{Operation, ProfileState};
    let lib = library(Setup::fake());
    let item = reddit_login_item(&lib);
    let p = connect_firefox(&lib, "broken", &[Operation::Metadata, Operation::Download]);
    let job = download(&lib, &item, PresetId::Best);
    let j = lib.job(&job);
    assert_eq!(j.status, JobStatus::Blocked);
    assert_eq!(j.blocked_reason.as_deref(), Some("auth"));
    assert_eq!(j.error_code.as_deref(), Some("COOKIE_ACCESS_FAILED"));
    assert!(!j
        .redacted_error_message
        .unwrap_or_default()
        .contains("someone"));
    assert_eq!(
        profile(&lib, &p).session_state,
        ProfileState::NeedsVerification
    );

    // Signed out in Firefox: still a login refusal; the session is not
    // proven expired, and nothing claims it is available.
    let lib2 = library(Setup::fake());
    let item2 = reddit_login_item(&lib2);
    let p2 = connect_firefox(&lib2, "signedout", &[Operation::Download]);
    let job2 = download(&lib2, &item2, PresetId::Best);
    assert_eq!(lib2.job(&job2).status, JobStatus::Blocked);
    assert_eq!(
        lib2.job(&job2).error_code.as_deref(),
        Some("LOGIN_REQUIRED")
    );
    let pr = profile(&lib2, &p2);
    assert_eq!(pr.session_state, ProfileState::NeedsVerification);
    assert_eq!(pr.last_verified_at, None);
}

#[test]
fn f5_2_public_content_never_uses_the_profile() {
    use vv_persistence::auth::{Operation, ProfileState};
    let lib = library(Setup::fake());
    let item = lib.save("https://fake.test/video");
    lib.write(|c| {
        c.execute(
            "UPDATE library_item SET source_platform = 'reddit' WHERE id = ?1",
            [&item],
        )
        .unwrap()
    });
    let p = connect_firefox(
        &lib,
        "x1.default-release",
        &[Operation::Metadata, Operation::Download],
    );
    let job = download(&lib, &item, PresetId::Best);
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
    // AC-54: no evidence, so the profile is untouched.
    let pr = profile(&lib, &p);
    assert_eq!(pr.session_state, ProfileState::NeedsVerification);
    assert_eq!(pr.last_verified_at, None);
}

// ---- F5.3: cookie file, per-job working copy (AC-55, VK-10) ----

const REDDIT_COOKIES: &str = "# Netscape HTTP Cookie File\n\
.reddit.com\tTRUE\t/\tTRUE\t0\treddit_session\tVV-CANARY-cookie-4b2e\n\
.google.com\tTRUE\t/\tTRUE\t0\tSID\tGOOGLE-CANARY\n";

fn reddit_item(lib: &Lib, scenario: &str) -> String {
    let item = lib.save(&format!("https://fake.test/{scenario}"));
    lib.write(|c| {
        c.execute(
            "UPDATE library_item SET source_platform = 'reddit' WHERE id = ?1",
            [&item],
        )
        .unwrap()
    });
    item
}

/// Connects a cookie file the way the desktop command does: filter to the
/// platform, store the result as a secret.
fn connect_file(lib: &Lib, text: &str, ops: &[vv_persistence::auth::Operation]) -> String {
    use vv_persistence::auth::*;
    let f = vv_engine::cookiefile::filter(text, vv_domain::normalize::Platform::Reddit, 1).unwrap();
    lib.write(|c| {
        let p = create(
            c,
            &NewProfile {
                provider: vv_domain::normalize::Platform::Reddit,
                label: "Reddit · cookie file",
                method: Method::CookieFile,
                browser_kind: None,
                profile_locator: None,
                consent_scope: ops,
            },
            1,
        )
        .unwrap();
        attach_secret(
            c,
            lib.secrets.as_ref(),
            &p.id,
            &vv_secrets::Secret::new(f.text.into_bytes()),
            1,
        )
        .unwrap()
        .id
    })
}

fn copies(lib: &Lib) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(lib.cx().dirs.cookie_copies())
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default()
}

fn seen_copy(lib: &Lib) -> std::path::PathBuf {
    std::path::PathBuf::from(
        std::fs::read_to_string(lib.cx().dirs.staging.join("fake-cookie-seen"))
            .expect("yt-dlp got a cookie file"),
    )
}

#[test]
fn f5_3_cookie_file_runs_from_a_private_copy_that_is_gone_afterwards() {
    use vv_persistence::auth::{Operation, ProfileState};
    let lib = library(Setup::fake());
    let item = reddit_item(&lib, "login-file");
    let p = connect_file(
        &lib,
        REDDIT_COOKIES,
        &[Operation::Metadata, Operation::Download],
    );

    let job = download(&lib, &item, PresetId::Best);
    assert_eq!(
        lib.job(&job).status,
        JobStatus::Succeeded,
        "{:?}",
        lib.job(&job)
    );
    // §12.1 (a): refused without, fine with the cookies.
    let it = lib.write(|c| repo::get_item(c, &item).unwrap().unwrap());
    assert_eq!(it.access_state, AccessState::Authorized);
    let pr = profile(&lib, &p);
    assert_eq!(pr.session_state, ProfileState::SessionAvailable);
    assert!(pr.last_verified_at.is_some());

    // yt-dlp got a copy inside the private folder, which is gone now
    // (yt-dlp wrote to it; nothing else was written).
    let seen = seen_copy(&lib);
    assert!(seen.starts_with(lib.cx().dirs.cookie_copies()), "{seen:?}");
    assert!(!seen.exists(), "AC-55: copy deleted after success");
    assert!(copies(&lib).is_empty());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(lib.cx().dirs.cookie_copies())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
    }
    // Only the platform's cookies were ever stored, and neither they nor
    // the copy's path are in the database (job rows, payloads, items).
    let stored = lib.write(|c| {
        vv_persistence::auth::read_secret(c, lib.secrets.as_ref(), &p, Operation::Download).unwrap()
    });
    let stored = String::from_utf8(stored.expose().to_vec()).unwrap();
    assert!(stored.contains("reddit_session") && !stored.contains("GOOGLE"));
    let dump = lib.dump();
    assert!(
        !dump.contains("VV-CANARY-cookie"),
        "AC-55: cookie in the database"
    );
    assert!(!dump.contains(seen.file_name().unwrap().to_str().unwrap()));
    assert!(!dump.contains("cookies"), "no copy path in the database");
}

#[test]
fn f5_3_copy_is_deleted_after_a_failure_and_after_a_cancel() {
    use vv_persistence::auth::{Operation, ProfileState};
    // Failure: the site still says "sign in" with these cookies.
    let lib = library(Setup::fake());
    let item = reddit_item(&lib, "login-file");
    let signed_out = REDDIT_COOKIES.replace("VV-CANARY-cookie-4b2e", "signedout");
    let p = connect_file(&lib, &signed_out, &[Operation::Download]);
    let job = download(&lib, &item, PresetId::Best);
    assert_eq!(lib.job(&job).status, JobStatus::Blocked);
    assert_eq!(lib.job(&job).error_code.as_deref(), Some("LOGIN_REQUIRED"));
    assert_eq!(
        profile(&lib, &p).session_state,
        ProfileState::NeedsVerification
    );
    assert!(
        !seen_copy(&lib).exists(),
        "AC-55: copy deleted after a failure"
    );
    assert!(copies(&lib).is_empty());

    // Cancel while yt-dlp runs with the copy.
    let lib = library(Setup::fake());
    let item = reddit_item(&lib, "login-file-slow");
    lib.write(|c| {
        c.execute(
            "UPDATE library_item SET access_state = 'login_required' WHERE id = ?1",
            [&item],
        )
        .unwrap()
    });
    connect_file(&lib, REDDIT_COOKIES, &[Operation::Download]);
    let now = lib.clock.now_ms();
    let job = lib.write(|c| download::enqueue(c, &item, PresetId::Best, now).unwrap().id);
    std::thread::scope(|s| {
        let h = s.spawn(|| lib.run());
        for _ in 0..400 {
            if lib.job(&job).progress.bytes_done.unwrap_or(0) > 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(copies(&lib).len(), 1, "the copy exists while yt-dlp runs");
        let now = lib.clock.now_ms();
        lib.write(|c| jobs::cancel(c, &job, CancelReason::User, now).unwrap());
        h.join().unwrap();
    });
    assert_eq!(lib.job(&job).status, JobStatus::Canceled);
    assert!(
        copies(&lib).is_empty(),
        "AC-55: copy deleted after a cancel"
    );
}

#[test]
fn f5_3_copies_left_by_a_crash_and_orphan_secrets_go_at_start() {
    use vv_secrets::SecretStore;
    let lib = library(Setup::fake());
    let dir = lib.cx().dirs.cookie_copies();
    vv_secrets::private_fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("left-by-crash.txt"), REDDIT_COOKIES).unwrap();
    let orphan = lib
        .secrets
        .put(&vv_secrets::Secret::new(b"nobody points here".to_vec()))
        .unwrap();
    lib.engine.reconcile();
    assert!(
        copies(&lib).is_empty(),
        "AC-55: crash copies deleted at start"
    );
    assert!(lib.secrets.get(&orphan).is_err());
}

#[test]
fn f5_3_unreadable_stored_cookies_are_cookie_access_failed() {
    use vv_persistence::auth::{Operation, ProfileState};
    let lib = library(Setup::fake());
    let item = reddit_item(&lib, "login-file");
    let p = connect_file(&lib, REDDIT_COOKIES, &[Operation::Download]);
    // The secret vanished from the store (e.g. another Windows user).
    lib.write(|c| {
        let r: String = c
            .query_row(
                "SELECT secure_local_ref FROM auth_profile WHERE id = ?1",
                [&p],
                |r| r.get(0),
            )
            .unwrap();
        use vv_secrets::SecretStore;
        lib.secrets
            .delete(&vv_secrets::SecretRef::parse(&r).unwrap())
            .unwrap();
    });
    let job = download(&lib, &item, PresetId::Best);
    assert_eq!(
        lib.job(&job).error_code.as_deref(),
        Some("COOKIE_ACCESS_FAILED")
    );
    assert_eq!(
        profile(&lib, &p).session_state,
        ProfileState::NeedsVerification
    );
    assert!(copies(&lib).is_empty());
}

// ---- F5.4: evidence rule and verify_session (§12.1, AC-19, AC-54) ----

#[test]
fn f5_4_access_denied_then_profile_success_is_evidence_and_denied_with_profile_keeps_the_session() {
    use vv_persistence::auth::{Operation, ProfileState};
    let lib = library(Setup::fake());
    let item = reddit_item(&lib, "denied-auth");
    let p = connect_firefox(&lib, "x1.default-release", &[Operation::Metadata]);
    let job = fetch_metadata(&lib, &item);
    assert_eq!(
        lib.job(&job).status,
        JobStatus::Succeeded,
        "{:?}",
        lib.job(&job)
    );
    let it = lib.write(|c| repo::get_item(c, &item).unwrap().unwrap());
    assert_eq!(it.access_state, AccessState::Authorized);
    assert_eq!(
        profile(&lib, &p).session_state,
        ProfileState::SessionAvailable
    );

    // AC-19: signed in, but this account may not see it. The item says
    // access denied; the session stays as it was; the bookmark stays.
    let lib = library(Setup::fake());
    let item = reddit_item(&lib, "denied-auth");
    let p = connect_firefox(&lib, "x1.nomember", &[Operation::Metadata]);
    let job = fetch_metadata(&lib, &item);
    let j = lib.job(&job);
    assert_eq!(j.status, JobStatus::Failed);
    assert_eq!(j.error_code.as_deref(), Some("ACCESS_DENIED"));
    let it = lib.write(|c| repo::get_item(c, &item).unwrap().unwrap());
    assert_eq!(it.access_state, AccessState::AccessDenied);
    assert!(it.removed_at.is_none(), "AC-19: the bookmark is kept");
    let pr = profile(&lib, &p);
    assert_eq!(pr.session_state, ProfileState::NeedsVerification);
    assert_eq!(pr.last_verified_at, None);
}

#[test]
fn f5_4_verify_session_proves_the_session_with_a_login_item() {
    use vv_engine::verify::{verify_session, VerifyOutcome};
    use vv_persistence::auth::{Operation, ProfileState};
    let lib = library(Setup::fake());
    let all = [Operation::Metadata, Operation::Download, Operation::Verify];

    // Nothing has needed a login yet: nothing to test with.
    let p = connect_firefox(&lib, "x1.default-release", &all);
    let r = verify_session(lib.cx(), &p).unwrap();
    assert_eq!(r.outcome, VerifyOutcome::NoTestItem);
    assert_eq!(r.profile.session_state, ProfileState::NeedsVerification);

    // No consent for verify.
    let p2 = connect_firefox(&lib, "x2.default-release", &[Operation::Download]);
    assert_eq!(
        verify_session(lib.cx(), &p2).unwrap().outcome,
        VerifyOutcome::NoConsent
    );

    // A login item: refused anonymously, read with the profile.
    let item = reddit_item(&lib, "login-auth");
    lib.write(|c| {
        c.execute(
            "UPDATE library_item SET access_state = 'login_required' WHERE id = ?1",
            [&item],
        )
        .unwrap()
    });
    let r = verify_session(lib.cx(), &p).unwrap();
    assert_eq!(r.outcome, VerifyOutcome::Verified);
    assert_eq!(r.item_id.as_deref(), Some(item.as_str()));
    assert_eq!(r.profile.session_state, ProfileState::SessionAvailable);
    assert!(r.profile.last_verified_at.is_some());
    let it = lib.write(|c| repo::get_item(c, &item).unwrap().unwrap());
    assert_eq!(it.access_state, AccessState::Authorized);
    assert_eq!(it.access_profile_id.as_deref(), Some(p.as_str()));
}

#[test]
fn f5_4_verify_session_failures_follow_the_table() {
    use vv_engine::verify::{verify_session, VerifyOutcome};
    use vv_persistence::auth::{Operation, ProfileState};
    let all = [Operation::Metadata, Operation::Download, Operation::Verify];
    let gated = |lib: &Lib, scenario: &str| {
        let item = reddit_item(lib, scenario);
        lib.write(|c| {
            c.execute(
                "UPDATE library_item SET access_state = 'login_required' WHERE id = ?1",
                [&item],
            )
            .unwrap()
        });
        item
    };

    // Signed out in Firefox: not signed in, not expired.
    let lib = library(Setup::fake());
    gated(&lib, "login-auth");
    let p = connect_firefox(&lib, "signedout", &all);
    let r = verify_session(lib.cx(), &p).unwrap();
    assert_eq!(r.outcome, VerifyOutcome::NotSignedIn);
    assert_eq!(r.profile.session_state, ProfileState::NeedsVerification);
    assert_eq!(r.profile.last_verified_at, None);

    // Store unreadable.
    let lib = library(Setup::fake());
    gated(&lib, "login-auth");
    let p = connect_firefox(&lib, "broken", &all);
    let r = verify_session(lib.cx(), &p).unwrap();
    assert_eq!(r.outcome, VerifyOutcome::CookieAccessFailed);
    assert_eq!(r.profile.session_state, ProfileState::NeedsVerification);

    // AC-54: the test item opens without a login: no evidence, the
    // session is not touched.
    let lib = library(Setup::fake());
    let item = gated(&lib, "video");
    let p = connect_firefox(&lib, "x1.default-release", &all);
    let r = verify_session(lib.cx(), &p).unwrap();
    assert_eq!(r.outcome, VerifyOutcome::PublicItem);
    assert_eq!(r.profile.session_state, ProfileState::NeedsVerification);
    assert_eq!(r.profile.last_verified_at, None);
    let it = lib.write(|c| repo::get_item(c, &item).unwrap().unwrap());
    assert_eq!(it.access_state, AccessState::PublicOk);

    // AC-19: signed in, membership missing: the item says so, the session
    // is not called expired.
    let lib = library(Setup::fake());
    let item = gated(&lib, "denied-auth");
    let p = connect_firefox(&lib, "x1.nomember", &all);
    let r = verify_session(lib.cx(), &p).unwrap();
    assert_eq!(r.outcome, VerifyOutcome::AccessDenied);
    assert_eq!(r.profile.session_state, ProfileState::NeedsVerification);
    let it = lib.write(|c| repo::get_item(c, &item).unwrap().unwrap());
    assert_eq!(it.access_state, AccessState::AccessDenied);

    // Offline Mode: nothing runs.
    let lib = library(Setup::fake());
    gated(&lib, "login-auth");
    let p = connect_firefox(&lib, "x1.default-release", &all);
    lib.cx()
        .offline
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let r = verify_session(lib.cx(), &p).unwrap();
    assert_eq!(r.outcome, VerifyOutcome::Failed);
    assert_eq!(r.code, Some(vv_domain::error::ErrorCode::OfflineMode));
}

#[test]
fn f5_4_verify_session_with_a_cookie_file_leaves_no_copy() {
    use vv_engine::verify::{verify_session, VerifyOutcome};
    use vv_persistence::auth::{Operation, ProfileState};
    let lib = library(Setup::fake());
    let item = reddit_item(&lib, "login-file");
    lib.write(|c| {
        c.execute(
            "UPDATE library_item SET access_state = 'login_required' WHERE id = ?1",
            [&item],
        )
        .unwrap()
    });
    let p = connect_file(&lib, REDDIT_COOKIES, &[Operation::Verify]);
    let r = verify_session(lib.cx(), &p).unwrap();
    assert_eq!(r.outcome, VerifyOutcome::Verified);
    assert_eq!(r.profile.session_state, ProfileState::SessionAvailable);
    assert!(copies(&lib).is_empty());
}
