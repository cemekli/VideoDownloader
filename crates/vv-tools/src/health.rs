//! Version health checks (SPEC §23.1: a light version query at start-up and
//! after a tool update). Parses what `yt-dlp --version`, `deno --version` and
//! yt-dlp's `-v` header print; the strings come from real runs of the pinned
//! releases (`tests/fixtures/tools/`).

use std::cmp::Ordering;
use std::fmt;
use vv_domain::error::ErrorCode;

/// Oldest Deno the pinned yt-dlp accepts (`DenoJsRuntime.MIN_SUPPORTED_VERSION`
/// in yt-dlp 2026.10.26, `yt_dlp/utils/_jsruntime.py`). Older ones show as
/// "deno-x.y.z (unsupported)" and are not used.
pub const DENO_MIN_FOR_YTDLP: Version = Version([2, 3, 0]);

/// A three-part version. yt-dlp's date versions (2026.10.26) fit too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(pub [u32; 3]);

impl Version {
    pub fn parse(s: &str) -> Option<Version> {
        let mut parts = s.trim().split('.');
        let mut out = [0u32; 3];
        for slot in &mut out {
            let p = parts.next()?;
            if p.is_empty() || p.len() > 9 || !p.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            *slot = p.parse().ok()?;
        }
        parts.next().is_none().then_some(Version(out))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c] = self.0;
        write!(f, "{a}.{b}.{c}")
    }
}

/// `yt-dlp --version` prints only the version, e.g. `2026.10.26`.
/// Nightly builds print `2026.10.26.123456`, which is not a pinned release.
pub fn parse_ytdlp_version(stdout: &str) -> Option<Version> {
    let line = stdout.lines().find(|l| !l.trim().is_empty())?;
    let v = line.trim();
    // Keep the zero padding out of the comparison: 2026.01.05 == 2026.1.5.
    Version::parse(v)
}

/// `deno --version`: first line `deno 2.9.8 (stable, release, x86_64-...)`.
pub fn parse_deno_version(stdout: &str) -> Option<Version> {
    let first = stdout.lines().next()?.trim();
    let rest = first.strip_prefix("deno ")?;
    let v = rest.split_whitespace().next()?;
    Version::parse(v)
}

/// The `[debug] JS runtimes:` line of `yt-dlp -v`, which says what yt-dlp
/// actually picked. Used in diagnostics and the compatibility check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsRuntimeLine {
    /// `none (disabled)`: we passed `--no-js-runtimes` and nothing else.
    Disabled,
    /// `none`: a runtime was enabled but not found at the given path.
    NotFound,
    /// `deno-2.9.8`.
    Supported { name: String, version: String },
    /// `deno-1.46.3 (unsupported)`: found but too old.
    Unsupported { name: String, version: String },
}

pub fn parse_js_runtimes_line(line: &str) -> Option<JsRuntimeLine> {
    let value = line.trim().strip_prefix("[debug] JS runtimes:")?.trim();
    match value {
        "none (disabled)" => return Some(JsRuntimeLine::Disabled),
        "none" => return Some(JsRuntimeLine::NotFound),
        _ => {}
    }
    // We enable exactly one runtime, so only the first entry matters.
    let first = value.split(',').next()?.trim();
    let (entry, unsupported) = match first.strip_suffix("(unsupported)") {
        Some(e) => (e.trim(), true),
        None => (first, false),
    };
    let (name, version) = entry.split_once('-')?;
    let (name, version) = (name.to_owned(), version.to_owned());
    Some(if unsupported {
        JsRuntimeLine::Unsupported { name, version }
    } else {
        JsRuntimeLine::Supported { name, version }
    })
}

/// Result of comparing a reported version with the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionCheck {
    Ok,
    /// Nothing parseable came back: broken, blocked or not the tool at all.
    Unrecognised,
    /// A different version than the manifest pins. Never run: the hash would
    /// have failed too, so this means something replaced the file.
    Mismatch {
        expected: Version,
        found: Version,
    },
}

impl VersionCheck {
    pub fn code(&self) -> Option<ErrorCode> {
        match self {
            VersionCheck::Ok => None,
            VersionCheck::Unrecognised | VersionCheck::Mismatch { .. } => {
                Some(ErrorCode::ToolMissing)
            }
        }
    }
}

/// Compares `found` (already parsed) with the manifest's pinned version.
pub fn check_version(pinned: &str, found: Option<Version>) -> VersionCheck {
    let Some(expected) = Version::parse(pinned) else {
        return VersionCheck::Unrecognised;
    };
    match found {
        None => VersionCheck::Unrecognised,
        Some(f) if f.cmp(&expected) == Ordering::Equal => VersionCheck::Ok,
        Some(found) => VersionCheck::Mismatch { expected, found },
    }
}

/// What `ffmpeg -version` / `ffprobe -version` print in their first lines:
/// `ffmpeg version n9.0.2-39-g7adfbe40dc-20261031 Copyright (c) ...` and
/// `configuration: --prefix=... --enable-version3 ...`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegBanner {
    /// `ffmpeg` or `ffprobe`.
    pub program: String,
    /// The build string, compared with the manifest's `build` as is.
    pub build: String,
    pub configuration: Vec<String>,
}

/// Licence of an FFmpeg build, read from its `configure` flags. FFmpeg is
/// LGPL unless built with `--enable-gpl` (GPL) or `--enable-nonfree` (not
/// redistributable at all).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfmpegLicence {
    /// LGPL; with `--enable-version3` it is LGPL-3.0-or-later.
    Lgpl {
        version3: bool,
    },
    Gpl,
    Nonfree,
}

pub fn parse_ffmpeg_banner(stdout: &str) -> Option<FfmpegBanner> {
    let mut lines = stdout.lines();
    let first = lines.next()?.trim();
    let (program, rest) = first.split_once(" version ")?;
    if !matches!(program, "ffmpeg" | "ffprobe") {
        return None;
    }
    let build = rest.split_whitespace().next()?.to_owned();
    let configuration = lines
        .take(4)
        .find_map(|l| l.trim().strip_prefix("configuration:"))?
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    Some(FfmpegBanner {
        program: program.to_owned(),
        build,
        configuration,
    })
}

impl FfmpegBanner {
    pub fn licence(&self) -> FfmpegLicence {
        let has = |flag: &str| self.configuration.iter().any(|f| f == flag);
        if has("--enable-nonfree") {
            FfmpegLicence::Nonfree
        } else if has("--enable-gpl") {
            FfmpegLicence::Gpl
        } else {
            FfmpegLicence::Lgpl {
                version3: has("--enable-version3"),
            }
        }
    }

    /// Start-up check against the manifest: the right program, exactly the
    /// pinned build, and an LGPL configuration. Anything else is reported as
    /// `TOOL_MISSING` and the tool is not used.
    pub fn check(&self, program: &str, pinned_build: &str) -> VersionCheck {
        let pinned = pinned_version_of_build(pinned_build);
        let found = pinned_version_of_build(&self.build);
        if self.program != program || !matches!(self.licence(), FfmpegLicence::Lgpl { .. }) {
            return VersionCheck::Unrecognised;
        }
        match (pinned, found) {
            (Some(_), _) if self.build == pinned_build => VersionCheck::Ok,
            (Some(expected), Some(found)) => VersionCheck::Mismatch { expected, found },
            _ => VersionCheck::Unrecognised,
        }
    }
}

/// `n9.0.2-39-g7adfbe40dc-20261031` -> 9.0.2; distro builds such as
/// `6.1.1-3ubuntu5` -> 6.1.1.
fn pinned_version_of_build(build: &str) -> Option<Version> {
    let b = build.strip_prefix('n').unwrap_or(build);
    Version::parse(b.split('-').next()?)
}
