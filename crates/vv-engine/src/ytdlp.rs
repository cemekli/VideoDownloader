//! One guarded yt-dlp run (STATE.md F3.3; SPEC §14.1, §20.2, AC-29;
//! ADR-0003, ADR-0005, ADR-0007).
//!
//! Every run:
//!
//! - starts its own network guard ([`vv_netguard::Guard`]) and gives yt-dlp
//!   only that proxy; every connection yt-dlp makes (redirects, manifests,
//!   segments, thumbnails) is checked there. In Offline Mode the guard
//!   refuses everything before name resolution;
//! - uses the fixed flags of `vv_ytdlp::args`, the managed Deno by explicit
//!   path, and builds every run-specific part (output folder and template,
//!   format selector) here, as separate argv entries, with `--` before the
//!   URL so a URL can never be read as an option;
//! - keeps only bounded, redacted stderr (`RunObserver`);
//! - ends with one SPEC §19 code. A connection the guard refused makes the
//!   run `URL_BLOCKED` whatever yt-dlp printed (`OFFLINE_MODE` when it was
//!   Offline Mode).

use crate::proc::{run_streaming, spawn_error_code, StdoutMode};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;
use vv_domain::error::ErrorCode;
use vv_netguard::{Block, Decision, Guard, Policy, SystemResolver};
use vv_process::ManagedCommand;
use vv_ytdlp::outcome::{RunObserver, RunOutcome, Stream};
use vv_ytdlp::progress::ProgressEvent;

/// Prefix of the line `--print after_move:...` writes with the final path.
pub const FILE_PREFIX: &str = "vv-file ";

/// Metadata runs are short; a hung extractor must not hold a worker.
pub const METADATA_TIMEOUT: Duration = Duration::from_secs(180);

/// What a run needs from the app.
#[derive(Debug, Clone)]
pub struct YtdlpTools {
    pub ytdlp: PathBuf,
    /// `None` when the managed Deno is missing or failed its check.
    pub deno: Option<PathBuf>,
    /// For merging and thumbnail conversion (local files only).
    pub ffmpeg: PathBuf,
    /// `DENO_DIR` (yt-dlp passes it to Deno; ADR-0005).
    pub deno_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub enum Mode {
    /// `--dump-single-json`; with `thumbnail_dir` the thumbnail is written
    /// there as `thumb.jpg` (converted by the local FFmpeg).
    Metadata { thumbnail_dir: Option<PathBuf> },
    /// Download into `dir` as `media.<ext>`; `merge_format` is the
    /// container separate video and audio are merged into.
    Download {
        dir: PathBuf,
        selector: String,
        merge_format: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunResult {
    Succeeded,
    Canceled,
    Failed {
        code: ErrorCode,
        /// Redacted, for the job row and diagnostics only.
        message: Option<String>,
    },
}

#[derive(Debug)]
pub struct Run {
    pub result: RunResult,
    /// `--dump-single-json` output (metadata mode).
    pub stdout: Vec<u8>,
    /// Final file paths yt-dlp reported (download mode).
    pub files: Vec<PathBuf>,
    /// Host, port and decision of every connection; no path, no query.
    pub connections: Vec<vv_netguard::Event>,
}

/// Arguments of a run, without the proxy (added by [`run`]). Public so the
/// tests can check them.
pub fn build_args(tools: &YtdlpTools, mode: &Mode) -> Vec<OsString> {
    let mut a: Vec<OsString> = vv_ytdlp::args::COMMON.iter().map(Into::into).collect();
    a.extend(vv_ytdlp::args::js_runtime_args(tools.deno.as_deref()));
    match mode {
        Mode::Metadata { thumbnail_dir } => {
            a.extend(vv_ytdlp::args::METADATA.iter().map(Into::into));
            if let Some(dir) = thumbnail_dir {
                for s in [
                    "--no-simulate",
                    "--skip-download",
                    "--write-thumbnail",
                    "--convert-thumbnails",
                    "jpg",
                    "-o",
                    "thumbnail:thumb.%(ext)s",
                    "-o",
                    "vv-metadata",
                ] {
                    a.push(s.into());
                }
                a.push("-P".into());
                a.push(dir.into());
                a.push("--ffmpeg-location".into());
                a.push(tools.ffmpeg.clone().into());
            }
        }
        Mode::Download {
            dir,
            selector,
            merge_format,
        } => {
            a.extend(vv_ytdlp::args::DOWNLOAD.iter().map(Into::into));
            for s in [
                "--print",
                "after_move:vv-file %(filepath)j",
                "--no-simulate",
                "--no-mtime",
                "--no-write-thumbnail",
                "--no-write-subs",
                "-o",
                "media.%(ext)s",
                "-f",
            ] {
                a.push(s.into());
            }
            a.push(selector.into());
            if let Some(m) = merge_format {
                a.push("--merge-output-format".into());
                a.push(m.into());
            }
            a.push("-P".into());
            a.push(dir.into());
            a.push("--ffmpeg-location".into());
            a.push(tools.ffmpeg.clone().into());
        }
    }
    a
}

/// Runs yt-dlp for `url` behind a new guard with `policy`.
///
/// `on_progress` gets every progress event; `should_stop` is polled and,
/// when it returns true, the process tree is killed and the run ends
/// `Canceled` (the caller knows whether that was a cancel, a pause or
/// Offline Mode).
pub fn run(
    tools: &YtdlpTools,
    policy: Policy,
    url: &str,
    mode: &Mode,
    deadline: Option<Duration>,
    mut on_progress: impl FnMut(&ProgressEvent),
    should_stop: impl FnMut() -> bool,
) -> Run {
    let guard = match Guard::start(policy, Box::new(SystemResolver)) {
        Ok(g) => g,
        Err(_) => {
            return Run {
                result: RunResult::Failed {
                    code: ErrorCode::InternalError,
                    message: Some("network guard did not start".into()),
                },
                stdout: Vec::new(),
                files: Vec::new(),
                connections: Vec::new(),
            }
        }
    };
    let mut args = build_args(tools, mode);
    args.extend(vv_ytdlp::args::network_guard_args(&guard.proxy_url()));
    args.push("--".into());
    args.push(url.into());

    let mut cmd = ManagedCommand::new(&tools.ytdlp);
    cmd.args(&args).env("DENO_DIR", &tools.deno_dir);
    if let Mode::Download { dir, .. } = mode {
        cmd.current_dir(dir);
    }
    let stdout_mode = match mode {
        Mode::Metadata { .. } => StdoutMode::Raw(vv_ytdlp::metadata::MAX_METADATA_BYTES),
        Mode::Download { .. } => StdoutMode::Lines,
    };
    let mut observer = RunObserver::new();
    let mut files = Vec::new();
    let streamed = run_streaming(
        cmd,
        stdout_mode,
        deadline,
        |stream, line| {
            if stream == Stream::Stdout {
                if let Some(p) = line.strip_prefix(FILE_PREFIX) {
                    if let Ok(path) = serde_json::from_str::<String>(p) {
                        files.push(PathBuf::from(path));
                    }
                    return;
                }
            }
            if let Some(ev) = observer.observe(stream, line) {
                on_progress(&ev);
            }
        },
        should_stop,
    );
    let connections = guard.events();
    drop(guard);
    let streamed = match streamed {
        Ok(s) => s,
        Err(e) => {
            return Run {
                result: RunResult::Failed {
                    code: spawn_error_code(&e),
                    message: None,
                },
                stdout: Vec::new(),
                files,
                connections,
            }
        }
    };
    let blocked: Vec<&Block> = connections
        .iter()
        .filter_map(|e| match &e.decision {
            Decision::Blocked { block } => Some(block),
            _ => None,
        })
        .collect();
    let result = if streamed.timed_out {
        RunResult::Failed {
            code: ErrorCode::Timeout,
            message: None,
        }
    } else if streamed.stopped {
        RunResult::Canceled
    } else if blocked.iter().any(|b| matches!(b, Block::Offline)) {
        RunResult::Failed {
            code: ErrorCode::OfflineMode,
            message: None,
        }
    } else if !blocked.is_empty() {
        RunResult::Failed {
            code: ErrorCode::UrlBlocked,
            message: None,
        }
    } else {
        match observer.finish(streamed.exit, false) {
            RunOutcome::Succeeded => RunResult::Succeeded,
            RunOutcome::Canceled => RunResult::Canceled,
            RunOutcome::Failed { code, diagnostic } => RunResult::Failed {
                code,
                message: diagnostic,
            },
        }
    };
    Run {
        result,
        stdout: if streamed.stdout_truncated {
            Vec::new()
        } else {
            streamed.stdout
        },
        files,
        connections,
    }
}

/// `true` when `p` is inside `dir` (both canonicalised). Used before a file
/// yt-dlp reported is trusted (§14.2: no string prefix checks).
pub fn inside(dir: &Path, p: &Path) -> bool {
    match (dir.canonicalize(), p.canonicalize()) {
        (Ok(d), Ok(f)) => f.starts_with(&d) && f != d,
        _ => false,
    }
}

/// The check before any process starts (§20.2): HTTP(S), no credentials,
/// no loopback/private literal or `localhost`. The guard then checks every
/// connection. Literal addresses in `policy.test_targets` pass, so tests can
/// point the real yt-dlp at local servers; the app never sets any.
pub fn precheck(policy: &Policy, url: &str) -> Result<(), ErrorCode> {
    if !policy.test_targets.is_empty() {
        if let Ok(u) = url::Url::parse(url) {
            let ip = match u.host() {
                Some(url::Host::Ipv4(ip)) => Some(std::net::IpAddr::V4(ip)),
                Some(url::Host::Ipv6(ip)) => Some(std::net::IpAddr::V6(ip)),
                _ => None,
            };
            if let (Some(ip), Some(port)) = (ip, u.port_or_known_default()) {
                if policy
                    .test_targets
                    .contains(&std::net::SocketAddr::new(ip, port))
                {
                    return Ok(());
                }
            }
        }
    }
    vv_domain::url_policy::validate_capture_url(url)
        .map(|_| ())
        .map_err(|e| ErrorCode::from(&e))
}
