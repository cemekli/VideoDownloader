//! The pinned yt-dlp, Deno, FFmpeg and ffprobe from `tools/bin` (STATE.md
//! F3.2, F3.3, F3.4, F3.6). Skipped locally without the tools; CI fetches
//! them first (`scripts/fetch-tools.mjs`).
//!
//! - Tool health check: all four pass; a changed byte, a missing file and
//!   (Unix) a file the OS refuses to run each get their own code (AC-25,
//!   `TOOL_MISSING`, `TOOL_BLOCKED`).
//! - AC-29 through the engine: a page that redirects to a canary server,
//!   to the cloud metadata address or to an IPv4-mapped loopback ends
//!   `URL_BLOCKED` and the canary gets no connection.
//! - A real download from a local server: metadata with thumbnail, then
//!   download, verify and publish.

mod common;

use common::*;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use vv_engine::formats::PresetId;
use vv_engine::toolset::{check_tools, ToolPaths, ToolState};
use vv_engine::{download, metadata};
use vv_persistence::jobs::JobStatus;
use vv_persistence::library::repo::{self, LibraryItem};
use vv_persistence::library::*;

fn manifest() -> vv_tools::Manifest {
    vv_tools::Manifest::parse(
        &std::fs::read_to_string(repo_root().join("tools/manifest.json")).unwrap(),
    )
    .unwrap()
}

fn real() -> Option<ToolPaths> {
    let p = real_tool_paths();
    if p.is_none() {
        media_tools(); // asserts in CI, prints the skip note locally
    }
    p
}

#[test]
fn health_check_passes_the_pinned_tools_and_names_each_failure() {
    let Some(paths) = real() else { return };
    let m = manifest();
    let report = check_tools(&m, &paths, 1);
    for t in &report.tools {
        // whisper.cpp publishes no Linux build (ADR-0028).
        if t.id == "whisper-cli" && cfg!(not(windows)) {
            assert_eq!(t.state, ToolState::Unavailable, "{t:?}");
            assert!(t.optional);
            continue;
        }
        assert_eq!(t.state, ToolState::Ok, "{t:?}");
        assert!(t.reported_version.is_some());
    }
    assert!(report.ready_for_downloads().is_ok());
    let json = serde_json::to_string(&report).unwrap();
    assert!(
        !json.contains("tools/bin") && !json.contains("tools\\bin"),
        "no paths: {json}"
    );

    let d = tempfile::tempdir().unwrap();
    // One byte changed: never run (AC-25).
    let mut bytes = std::fs::read(&paths.ytdlp).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    let tampered = d.path().join("yt-dlp-tampered");
    std::fs::write(&tampered, &bytes).unwrap();
    let missing = d.path().join("ffprobe-missing");
    let mut broken = paths.clone();
    broken.ytdlp = tampered;
    broken.ffprobe = missing;
    #[cfg(unix)]
    {
        // The right bytes, but the OS refuses to start it.
        use std::os::unix::fs::PermissionsExt;
        let deno = d.path().join("deno-noexec");
        std::fs::copy(&paths.deno, &deno).unwrap();
        std::fs::set_permissions(&deno, std::fs::Permissions::from_mode(0o644)).unwrap();
        broken.deno = deno;
    }
    let report = check_tools(&m, &broken, 2);
    let state = |id| report.health(id).unwrap().state;
    assert_eq!(state(vv_tools::ToolId::YtDlp), ToolState::ChecksumMismatch);
    assert_eq!(
        report.health(vv_tools::ToolId::YtDlp).unwrap().code,
        Some(vv_domain::error::ErrorCode::ChecksumMismatch)
    );
    assert_eq!(state(vv_tools::ToolId::Ffprobe), ToolState::Missing);
    #[cfg(unix)]
    assert_eq!(state(vv_tools::ToolId::Deno), ToolState::Blocked);
    assert_eq!(state(vv_tools::ToolId::Ffmpeg), ToolState::Ok);
    assert_eq!(
        report.ready_for_downloads(),
        Err(vv_domain::error::ErrorCode::ChecksumMismatch)
    );

    // whisper-cli with one of its DLLs changed is never started, and that
    // does not stop downloads (ADR-0028).
    if cfg!(windows) {
        let wdir = d.path().join("whisper-cli");
        std::fs::create_dir_all(&wdir).unwrap();
        for e in std::fs::read_dir(paths.whisper.parent().unwrap())
            .unwrap()
            .flatten()
        {
            std::fs::copy(e.path(), wdir.join(e.file_name())).unwrap();
        }
        let dll = wdir.join("ggml-base.dll");
        let mut b = std::fs::read(&dll).unwrap();
        b[100] ^= 0xFF;
        std::fs::write(&dll, b).unwrap();
        let mut w = paths.clone();
        w.whisper = wdir.join("whisper-cli.exe");
        let report = check_tools(&m, &w, 3);
        assert_eq!(
            report.health(vv_tools::ToolId::WhisperCli).unwrap().state,
            ToolState::ChecksumMismatch
        );
        assert!(report
            .health(vv_tools::ToolId::WhisperCli)
            .unwrap()
            .reported_version
            .is_none());
        assert!(report.ready_for_downloads().is_ok());
    }
}

type Route = (u16, Vec<(String, String)>, Vec<u8>);

/// A tiny HTTP/1.1 server: one request per connection.
struct Server {
    addr: SocketAddr,
    hits: Arc<AtomicUsize>,
}

fn serve(routes: impl Fn(&str) -> Route + Send + Sync + 'static) -> Server {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let h = hits.clone();
    let routes = Arc::new(routes);
    std::thread::spawn(move || {
        for c in l.incoming().flatten() {
            h.fetch_add(1, Ordering::SeqCst);
            let routes = routes.clone();
            std::thread::spawn(move || handle(c, &*routes));
        }
    });
    Server { addr, hits }
}

fn handle(mut c: TcpStream, routes: &dyn Fn(&str) -> Route) {
    let mut buf = Vec::new();
    let mut b = [0u8; 4096];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        match c.read(&mut b) {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&b[..n]),
        }
    }
    let head = String::from_utf8_lossy(&buf);
    let path = head.split_whitespace().nth(1).unwrap_or("/").to_owned();
    let head_only = head.starts_with("HEAD ");
    let (status, headers, body) = routes(&path);
    let mut resp = format!(
        "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in headers {
        resp.push_str(&format!("{k}: {v}\r\n"));
    }
    resp.push_str("\r\n");
    let _ = c.write_all(resp.as_bytes());
    if !head_only {
        let _ = c.write_all(&body);
    }
}

fn remote_item(lib: &Lib, url: &str) -> String {
    let now = lib.clock.now_ms();
    let mut it = LibraryItem::new(ItemKind::Bookmark, SourcePlatform::Generic, now);
    it.canonical_url = Some(url.to_owned());
    it.original_url = Some(url.to_owned());
    lib.write(|c| repo::insert_item(c, &it).unwrap());
    it.id
}

fn real_lib(paths: &ToolPaths, test_target: SocketAddr) -> Lib {
    let policy = vv_netguard::Policy {
        test_targets: vec![test_target],
        ..Default::default()
    };
    library(Setup {
        ytdlp: paths.ytdlp.clone(),
        media: media_tools(),
        policy,
        whisper: None,
    })
}

#[test]
fn redirects_to_private_targets_end_url_blocked_and_reach_nothing() {
    let Some(paths) = real() else { return };
    let canary = serve(|_| (200, vec![], b"canary".to_vec()));
    let canary_addr = canary.addr;
    let origin = serve(move |path| match path {
        "/to-canary" => (
            302,
            vec![("Location".into(), format!("http://{canary_addr}/x"))],
            vec![],
        ),
        "/to-metadata" => (
            302,
            vec![(
                "Location".into(),
                "http://169.254.169.254/latest/meta-data/".into(),
            )],
            vec![],
        ),
        "/to-mapped" => (
            302,
            vec![(
                "Location".into(),
                format!("http://[::ffff:127.0.0.1]:{}/x", canary_addr.port()),
            )],
            vec![],
        ),
        _ => (404, vec![], vec![]),
    });
    let lib = real_lib(&paths, origin.addr);
    for path in ["to-canary", "to-metadata", "to-mapped"] {
        let item = remote_item(&lib, &format!("http://{}/{path}", origin.addr));
        let now = lib.clock.now_ms();
        let job = lib.write(|c| metadata::enqueue(c, &item, now).unwrap().id);
        lib.run();
        let j = lib.job(&job);
        assert_eq!(j.status, JobStatus::Failed, "{path}");
        assert_eq!(j.error_code.as_deref(), Some("URL_BLOCKED"), "{path}");
    }
    assert!(origin.hits.load(Ordering::SeqCst) >= 3);
    assert_eq!(
        canary.hits.load(Ordering::SeqCst),
        0,
        "AC-29: the canary was reached"
    );

    // Loopback, metadata and mapped-loopback URLs themselves never start a
    // process (and are refused at capture already).
    for url in [
        "http://127.0.0.1/",
        "http://169.254.169.254/",
        "http://[::ffff:127.0.0.1]/",
    ] {
        let item = remote_item(&lib, url);
        let now = lib.clock.now_ms();
        let job = lib.write(|c| metadata::enqueue(c, &item, now).unwrap().id);
        lib.run();
        assert_eq!(
            lib.job(&job).error_code.as_deref(),
            Some("URL_BLOCKED"),
            "{url}"
        );
    }
}

#[test]
fn a_real_download_from_a_local_page_is_fetched_verified_and_playable() {
    let Some(paths) = real() else { return };
    let Some(media) = media_tools() else { return };
    let www = tempfile::tempdir().unwrap();
    let thumb: PathBuf = www.path().join("thumb.jpg");
    let ok = std::process::Command::new(&media.ffmpeg)
        .args([
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x90",
            "-frames:v",
            "1",
        ])
        .arg(&thumb)
        .status()
        .unwrap()
        .success();
    assert!(ok);
    let thumb_bytes = std::fs::read(&thumb).unwrap();
    let clip = std::fs::read(repo_root().join("tests/fixtures/media/mp4-h264-aac.mp4")).unwrap();
    let served = clip.clone();
    let origin = {
        let addr = Arc::new(std::sync::OnceLock::<SocketAddr>::new());
        let a = addr.clone();
        let s = serve(move |path| {
            let me = a.get().copied().unwrap();
            match path {
                "/page.html" => (
                    200,
                    vec![("Content-Type".into(), "text/html".into())],
                    format!("<html><head><title>Local clip</title><meta property=\"og:title\" content=\"Local clip\"><meta property=\"og:image\" content=\"http://{me}/thumb.jpg\"><meta property=\"og:video\" content=\"http://{me}/clip.mp4\"></head></html>").into_bytes(),
                ),
                "/clip.mp4" => (200, vec![("Content-Type".into(), "video/mp4".into())], served.clone()),
                "/thumb.jpg" => (200, vec![("Content-Type".into(), "image/jpeg".into())], thumb_bytes.clone()),
                _ => (404, vec![], vec![]),
            }
        });
        addr.set(s.addr).unwrap();
        s
    };
    let lib = real_lib(&paths, origin.addr);
    let item = remote_item(&lib, &format!("http://{}/page.html", origin.addr));
    let now = lib.clock.now_ms();
    let job = lib.write(|c| metadata::enqueue(c, &item, now).unwrap().id);
    lib.run();
    let j = lib.job(&job);
    assert_eq!(
        j.status,
        JobStatus::Succeeded,
        "{:?} {:?}",
        j.error_code,
        j.redacted_error_message
    );
    let it = lib.write(|c| repo::get_item(c, &item).unwrap().unwrap());
    assert_eq!(it.title.as_deref(), Some("Local clip"));
    assert!(
        it.thumbnail_asset_id.is_some(),
        "remote thumbnail through the guard"
    );

    let job = lib.write(|c| download::enqueue(c, &item, PresetId::Best, now).unwrap().id);
    lib.run();
    let j = lib.job(&job);
    assert_eq!(
        j.status,
        JobStatus::Succeeded,
        "{:?} {:?}",
        j.error_code,
        j.redacted_error_message
    );
    let a = lib
        .write(|c| repo::assets_for_item(c, &item).unwrap())
        .into_iter()
        .find(|a| a.role == AssetRole::Original)
        .unwrap();
    assert_eq!(a.video_codec.as_deref(), Some("h264"));
    assert_eq!(a.size_bytes, Some(clip.len() as i64));
    let root = lib.write(|c| {
        repo::get_storage_root(c, &a.storage_root_id)
            .unwrap()
            .unwrap()
    });
    let f = vv_import::fsutil::resolve_ref(Path::new(&root.path), &a.storage_ref);
    assert_eq!(
        std::fs::read(f).unwrap(),
        clip,
        "the published file is the served file"
    );
}

/// F6.2: the real yt-dlp lists an HTML5 `<track>` as an uploader caption
/// track and writes it with the flags of `Mode::Captions`; it becomes a
/// `source_captions` transcript.
#[test]
fn real_site_captions_from_a_local_page_become_a_transcript() {
    let Some(paths) = real() else { return };
    let clip = std::fs::read(repo_root().join("tests/fixtures/media/mp4-h264-aac.mp4")).unwrap();
    let vtt = "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nYerel <i>altyazı</i>\n\n00:00:01.000 --> 00:00:02.000\nikinci satır\n";
    let origin = serve(move |path| {
        match path {
        "/page.html" => (
            200,
            vec![("Content-Type".into(), "text/html".into())],
            b"<html><head><title>Captioned clip</title></head><body><video src=\"/clip.mp4\" controls><track kind=\"captions\" srclang=\"tr\" label=\"T\xc3\xbcrk\xc3\xa7e\" src=\"/caps.tr.vtt\"></video></body></html>".to_vec(),
        ),
        "/clip.mp4" => (200, vec![("Content-Type".into(), "video/mp4".into())], clip.clone()),
        "/caps.tr.vtt" => (200, vec![("Content-Type".into(), "text/vtt".into())], vtt.as_bytes().to_vec()),
        _ => (404, vec![], vec![]),
    }
    });
    let lib = real_lib(&paths, origin.addr);
    let item = remote_item(&lib, &format!("http://{}/page.html", origin.addr));
    let now = lib.clock.now_ms();
    let job = lib.write(|c| vv_engine::captions::enqueue(c, &item, now).unwrap().id);
    lib.run();
    let j = lib.job(&job);
    assert_eq!(
        j.status,
        JobStatus::Succeeded,
        "{:?} {:?}",
        j.error_code,
        j.redacted_error_message
    );
    let ts = lib.write(|c| vv_persistence::transcripts::list_for_item(c, &item).unwrap());
    assert_eq!(ts.len(), 1, "one transcript from the page's track");
    assert_eq!(ts[0].origin.language, "tr");
    let segs = lib.write(|c| vv_persistence::transcripts::segments(c, &ts[0].id).unwrap());
    let got: Vec<_> = segs
        .iter()
        .map(|s| (s.start_ms, s.end_ms, s.text.as_str()))
        .collect();
    assert_eq!(
        got,
        vec![(0, 1_000, "Yerel altyazı"), (1_000, 2_000, "ikinci satır")]
    );
}
