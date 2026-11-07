//! STATE.md F1.4/F1.5 against the pinned ffprobe/FFmpeg (`tools/bin`, from
//! `scripts/fetch-tools.mjs`). Covers AC-04, AC-34, AC-35, AC-47 and AC-59.
//! Without the tools the tests are skipped locally and fail in CI.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use vv_domain::error::ErrorCode;
use vv_import::import::{import_paths, FileOutcome, ImportOptions};
use vv_import::relink::{relink, RelinkError};
use vv_import::roots::check_storage;
use vv_import::thumbnail::{ensure_cache_root, generate_missing};
use vv_import::MediaTools;
use vv_persistence::library::repo;
use vv_persistence::library::search::{search, SearchQuery};
use vv_persistence::library::*;
use vv_persistence::{Database, OpenOptions};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn tools() -> Option<MediaTools> {
    let m: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("tools/manifest.json")).unwrap(),
    )
    .unwrap();
    let build = m["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "ffmpeg")
        .unwrap()["build"]
        .as_str()
        .unwrap()
        .to_owned();
    match MediaTools::from_tools_dir(&repo_root().join("tools/bin"), build) {
        Some(t) => Some(t),
        None if std::env::var_os("CI").is_some() => {
            panic!("pinned ffprobe/ffmpeg missing in CI: run scripts/fetch-tools.mjs")
        }
        None => {
            eprintln!("skipped: no tools/bin (run node scripts/fetch-tools.mjs)");
            None
        }
    }
}

fn media(name: &str) -> PathBuf {
    repo_root().join("tests/fixtures/media").join(name)
}

fn copy(name: &str, to: &Path) -> PathBuf {
    std::fs::create_dir_all(to).unwrap();
    let dst = to.join(name);
    std::fs::copy(media(name), &dst).unwrap();
    dst
}

/// Every file under `dir` with its bytes, to prove import only reads.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut v = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            v.extend(snapshot(&p));
        } else {
            v.push((p.clone(), std::fs::read(&p).unwrap()));
        }
    }
    v.sort();
    v
}

struct Env {
    _tmp: tempfile::TempDir,
    dir: PathBuf,
    db: Database,
    clock: FixedClock,
    tools: MediaTools,
}

fn env() -> Option<Env> {
    let tools = tools()?;
    let tmp = tempfile::tempdir().unwrap();
    let dir = vv_import::fsutil::canonical(tmp.path()).unwrap();
    let db = Database::open(&dir.join("library.sqlite"), &OpenOptions::default()).unwrap();
    Some(Env {
        _tmp: tmp,
        dir,
        db,
        clock: FixedClock::new(1_762_500_000_000),
        tools,
    })
}

fn no_cancel() -> AtomicBool {
    AtomicBool::new(false)
}

#[test]
fn ac04_valid_and_broken_files_from_one_folder() {
    let Some(mut e) = env() else { return };
    let videos = e.dir.join("Videos");
    copy("mp4-h264-aac.mp4", &videos);
    copy("webm-vp9-opus.webm", &videos);
    std::fs::write(videos.join("broken.mp4"), b"not a video at all").unwrap();
    let mkv = std::fs::read(media("mkv-h264-aac.mkv")).unwrap();
    std::fs::write(videos.join("truncated.mkv"), &mkv[..300]).unwrap();
    std::fs::write(videos.join("notes.txt"), b"hello").unwrap();
    std::fs::create_dir_all(videos.join("sub")).unwrap();
    copy("mov-h264-aac.mov", &videos.join("sub"));
    let before = snapshot(&videos);

    let r = import_paths(
        e.db.writer(),
        &e.clock,
        &e.tools,
        std::slice::from_ref(&videos),
        &ImportOptions::default(),
        &no_cancel(),
    )
    .unwrap();
    assert_eq!(
        (r.imported(), r.rejected(), r.ignored, r.canceled),
        (2, 2, 1, false),
        "{r:#?}"
    );
    for f in &r.files {
        let name = f.path.file_name().unwrap().to_str().unwrap();
        match (name, &f.outcome) {
            ("broken.mp4" | "truncated.mkv", FileOutcome::Rejected { code }) => {
                assert_eq!(*code, ErrorCode::MediaCorrupt, "{name}")
            }
            (
                "mp4-h264-aac.mp4" | "webm-vp9-opus.webm",
                FileOutcome::Imported { duplicate_of, .. },
            ) => assert!(duplicate_of.is_empty()),
            o => panic!("{o:?}"),
        }
    }
    assert_eq!(
        snapshot(&videos),
        before,
        "sources unchanged, nothing written next to them"
    );

    let c = e.db.writer();
    let mp4 = r
        .files
        .iter()
        .find(|f| f.path.ends_with("mp4-h264-aac.mp4"))
        .unwrap();
    let FileOutcome::Imported {
        item_id, asset_id, ..
    } = &mp4.outcome
    else {
        unreachable!()
    };
    let it = repo::get_item(c, item_id).unwrap().unwrap();
    assert_eq!(
        (it.kind, it.source_platform, it.canonical_url.as_deref()),
        (ItemKind::LocalVideo, SourcePlatform::Local, None)
    );
    assert_eq!(
        (it.title.as_deref(), it.duration_ms, it.metadata_status),
        (Some("mp4-h264-aac"), Some(3000), MetadataStatus::Ready)
    );
    let a = repo::get_media_asset(c, asset_id).unwrap().unwrap();
    assert_eq!(
        (a.storage_kind, a.storage_ref.as_str()),
        (StorageKind::ExternalReference, "mp4-h264-aac.mp4")
    );
    assert_eq!(
        (
            a.container.as_deref(),
            a.mime_type.as_deref(),
            a.video_codec.as_deref()
        ),
        (Some("mp4"), Some("video/mp4"), Some("h264"))
    );
    assert_eq!(
        (a.width, a.height, a.size_bytes),
        (Some(320), Some(180), Some(81_339))
    );
    assert_eq!(
        (
            a.hash_algorithm.as_deref(),
            a.content_hash.as_ref().map(String::len)
        ),
        (Some("sha256"), Some(64))
    );
    assert_eq!(a.integrity_state, IntegrityState::Verified);
    let tracks: serde_json::Value =
        serde_json::from_str(a.audio_tracks.as_deref().unwrap()).unwrap();
    assert_eq!(tracks["tracks"][0]["codec"], "aac");
    assert_eq!(tracks["tracks"][0]["channels"], 2);
    let root = repo::get_storage_root(c, &a.storage_root_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        (root.kind, Path::new(&root.path)),
        (StorageRootKind::ExternalImport, videos.as_path())
    );
    // Searchable right away (SEA-05), with or without a thumbnail (IMP-06).
    assert_eq!(
        search(
            c,
            &SearchQuery {
                text: "webm vp9".into(),
                ..Default::default()
            }
        )
        .unwrap()
        .total,
        1
    );

    // Subfolders only when asked; the folder's root is reused.
    let r2 = import_paths(
        c,
        &e.clock,
        &e.tools,
        std::slice::from_ref(&videos),
        &ImportOptions {
            recursive: true,
            hash: true,
        },
        &no_cancel(),
    )
    .unwrap();
    let mov: Vec<_> = r2
        .files
        .iter()
        .filter(|f| matches!(f.outcome, FileOutcome::Imported { .. }))
        .collect();
    assert_eq!(mov.len(), 1);
    assert!(mov[0].path.ends_with("sub/mov-h264-aac.mov"));
    assert_eq!(
        r2.files
            .iter()
            .filter(|f| matches!(f.outcome, FileOutcome::AlreadyImported { .. }))
            .count(),
        2
    );
    assert_eq!(repo::list_storage_roots(c).unwrap().len(), 1);
    let movit = repo::get_media_asset(
        c,
        match &mov[0].outcome {
            FileOutcome::Imported { asset_id, .. } => asset_id,
            _ => unreachable!(),
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        (
            movit.storage_ref.as_str(),
            movit.container.as_deref(),
            movit.mime_type.as_deref()
        ),
        ("sub/mov-h264-aac.mov", Some("mov"), Some("video/quicktime"))
    );
}

#[test]
fn ac34_same_content_at_two_paths_is_a_candidate_not_a_merge() {
    let Some(mut e) = env() else { return };
    let a = copy("mp4-h264-aac.mp4", &e.dir.join("a"));
    let b = copy("mp4-h264-aac.mp4", &e.dir.join("b"));
    let c = e.db.writer();
    let r = import_paths(
        c,
        &e.clock,
        &e.tools,
        &[a.clone(), b],
        &ImportOptions::default(),
        &no_cancel(),
    )
    .unwrap();
    let ids: Vec<(String, Vec<String>)> = r
        .files
        .iter()
        .map(|f| match &f.outcome {
            FileOutcome::Imported {
                item_id,
                duplicate_of,
                ..
            } => (item_id.clone(), duplicate_of.clone()),
            o => panic!("{o:?}"),
        })
        .collect();
    assert_eq!(ids.len(), 2, "two items, nothing merged");
    assert_eq!(ids[1].1, vec![ids[0].0.clone()]);
    let d = repo::open_duplicates_for_item(c, &ids[0].0).unwrap();
    assert_eq!((d.len(), d[0].reason), (1, DuplicateReason::ContentHash));
    // The same path again is recognized, not a third item.
    let again = import_paths(
        c,
        &e.clock,
        &e.tools,
        &[a],
        &ImportOptions::default(),
        &no_cancel(),
    )
    .unwrap();
    assert_eq!(
        again.files[0].outcome,
        FileOutcome::AlreadyImported {
            item_id: ids[0].0.clone()
        }
    );
    assert_eq!(repo::count_items(c).unwrap(), 2);
}

#[test]
fn ac59_manifests_pointing_at_the_network_or_outside_files_are_rejected() {
    let Some(mut e) = env() else { return };
    let canary = TcpListener::bind("127.0.0.1:0").unwrap();
    canary.set_nonblocking(true).unwrap();
    let port = canary.local_addr().unwrap().port();
    let outside = copy("mp4-h264-aac.mp4", &e.dir.join("outside"));
    let import = e.dir.join("import");
    std::fs::create_dir_all(&import).unwrap();
    let hls = import.join("list.m3u8");
    std::fs::write(&hls, format!("#EXTM3U\n#EXT-X-TARGETDURATION:3\n#EXTINF:3,\nhttp://127.0.0.1:{port}/seg.ts\n#EXTINF:3,\n{}\n#EXT-X-ENDLIST\n", outside.display())).unwrap();
    let concat = import.join("list.ffconcat");
    std::fs::write(
        &concat,
        format!(
            "ffconcat version 1.0\nfile '{}'\nfile 'http://127.0.0.1:{port}/x.mp4'\n",
            outside.display()
        ),
    )
    .unwrap();
    // A manifest renamed to look like a video is refused as well.
    let disguised = import.join("movie.mp4");
    std::fs::copy(&hls, &disguised).unwrap();

    let c = e.db.writer();
    let r = import_paths(
        c,
        &e.clock,
        &e.tools,
        &[hls, concat, disguised],
        &ImportOptions::default(),
        &no_cancel(),
    )
    .unwrap();
    assert_eq!(r.files.len(), 3);
    for f in &r.files {
        match &f.outcome {
            FileOutcome::Rejected { code } => assert!(
                *code == ErrorCode::InputRejected || *code == ErrorCode::MediaCorrupt,
                "{}: {code:?}",
                f.path.display()
            ),
            o => panic!("{o:?}"),
        }
    }
    assert!(r.files.iter().any(|f| f.outcome
        == FileOutcome::Rejected {
            code: ErrorCode::InputRejected
        }));
    assert!(canary.accept().is_err(), "no connection reached the canary");
    assert_eq!(repo::count_items(c).unwrap(), 0);
}

#[test]
fn ac35_moved_file_is_missing_then_relinked_with_notes_kept() {
    let Some(mut e) = env() else { return };
    let lib = e.dir.join("lib");
    let file = copy("mkv-h264-aac.mkv", &lib);
    let c = e.db.writer();
    let r = import_paths(
        c,
        &e.clock,
        &e.tools,
        std::slice::from_ref(&lib),
        &ImportOptions::default(),
        &no_cancel(),
    )
    .unwrap();
    let FileOutcome::Imported {
        item_id, asset_id, ..
    } = r.files[0].outcome.clone()
    else {
        panic!()
    };
    repo::set_notes(c, &item_id, 1, "42. dakikada önemli kısım", 2).unwrap();

    std::fs::create_dir_all(lib.join("moved")).unwrap();
    let moved = lib.join("moved").join("renamed.mkv");
    std::fs::rename(&file, &moved).unwrap();
    let chk = check_storage(c, &e.clock).unwrap();
    assert_eq!((chk.missing, chk.verified), (1, 0));
    assert_eq!(
        repo::get_media_asset(c, &asset_id)
            .unwrap()
            .unwrap()
            .integrity_state,
        IntegrityState::Missing
    );
    // The item stays (IMP-07).
    assert_eq!(
        repo::get_item(c, &item_id).unwrap().unwrap().user_notes,
        "42. dakikada önemli kısım"
    );

    // A different file is refused unless the user accepts it.
    let other = copy("mp4-h264-aac.mp4", &e.dir.join("elsewhere"));
    assert!(matches!(
        relink(c, &e.clock, &e.tools, &asset_id, &other, false),
        Err(RelinkError::ContentDiffers)
    ));
    relink(c, &e.clock, &e.tools, &asset_id, &moved, false).unwrap();
    let a = repo::get_media_asset(c, &asset_id).unwrap().unwrap();
    assert_eq!(
        (a.storage_ref.as_str(), a.integrity_state),
        ("moved/renamed.mkv", IntegrityState::Verified)
    );
    let it = repo::get_item(c, &item_id).unwrap().unwrap();
    assert_eq!(
        (it.user_notes.as_str(), it.id.as_str()),
        ("42. dakikada önemli kısım", item_id.as_str())
    );
    assert_eq!(check_storage(c, &e.clock).unwrap().verified, 1);
}

#[test]
fn ac47_unplugged_root_is_disconnected_not_missing_and_recovers() {
    let Some(mut e) = env() else { return };
    let drive = e.dir.join("E-drive");
    let videos = drive.join("Videos");
    copy("mp4-h264-aac.mp4", &videos);
    copy("webm-vp8-vorbis.webm", &videos);
    let c = e.db.writer();
    let r = import_paths(
        c,
        &e.clock,
        &e.tools,
        std::slice::from_ref(&videos),
        &ImportOptions::default(),
        &no_cancel(),
    )
    .unwrap();
    assert_eq!(r.imported(), 2);
    let root_id = repo::list_storage_roots(c).unwrap()[0].id.clone();

    // "Unplug": the whole drive path disappears.
    let away = e.dir.join("E-drive-unplugged");
    std::fs::rename(&drive, &away).unwrap();
    e.clock.advance(1_000);
    let chk = check_storage(c, &e.clock).unwrap();
    assert_eq!(chk.roots[0].after, StorageRootStatus::Disconnected);
    assert_eq!((chk.missing, chk.unavailable), (0, 2));
    let assets = repo::assets_in_root(c, &root_id).unwrap();
    assert!(
        assets
            .iter()
            .all(|a| a.integrity_state == IntegrityState::Verified),
        "no asset becomes missing one by one"
    );
    // Even a direct write cannot mark them missing while the root is away (DATA-09).
    assert!(!repo::set_asset_integrity(c, &assets[0].id, IntegrityState::Missing).unwrap());

    // "Plug back in": the next check verifies the root and its assets.
    std::fs::rename(&away, &drive).unwrap();
    e.clock.advance(1_000);
    let chk = check_storage(c, &e.clock).unwrap();
    assert_eq!(
        (chk.roots[0].before, chk.roots[0].after),
        (
            StorageRootStatus::Disconnected,
            StorageRootStatus::Available
        )
    );
    assert_eq!((chk.verified, chk.missing), (2, 0));
    assert_eq!(
        repo::get_storage_root(c, &root_id)
            .unwrap()
            .unwrap()
            .last_verified_at,
        Some(e.clock.now_ms())
    );
}

#[test]
fn thumbnails_are_derived_assets_in_the_cache_root() {
    let Some(mut e) = env() else { return };
    let videos = e.dir.join("v");
    copy("mp4-h264-aac.mp4", &videos);
    copy("mkv-vp9-opus.mkv", &videos);
    let c = e.db.writer();
    import_paths(
        c,
        &e.clock,
        &e.tools,
        std::slice::from_ref(&videos),
        &ImportOptions::default(),
        &no_cancel(),
    )
    .unwrap();
    let cache = ensure_cache_root(c, &e.clock, &e.dir.join("cache")).unwrap();
    let done = generate_missing(c, &e.clock, &e.tools, &cache, 10).unwrap();
    assert_eq!(done.len(), 2);
    for (item_id, r) in done {
        let thumb_id = r.unwrap_or_else(|err| panic!("{err:?}"));
        let it = repo::get_item(c, &item_id).unwrap().unwrap();
        assert_eq!(it.thumbnail_asset_id.as_deref(), Some(thumb_id.as_str()));
        let t = repo::get_media_asset(c, &thumb_id).unwrap().unwrap();
        assert_eq!(
            (t.role, t.storage_root_id.as_str(), t.mime_type.as_deref()),
            (AssetRole::Thumbnail, cache.id.as_str(), Some("image/jpeg"))
        );
        let d: serde_json::Value = serde_json::from_str(t.derivation.as_deref().unwrap()).unwrap();
        assert_eq!(
            (d["generator"].as_str(), d["config"]["width"].as_u64()),
            (Some("ffmpeg"), Some(320))
        );
        let bytes = std::fs::read(vv_import::fsutil::resolve_ref(
            Path::new(&cache.path),
            &t.storage_ref,
        ))
        .unwrap();
        assert_eq!(&bytes[..3], &[0xFF, 0xD8, 0xFF], "JPEG");
    }
    assert!(generate_missing(c, &e.clock, &e.tools, &cache, 10)
        .unwrap()
        .is_empty());
}

#[test]
fn cancel_keeps_finished_files_and_stops() {
    let Some(mut e) = env() else { return };
    let videos = e.dir.join("v");
    copy("mp4-h264-aac.mp4", &videos);
    let r = import_paths(
        e.db.writer(),
        &e.clock,
        &e.tools,
        &[videos],
        &ImportOptions::default(),
        &AtomicBool::new(true),
    )
    .unwrap();
    assert!(r.canceled);
    assert_eq!(repo::count_items(e.db.writer()).unwrap(), 0);
}

#[cfg(unix)]
#[test]
fn links_in_folders_are_not_followed() {
    let Some(mut e) = env() else { return };
    let videos = e.dir.join("v");
    let outside = copy("mp4-h264-aac.mp4", &e.dir.join("outside"));
    std::fs::create_dir_all(&videos).unwrap();
    std::os::unix::fs::symlink(&outside, videos.join("link.mp4")).unwrap();
    std::os::unix::fs::symlink(&videos, videos.join("loop")).unwrap();
    let r = import_paths(
        e.db.writer(),
        &e.clock,
        &e.tools,
        &[videos],
        &ImportOptions {
            recursive: true,
            hash: false,
        },
        &no_cancel(),
    )
    .unwrap();
    assert_eq!(r.imported(), 0);
    assert!(r.files.iter().all(|f| matches!(
        f.outcome,
        FileOutcome::Skipped {
            reason: vv_import::fsutil::SkipReason::Link
        }
    )));
}
