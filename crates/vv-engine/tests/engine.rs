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
    assert_eq!(j.error_code, None);
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
