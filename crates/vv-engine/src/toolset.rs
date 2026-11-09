//! Managed tool health check (STATE.md F3.2; SPEC §14.1, §23.1, DL-09,
//! AC-25; ADR-0005, ADR-0006, ADR-0021).
//!
//! At start-up (and after a tool update) every tool in `tools/manifest.json`
//! is checked in this order, and the first failure decides its state:
//!
//! 1. The file is where the app looks for it (`TOOL_MISSING` otherwise).
//! 2. Its SHA-256 equals the manifest's `executableSha256`. A different file
//!    is never run (`CHECKSUM_MISMATCH`, AC-25).
//! 3. A light version query starts. If the OS refuses to start it (antivirus
//!    quarantine, policy) the state is `TOOL_BLOCKED`.
//! 4. The version it prints is the pinned one (`TOOL_MISSING` otherwise).
//!
//! The report holds versions, sources and licences for Diagnostics and
//! never a path (§18.3).

use crate::proc::{run_capture, spawn_error_code};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;
use vv_domain::error::ErrorCode;
use vv_tools::{
    check_version, parse_deno_version, parse_ffmpeg_banner, parse_ytdlp_version, verify_file,
    Manifest, Provenance, ToolId, VerifyError, Version, VersionCheck, DENO_MIN_FOR_YTDLP,
};

/// How long a version query may take before the tool counts as broken.
pub const VERSION_TIMEOUT: Duration = Duration::from_secs(20);
const VERSION_OUTPUT_MAX: u64 = 64 * 1024;

/// `tools/bin/<platform>` directory name and manifest platform for this OS.
pub fn platform() -> &'static str {
    if cfg!(windows) {
        "windows-x86_64"
    } else {
        "linux-x86_64"
    }
}

/// Where each managed executable lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPaths {
    pub ytdlp: PathBuf,
    pub deno: PathBuf,
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

impl ToolPaths {
    /// The layout `scripts/fetch-tools.mjs` and the installer write:
    /// `<bin>/<platform>/<id>/<executable>`.
    pub fn in_dir(bin: &Path, manifest: &Manifest) -> ToolPaths {
        let dir = bin.join(platform());
        let exe = |id: ToolId| {
            let name = manifest
                .artifact(id, platform())
                .map(|a| a.executable.clone())
                .unwrap_or_else(|| id.as_str().to_owned());
            dir.join(id.as_str()).join(name)
        };
        ToolPaths {
            ytdlp: exe(ToolId::YtDlp),
            deno: exe(ToolId::Deno),
            ffmpeg: exe(ToolId::Ffmpeg),
            ffprobe: exe(ToolId::Ffprobe),
        }
    }

    pub fn get(&self, id: ToolId) -> &Path {
        match id {
            ToolId::YtDlp => &self.ytdlp,
            ToolId::Deno => &self.deno,
            ToolId::Ffmpeg => &self.ffmpeg,
            ToolId::Ffprobe => &self.ffprobe,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolState {
    Ok,
    Missing,
    Blocked,
    ChecksumMismatch,
}

/// One tool as Diagnostics shows it (DL-09, MT-11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolHealth {
    pub id: &'static str,
    /// The pinned version (`tools/manifest.json`).
    pub version: String,
    /// What the tool reported, when it ran.
    pub reported_version: Option<String>,
    pub state: ToolState,
    pub code: Option<ErrorCode>,
    pub license: String,
    pub source: String,
    /// `gpg-signed-sums` or `published-sha256`.
    pub provenance: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolReport {
    pub tools: Vec<ToolHealth>,
    pub checked_at: i64,
}

impl ToolReport {
    pub fn health(&self, id: ToolId) -> Option<&ToolHealth> {
        self.tools.iter().find(|t| t.id == id.as_str())
    }

    pub fn is_ok(&self, id: ToolId) -> bool {
        self.health(id).is_some_and(|t| t.state == ToolState::Ok)
    }

    /// What a download or metadata run needs: yt-dlp, FFmpeg and ffprobe.
    /// Deno is optional here: without it yt-dlp gets no JS runtime and the
    /// platforms that need one fail with `TOOL_MISSING` (ADR-0005).
    pub fn ready_for_downloads(&self) -> Result<(), ErrorCode> {
        for id in [ToolId::YtDlp, ToolId::Ffmpeg, ToolId::Ffprobe] {
            match self.health(id) {
                Some(t) if t.state == ToolState::Ok => {}
                Some(t) => return Err(t.code.unwrap_or(ErrorCode::ToolMissing)),
                None => return Err(ErrorCode::ToolMissing),
            }
        }
        Ok(())
    }

    /// A report for when no tools directory was found at all.
    pub fn all_missing(manifest: &Manifest, now: i64) -> ToolReport {
        ToolReport {
            tools: manifest
                .tools
                .iter()
                .map(|t| ToolHealth {
                    id: t.id.as_str(),
                    version: t.version.clone(),
                    reported_version: None,
                    state: ToolState::Missing,
                    code: Some(ErrorCode::ToolMissing),
                    license: t.license.clone(),
                    source: t.source.clone(),
                    provenance: provenance_name(&t.provenance),
                })
                .collect(),
            checked_at: now,
        }
    }
}

fn provenance_name(p: &Provenance) -> &'static str {
    match p {
        Provenance::GpgSignedSums { .. } => "gpg-signed-sums",
        Provenance::PublishedSha256 => "published-sha256",
    }
}

/// Checks every tool of the manifest at `paths`.
pub fn check_tools(manifest: &Manifest, paths: &ToolPaths, now: i64) -> ToolReport {
    let tools = manifest
        .tools
        .iter()
        .map(|t| {
            let mut h = ToolHealth {
                id: t.id.as_str(),
                version: t.version.clone(),
                reported_version: None,
                state: ToolState::Ok,
                code: None,
                license: t.license.clone(),
                source: t.source.clone(),
                provenance: provenance_name(&t.provenance),
            };
            let result = match manifest.artifact(t.id, platform()) {
                None => Err(ErrorCode::ToolMissing),
                Some(a) => check_one(
                    t.id,
                    &t.version,
                    t.build.as_deref(),
                    &a.executable_sha256,
                    paths.get(t.id),
                    &mut h.reported_version,
                ),
            };
            if let Err(code) = result {
                h.state = match code {
                    ErrorCode::ToolBlocked => ToolState::Blocked,
                    ErrorCode::ChecksumMismatch => ToolState::ChecksumMismatch,
                    _ => ToolState::Missing,
                };
                h.code = Some(code);
            }
            h
        })
        .collect();
    ToolReport {
        tools,
        checked_at: now,
    }
}

fn verify_code(e: &VerifyError) -> ErrorCode {
    match e {
        VerifyError::Unreadable(kind)
            if *kind == std::io::ErrorKind::PermissionDenied.to_string() =>
        {
            ErrorCode::ToolBlocked
        }
        other => other.code(),
    }
}

fn check_one(
    id: ToolId,
    pinned: &str,
    build: Option<&str>,
    sha256: &str,
    path: &Path,
    reported: &mut Option<String>,
) -> Result<(), ErrorCode> {
    verify_file(path, sha256, None).map_err(|e| verify_code(&e))?;
    let arg = if id.is_ffmpeg_family() {
        "-version"
    } else {
        "--version"
    };
    let out = run_capture(path, &[arg.into()], VERSION_TIMEOUT, VERSION_OUTPUT_MAX)
        .map_err(|e| spawn_error_code(&e))?;
    if out.timed_out || out.code != Some(0) {
        return Err(ErrorCode::ToolMissing);
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let check = match id {
        ToolId::YtDlp => {
            let v = parse_ytdlp_version(&stdout);
            *reported = v.map(|v| v.to_string());
            check_version(pinned, v)
        }
        ToolId::Deno => {
            let v = parse_deno_version(&stdout);
            *reported = v.map(|v| v.to_string());
            match v {
                Some(found) if found < DENO_MIN_FOR_YTDLP => VersionCheck::Mismatch {
                    expected: Version::parse(pinned).unwrap_or(DENO_MIN_FOR_YTDLP),
                    found,
                },
                _ => check_version(pinned, v),
            }
        }
        ToolId::Ffmpeg | ToolId::Ffprobe => match parse_ffmpeg_banner(&stdout) {
            None => VersionCheck::Unrecognised,
            Some(b) => {
                *reported = Some(b.build.clone());
                b.check(id.as_str(), build.unwrap_or(pinned))
            }
        },
    };
    match check.code() {
        None => Ok(()),
        Some(code) => Err(code),
    }
}
