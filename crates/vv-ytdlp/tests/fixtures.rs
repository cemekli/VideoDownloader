//! Integration tests on real yt-dlp output (STATE.md F0.6, SPEC §25.2).
//!
//! `tests/fixtures/ytdlp/runs/` was captured by
//! `scripts/capture-ytdlp-fixtures.mjs` from yt-dlp 2026.10.26 against a local
//! server; `errors-source.json` holds error lines copied from yt-dlp's source.
//! No test here runs yt-dlp.

use serde_json::Value;
use std::path::PathBuf;
use vv_domain::error::ErrorCode;
use vv_ytdlp::args;
use vv_ytdlp::lines::LineSplitter;
use vv_ytdlp::metadata::{self, Extracted};
use vv_ytdlp::outcome::{Exit, RunObserver, RunOutcome, Stream};
use vv_ytdlp::progress::{DownloadProgress, DownloadStatus, PostprocessStatus, ProgressEvent};

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/ytdlp")
}

fn json(rel: &str) -> Value {
    let text = std::fs::read_to_string(dir().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
    serde_json::from_str(&text).unwrap()
}

struct Run {
    exit: Exit,
    stdout: String,
    stderr: String,
}

fn run(name: &str) -> Run {
    let v = json(&format!("runs/{name}.json"));
    let exit = match v["exitCode"].as_i64() {
        Some(c) => Exit::Code(c as i32),
        None => Exit::Killed,
    };
    Run {
        exit,
        stdout: v["stdout"].as_str().unwrap().to_string(),
        stderr: v["stderr"].as_str().unwrap().to_string(),
    }
}

struct Replay {
    events: Vec<ProgressEvent>,
    outcome: RunOutcome,
    tail: Vec<String>,
    malformed: usize,
}

/// Feeds the captured output in small chunks, the way pipe reads arrive.
fn replay(stdout: &str, stderr: &str, exit: Exit, cancel_requested: bool) -> Replay {
    let mut obs = RunObserver::new();
    let mut events = Vec::new();
    for (stream, text) in [(Stream::Stdout, stdout), (Stream::Stderr, stderr)] {
        let mut split = LineSplitter::default();
        let mut lines = Vec::new();
        for chunk in text.as_bytes().chunks(7) {
            lines.extend(split.push(chunk));
        }
        lines.extend(split.finish());
        for line in lines {
            events.extend(obs.observe(stream, &line.text));
        }
    }
    let tail = obs.stderr_tail().map(str::to_string).collect();
    let malformed = obs.malformed_progress();
    Replay {
        events,
        outcome: obs.finish(exit, cancel_requested),
        tail,
        malformed,
    }
}

fn replay_run(name: &str, cancel_requested: bool) -> Replay {
    let r = run(name);
    replay(&r.stdout, &r.stderr, r.exit, cancel_requested)
}

fn downloads(events: &[ProgressEvent]) -> Vec<&DownloadProgress> {
    events
        .iter()
        .filter_map(|e| match e {
            ProgressEvent::Download(d) => Some(d),
            _ => None,
        })
        .collect()
}

fn postprocessors(events: &[ProgressEvent]) -> Vec<(String, PostprocessStatus)> {
    events
        .iter()
        .filter_map(|e| match e {
            ProgressEvent::Postprocess {
                postprocessor,
                status,
            } => Some((postprocessor.clone(), *status)),
            _ => None,
        })
        .collect()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn fixed_args_match_the_capture_script() {
    let a = json("args.json");
    assert_eq!(a["ytDlpVersion"], args::PINNED_VERSION);
    assert_eq!(
        json("capture-manifest.json")["ytDlpVersion"],
        args::PINNED_VERSION
    );
    assert_eq!(strings(&a["common"]), args::COMMON);
    assert_eq!(strings(&a["jsRuntime"]), args::JS_RUNTIME);
    assert_eq!(strings(&a["networkGuard"]), args::NETWORK_GUARD);
    let g = args::network_guard_args("http://vv:t@127.0.0.1:1");
    assert_eq!(g[1], "http://vv:t@127.0.0.1:1");
    assert_eq!(g.len(), args::NETWORK_GUARD.len());
    assert_eq!(strings(&a["metadata"]), args::METADATA);
    assert_eq!(strings(&a["download"]), args::DOWNLOAD);
}

#[test]
fn manifest_lists_every_captured_run() {
    let m = json("capture-manifest.json");
    let mut listed: Vec<String> = m["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["file"].as_str().unwrap().to_string())
        .collect();
    let mut on_disk: Vec<String> = std::fs::read_dir(dir().join("runs"))
        .unwrap()
        .map(|e| format!("runs/{}", e.unwrap().file_name().to_string_lossy()))
        .collect();
    listed.sort();
    on_disk.sort();
    assert_eq!(listed, on_disk);
}

#[test]
fn hls_metadata_lists_formats_without_media_urls() {
    let r = run("metadata-hls");
    assert_eq!(r.exit, Exit::Code(0));
    let Extracted::Video(v) = metadata::parse(r.stdout.as_bytes()).unwrap() else {
        panic!("not a video")
    };
    assert_eq!(v.id, "master");
    assert_eq!(v.extractor_key, "Generic");
    assert_eq!(
        v.webpage_url.as_deref(),
        Some("http://127.0.0.1:8765/master.m3u8")
    );
    let heights: Vec<_> = v.formats.iter().map(|f| f.height).collect();
    assert_eq!(heights, [Some(240), Some(480)]);
    let f = &v.formats[1];
    assert_eq!(f.format_id, "800");
    assert_eq!(f.protocol.as_deref(), Some("m3u8_native"));
    assert_eq!(f.vcodec.as_deref(), Some("avc1.64001e"));
    assert_eq!(f.acodec.as_deref(), Some("mp4a.40.2"));
    assert_eq!(f.tbr, Some(800.0));
    assert!(!v.has_drm);

    // The raw document has media URLs and headers; the parsed one must not.
    assert!(r.stdout.contains("v480.m3u8") && r.stdout.contains("User-Agent"));
    let kept = serde_json::to_string(&v).unwrap();
    for leaked in [
        "v240.m3u8",
        "v480.m3u8",
        "User-Agent",
        "http_headers",
        "manifest_url",
    ] {
        assert!(!kept.contains(leaked), "{leaked} leaked into {kept}");
    }
}

#[test]
fn direct_file_metadata() {
    let Extracted::Video(v) = metadata::parse(run("metadata-direct").stdout.as_bytes()).unwrap()
    else {
        panic!()
    };
    assert_eq!(v.id, "clip");
    assert_eq!(v.formats.len(), 1);
    assert_eq!(v.formats[0].ext.as_deref(), Some("mp4"));
    assert_eq!(
        v.duration_seconds, None,
        "generic extractor reports no duration; none is invented"
    );
    // The server sends no Last-Modified, so yt-dlp has no date either.
    assert_eq!((v.upload_date, v.timestamp), (None, None));
}

#[test]
fn playlist_is_reported_not_expanded() {
    match metadata::parse(run("metadata-playlist").stdout.as_bytes()).unwrap() {
        Extracted::Playlist {
            entry_count, title, ..
        } => {
            assert_eq!(entry_count, 2);
            assert_eq!(title.as_deref(), Some("Two clips"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn direct_download_progress_with_known_total() {
    let r = replay_run("download-direct", false);
    assert_eq!(r.outcome, RunOutcome::Succeeded);
    assert_eq!(r.malformed, 0);
    let d = downloads(&r.events);
    assert!(d.len() >= 3, "{} updates", d.len());
    let total = d[0].bytes_total.expect("Content-Length known");
    let mut last = -1.0;
    for p in &d {
        assert_eq!(p.bytes_total, Some(total));
        let pct = p.percentage.expect("total known, so % known");
        assert!(pct >= last, "percentage went back: {last} → {pct}");
        last = pct;
    }
    let end = d.last().unwrap();
    assert_eq!(end.status, DownloadStatus::Finished);
    assert_eq!((end.bytes_done, end.percentage), (total, Some(100.0)));
    assert_eq!(
        postprocessors(&r.events),
        [
            ("MoveFiles".to_string(), PostprocessStatus::Started),
            ("MoveFiles".to_string(), PostprocessStatus::Finished)
        ]
    );
}

#[test]
fn unknown_total_has_no_percentage_or_eta() {
    let r = replay_run("download-unknown-total", false);
    assert_eq!(r.outcome, RunOutcome::Succeeded);
    let d = downloads(&r.events);
    let (running, done): (Vec<&DownloadProgress>, Vec<&DownloadProgress>) = d
        .iter()
        .copied()
        .partition(|p| p.status == DownloadStatus::Downloading);
    assert!(running.len() >= 3);
    for p in &running {
        assert_eq!(
            (p.bytes_total, p.percentage, p.eta_seconds),
            (None, None, None),
            "§13.4: {p:?}"
        );
        assert!(p.bytes_done > 0);
    }
    // On finish yt-dlp reports the size it actually got.
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].bytes_total, Some(done[0].bytes_done));
}

#[test]
fn hls_download_counts_fragments_and_postprocessing() {
    let r = replay_run("download-hls", false);
    assert_eq!(r.outcome, RunOutcome::Succeeded);
    let d = downloads(&r.events);
    let running: Vec<_> = d
        .iter()
        .filter(|p| p.status == DownloadStatus::Downloading)
        .collect();
    assert!(running
        .iter()
        .all(|p| p.units_total == Some(3) && p.bytes_total.is_none()));
    assert!(running.iter().any(|p| p.bytes_total_estimate.is_some()));
    assert_eq!(running[0].percentage, Some(0.0));
    assert_eq!(running.last().unwrap().percentage, Some(100.0));
    let pp: Vec<_> = postprocessors(&r.events)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert_eq!(pp, ["FixupM3u8", "FixupM3u8", "MoveFiles", "MoveFiles"]);
}

#[test]
fn sigint_cancel_is_canceled_not_an_error() {
    let ours = replay_run("cancel-sigint", true);
    assert_eq!(ours.outcome, RunOutcome::Canceled);
    assert!(!downloads(&ours.events).is_empty());
    // Even if the cancel did not come from us, "Interrupted by user" is a cancel.
    assert_eq!(
        replay_run("cancel-sigint", false).outcome,
        RunOutcome::Canceled
    );
}

#[test]
fn killed_process_and_cut_lines() {
    let r = run("cancel-killed");
    assert_eq!(r.exit, Exit::Killed);
    assert_eq!(
        replay(&r.stdout, &r.stderr, r.exit, true).outcome,
        RunOutcome::Canceled
    );
    assert_eq!(
        replay(&r.stdout, &r.stderr, r.exit, false).outcome.code(),
        Some(ErrorCode::InternalError)
    );

    // A kill can land mid-write: the last line is then half a progress object.
    let cut = &r.stdout[..r.stdout.len() - 40];
    let partial = replay(cut, "", Exit::Killed, true);
    assert_eq!(partial.malformed, 1);
    assert_eq!(partial.outcome, RunOutcome::Canceled);
    assert_eq!(
        downloads(&partial.events).len(),
        downloads(&replay(&r.stdout, "", Exit::Killed, true).events).len() - 1
    );
}

#[test]
fn captured_errors_map_to_spec_codes() {
    let cases = [
        ("error-not-found", ErrorCode::ContentUnavailable),
        ("error-forbidden", ErrorCode::AccessDenied),
        ("error-rate-limited", ErrorCode::RateLimited),
        ("error-connection-refused", ErrorCode::NetworkUnavailable),
        ("error-unsupported", ErrorCode::ExtractorUnsupported),
        // Every fragment 403'd; yt-dlp only says the file is empty. Nothing
        // more precise is claimed (SPEC §19).
        ("error-fragments-forbidden", ErrorCode::ExtractorFailed),
        ("error-disk-full", ErrorCode::DiskFull),
    ];
    for (name, want) in cases {
        let r = replay_run(name, false);
        assert_eq!(r.outcome.code(), Some(want), "{name}");
        let RunOutcome::Failed { diagnostic, .. } = &r.outcome else {
            panic!("{name}")
        };
        assert!(diagnostic.is_some(), "{name}: no diagnostic");
    }
}

#[test]
fn source_error_lines_map_to_spec_codes() {
    let doc = json("errors-source.json");
    let cases = doc["cases"].as_array().unwrap();
    assert!(cases.len() >= 15);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let exit = Exit::Code(c["exitCode"].as_i64().unwrap() as i32);
        let r = replay("", c["stderr"].as_str().unwrap(), exit, false);
        let got = r.outcome.code().map(|c| serde_json::to_value(c).unwrap());
        assert_eq!(got.as_ref(), Some(&c["expected"]), "{name}");
    }
}

#[test]
fn canary_secrets_never_survive() {
    let doc = json("errors-source.json");
    let canary = doc["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "redaction-canary")
        .unwrap();
    let stderr = canary["stderr"].as_str().unwrap();
    assert!(stderr.contains("CANARY") && stderr.contains("jdoe"));
    let r = replay("", stderr, Exit::Code(1), false);
    let RunOutcome::Failed {
        diagnostic: Some(diag),
        ..
    } = &r.outcome
    else {
        panic!()
    };
    for kept in r.tail.iter().chain([diag]) {
        assert!(!kept.contains("CANARY"), "secret kept: {kept}");
        assert!(!kept.contains("jdoe"), "user name kept: {kept}");
    }
    assert!(diag.contains("https://media.example.test/…"), "{diag}");
}

#[test]
fn no_captured_run_keeps_unredacted_paths() {
    for entry in std::fs::read_dir(dir().join("runs")).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        for bad in ["/tmp/", "/home/", "\\Users\\", "scratchpad"] {
            assert!(!text.contains(bad), "{} contains {bad}", path.display());
        }
    }
}
