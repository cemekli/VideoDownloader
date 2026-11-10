//! Turning a finished yt-dlp run into one SPEC §19 outcome.
//!
//! [`RunObserver`] watches every line of both streams while the process runs.
//! It keeps only a bounded, redacted tail of stderr plus the last few `ERROR:`
//! lines, emits progress events as they arrive, and at the end maps exit
//! status + errors to [`RunOutcome`].
//!
//! The mapping only uses yt-dlp's own wording (copied from its source or
//! captured, see `tests/fixtures/ytdlp/`). When nothing specific matches the
//! code is `EXTRACTOR_FAILED`: SPEC §19 forbids inventing a more precise cause.
//! `MEDIA_URL_EXPIRED` is not produced yet: no captured run shows a wording
//! that tells it apart from a plain 403.

use crate::progress::{self, ProgressEvent};
use crate::redact::redact;
use std::collections::VecDeque;
use vv_domain::error::ErrorCode;

/// stderr lines kept (redacted) for diagnostics.
pub const STDERR_TAIL_LINES: usize = 20;
/// `ERROR:` lines kept for classification.
const ERROR_LINES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Code(i32),
    /// Ended by a signal / terminated (Job Object close on Windows).
    Killed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    Succeeded,
    /// Our own cancel, or yt-dlp reporting "Interrupted by user" (SPEC §19:
    /// terminal, not shown as an error).
    Canceled,
    Failed {
        code: ErrorCode,
        /// Redacted last error line, for the diagnostic log only.
        diagnostic: Option<String>,
    },
}

impl RunOutcome {
    pub fn code(&self) -> Option<ErrorCode> {
        match self {
            RunOutcome::Succeeded => None,
            RunOutcome::Canceled => Some(ErrorCode::Canceled),
            RunOutcome::Failed { code, .. } => Some(*code),
        }
    }
}

#[derive(Debug, Default)]
pub struct RunObserver {
    tail: VecDeque<String>,
    errors: VecDeque<String>,
    usage_error: bool,
    js_runtime_missing: bool,
    malformed_progress: usize,
    lines_seen: u64,
}

impl RunObserver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one line (already split and bounded by [`crate::lines`]).
    pub fn observe(&mut self, stream: Stream, line: &str) -> Option<ProgressEvent> {
        self.lines_seen += 1;
        match progress::parse_line(line) {
            Ok(Some(event)) => return Some(event),
            Ok(None) => {}
            Err(_) => {
                self.malformed_progress += 1;
                return None;
            }
        }
        if stream == Stream::Stdout {
            // Only progress is expected on stdout during a download; any other
            // stdout text is yt-dlp's own and not needed.
            return None;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Some(msg) = trimmed.strip_prefix("ERROR:") {
            push_bounded(&mut self.errors, msg.trim().to_string(), ERROR_LINES);
        } else if trimmed.starts_with("yt-dlp: error:") {
            self.usage_error = true;
        } else if trimmed.starts_with("WARNING:")
            && contains_any(&trimmed.to_lowercase(), JS_RUNTIME)
        {
            self.js_runtime_missing = true;
        }
        push_bounded(&mut self.tail, redact(trimmed), STDERR_TAIL_LINES);
        None
    }

    /// Redacted stderr tail, oldest first.
    pub fn stderr_tail(&self) -> impl Iterator<Item = &str> {
        self.tail.iter().map(String::as_str)
    }

    /// Progress lines that had our prefix but did not parse.
    pub fn malformed_progress(&self) -> usize {
        self.malformed_progress
    }

    pub fn finish(self, exit: Exit, cancel_requested: bool) -> RunOutcome {
        let last_error = self.errors.back().cloned();
        let diagnostic = last_error.as_deref().map(redact);
        if cancel_requested {
            return RunOutcome::Canceled;
        }
        match exit {
            Exit::Code(0) if last_error.is_none() => return RunOutcome::Succeeded,
            Exit::Code(2) if self.usage_error => {
                return RunOutcome::Failed {
                    code: ErrorCode::InternalError,
                    diagnostic: self.tail.back().cloned(),
                }
            }
            Exit::Killed if last_error.is_none() => {
                // Killed by someone else (OS, antivirus). The caller replaces
                // this with TIMEOUT when its own deadline fired.
                return RunOutcome::Failed {
                    code: ErrorCode::InternalError,
                    diagnostic: None,
                };
            }
            _ => {}
        }
        let mut code = last_error
            .as_deref()
            .map_or(ErrorCode::ExtractorFailed, classify);
        if code == ErrorCode::Canceled {
            return RunOutcome::Canceled;
        }
        if code == ErrorCode::ExtractorFailed && self.js_runtime_missing {
            code = ErrorCode::ToolMissing;
        }
        RunOutcome::Failed { code, diagnostic }
    }
}

const JS_RUNTIME: &[&str] = &["no supported javascript runtime"];

/// Rules in priority order: the first group whose phrase appears wins.
const RULES: &[(ErrorCode, &[&str])] = &[
    (ErrorCode::Canceled, &["interrupted by user"]),
    // `--cookies-from-browser` could not read the browser's store (missing
    // profile, locked or app-bound encrypted database). Before the path
    // rules: a locked database also says "permission denied". Not a proof
    // that the session is invalid (§12.1).
    (
        ErrorCode::CookieAccessFailed,
        &[
            "failed to load cookies",
            "cookies database",
            "cookie database",
            "failed to decrypt",
            "dpapi",
            "unsupported browser specified for cookies",
            "unsupported keyring",
        ],
    ),
    (
        ErrorCode::DiskFull,
        &[
            "no space left on device",
            "[errno 28]",
            "disk quota exceeded",
            "not enough space on the disk",
        ],
    ),
    (
        ErrorCode::PathNotWritable,
        &[
            "unable to open for writing",
            "permission denied",
            "[errno 13]",
        ],
    ),
    (
        ErrorCode::ToolMissing,
        &[
            "ffmpeg not found",
            "ffmpeg is not installed",
            "ffprobe and ffmpeg not found",
            "no supported javascript runtime",
        ],
    ),
    (ErrorCode::DrmUnsupported, &["drm protected"]),
    (
        ErrorCode::RegionRestricted,
        &[
            "geo restriction",
            "not available in your country",
            "not available from your location",
        ],
    ),
    (
        ErrorCode::LoginRequired,
        &[
            "only available for registered users",
            "--cookies-from-browser or --cookies",
            "login if you have permissions",
            "login required",
            // VK: posts and videos limited to a group's members or
            // followers; yt-dlp passes VK's own text through.
            "only available to followers",
            "only available to subscribers",
            "only available to members",
            "only available to community members",
            "available only to community members",
            "private video. sign in",
        ],
    ),
    (
        ErrorCode::RateLimited,
        &["http error 429", "too many requests"],
    ),
    (
        ErrorCode::ContentUnavailable,
        &[
            "http error 404",
            "http error 410",
            "video unavailable",
            "has been removed",
            "does not exist",
        ],
    ),
    (
        ErrorCode::AccessDenied,
        &["http error 403", "http error 401"],
    ),
    (ErrorCode::ExtractorUnsupported, &["unsupported url"]),
    (
        ErrorCode::NetworkUnavailable,
        &[
            "connection refused",
            "winerror 10061",
            "getaddrinfo failed",
            "name resolution",
            "name or service not known",
            "network is unreachable",
            "timed out",
            "connection reset",
            "remote end closed connection",
        ],
    ),
];

/// Maps one `ERROR:` message (without the prefix) to a code.
pub fn classify(message: &str) -> ErrorCode {
    let m = message.to_lowercase();
    RULES
        .iter()
        .find(|(_, phrases)| contains_any(&m, phrases))
        .map_or(ErrorCode::ExtractorFailed, |(code, _)| *code)
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

fn push_bounded(q: &mut VecDeque<String>, s: String, max: usize) {
    if q.len() == max {
        q.pop_front();
    }
    q.push_back(s);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stderr_tail_is_bounded_and_redacted() {
        let mut o = RunObserver::new();
        for i in 0..100 {
            o.observe(
                Stream::Stderr,
                &format!("WARNING: {i} https://x.test/p?sig=CANARY"),
            );
        }
        let tail: Vec<_> = o.stderr_tail().collect();
        assert_eq!(tail.len(), STDERR_TAIL_LINES);
        assert!(tail[0].starts_with("WARNING: 80 "));
        assert!(tail.iter().all(|l| !l.contains("CANARY")));
    }

    #[test]
    fn exit_zero_with_error_line_is_not_success() {
        let mut o = RunObserver::new();
        o.observe(
            Stream::Stderr,
            "ERROR: [generic] x: Unsupported URL: https://x.test/",
        );
        assert_eq!(
            o.finish(Exit::Code(0), false).code(),
            Some(ErrorCode::ExtractorUnsupported)
        );
    }

    #[test]
    fn browser_cookie_errors_are_cookie_access_not_login_or_path() {
        for m in [
            "could not find firefox cookies database in C:\\Users\\x\\AppData",
            "Could not copy Chrome cookie database. See https://github.com/yt-dlp/yt-dlp/issues/7271 for more info",
            "Failed to decrypt with DPAPI. See  https://github.com/yt-dlp/yt-dlp/issues/10927 for more info",
            "failed to load cookies",
        ] {
            assert_eq!(classify(m), ErrorCode::CookieAccessFailed, "{m}");
        }
        assert_eq!(
            classify("[Reddit] 1abc2de: Account authentication is required. Use --cookies-from-browser or --cookies for the authentication. See  https://github.com/yt-dlp/yt-dlp/wiki/FAQ"),
            ErrorCode::LoginRequired
        );
    }

    #[test]
    fn vk_follower_only_videos_ask_for_a_login() {
        for m in [
            "[vk] -159700045_456248459: Video only available to followers",
            "[vk] -1_2: This video is available only to community members",
            "[youtube] abc: Private video. Sign in if you've been granted access to this video",
        ] {
            assert_eq!(classify(m), ErrorCode::LoginRequired, "{m}");
        }
    }

    #[test]
    fn our_cancel_wins_over_any_error() {
        let mut o = RunObserver::new();
        o.observe(
            Stream::Stderr,
            "ERROR: unable to write data: [Errno 28] No space left on device",
        );
        assert_eq!(o.finish(Exit::Killed, true), RunOutcome::Canceled);
    }
}
