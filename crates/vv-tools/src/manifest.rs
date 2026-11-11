//! `tools/manifest.json`: what VideoVault may run, pinned by version and hash.
//!
//! The trust anchors live in code, not in the manifest: which URL prefixes a
//! tool may come from and which key signs yt-dlp's checksum file. A manifest
//! that points anywhere else is rejected, so editing the JSON alone cannot
//! redirect the app to an unknown executable (SPEC §23.1).

use serde::Deserialize;
use std::fmt;

pub const SCHEMA_VERSION: u32 = 1;

/// yt-dlp's release signing key (Simon Sawicki, rsa4096, created 2023-02-26),
/// published at github.com/yt-dlp/yt-dlp/blob/master/public.key. It signs
/// `SHA2-256SUMS`; `scripts/fetch-tools.mjs` checks that signature.
pub const YTDLP_SIGNING_KEY: &str = "AC0CBBE6848D6A873464AF4E57CF65933B5A7581";

/// Platforms the manifest may name. Windows is the release target (SPEC §3);
/// Linux keeps CI and development working.
pub const PLATFORMS: &[&str] = &["windows-x86_64", "linux-x86_64"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum ToolId {
    #[serde(rename = "yt-dlp")]
    YtDlp,
    #[serde(rename = "deno")]
    Deno,
    #[serde(rename = "ffmpeg")]
    Ffmpeg,
    #[serde(rename = "ffprobe")]
    Ffprobe,
    /// whisper.cpp's command-line transcriber (F6.2, ADR-0028). Optional:
    /// without it only local transcription is unavailable.
    #[serde(rename = "whisper-cli")]
    WhisperCli,
}

/// Every tool the app needs; a manifest without one of them is rejected.
pub const REQUIRED_TOOLS: &[ToolId] =
    &[ToolId::YtDlp, ToolId::Deno, ToolId::Ffmpeg, ToolId::Ffprobe];

/// Tools the manifest may name but need not, and that may have no build
/// for some platforms (whisper.cpp publishes no Linux binary).
pub const OPTIONAL_TOOLS: &[ToolId] = &[ToolId::WhisperCli];

/// The whisper.cpp release `whisper-bin-x64.zip` is built with MSVC and
/// loads its libraries from its own folder; only `.dll` companions exist.
const COMPANION_EXTENSION: &str = ".dll";
/// More companions than a release could plausibly need is a broken entry.
const MAX_COMPANIONS: usize = 32;

/// BtbN/FFmpeg-Builds release tags look like `autobuild-2026-10-31-12-59`.
const FFMPEG_TAG_PREFIX: &str = "autobuild-";

impl ToolId {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolId::YtDlp => "yt-dlp",
            ToolId::Deno => "deno",
            ToolId::Ffmpeg => "ffmpeg",
            ToolId::Ffprobe => "ffprobe",
            ToolId::WhisperCli => "whisper-cli",
        }
    }

    pub fn is_optional(self) -> bool {
        OPTIONAL_TOOLS.contains(&self)
    }

    /// FFmpeg and ffprobe come from the same BtbN archive.
    pub fn is_ffmpeg_family(self) -> bool {
        matches!(self, ToolId::Ffmpeg | ToolId::Ffprobe)
    }

    /// The only place this tool's files may be downloaded from.
    pub fn allowed_url_prefix(self) -> &'static str {
        match self {
            ToolId::YtDlp => "https://github.com/yt-dlp/yt-dlp/releases/download/",
            ToolId::Deno => "https://github.com/denoland/deno/releases/download/",
            ToolId::Ffmpeg | ToolId::Ffprobe => {
                "https://github.com/BtbN/FFmpeg-Builds/releases/download/"
            }
            ToolId::WhisperCli => "https://github.com/ggml-org/whisper.cpp/releases/download/",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Provenance {
    /// A checksum file signed with a release key (yt-dlp).
    #[serde(rename_all = "camelCase")]
    GpgSignedSums {
        sums_url: String,
        signature_url: String,
        key_fingerprint: String,
    },
    /// Only a published SHA-256 next to the release asset (Deno, FFmpeg).
    PublishedSha256,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Artifact {
    pub platform: String,
    pub url: String,
    /// SHA-256 of the downloaded file.
    pub sha256: String,
    pub size: u64,
    /// `zip` or `tar.xz` when the download is an archive; absent for a bare
    /// executable.
    #[serde(default)]
    pub archive: Option<String>,
    /// Path of the executable inside the archive when it is not at the top,
    /// `/`-separated (FFmpeg: `ffmpeg-n9.0.2-...-lgpl-9.0/bin/ffmpeg.exe`).
    /// Its last part must equal `executable`.
    #[serde(default)]
    pub archive_path: Option<String>,
    /// File name the executable is installed under.
    pub executable: String,
    /// SHA-256 of the executable itself; checked before every first run.
    pub executable_sha256: String,
    /// Libraries the executable loads from its own folder (whisper.cpp:
    /// `whisper.dll`, `ggml*.dll`). Each is unpacked next to it and checked
    /// by hash like the executable; a tool whose companion is missing or
    /// different is not run.
    #[serde(default)]
    pub companions: Vec<Companion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Companion {
    /// `/`-separated path inside the archive; its last part is `file`.
    pub archive_path: String,
    /// Name it is installed under, next to the executable.
    pub file: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tool {
    pub id: ToolId,
    pub version: String,
    /// The exact build string the tool prints, when the version alone does
    /// not identify the build (FFmpeg: `n9.0.2-39-g7adfbe40dc-20261031`).
    #[serde(default)]
    pub build: Option<String>,
    pub license: String,
    pub source: String,
    pub provenance: Provenance,
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub tools: Vec<Tool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    Json(String),
    Schema(u32),
    Duplicate(String),
    Missing(&'static str),
    Invalid { tool: &'static str, reason: String },
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::Json(e) => write!(f, "manifest is not valid JSON for the schema: {e}"),
            ManifestError::Schema(v) => write!(f, "unsupported manifest schemaVersion {v}"),
            ManifestError::Duplicate(what) => write!(f, "duplicate entry: {what}"),
            ManifestError::Missing(tool) => write!(f, "manifest has no {tool} entry"),
            ManifestError::Invalid { tool, reason } => write!(f, "{tool}: {reason}"),
        }
    }
}

impl std::error::Error for ManifestError {}

fn is_sha256(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_plain_file_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && !s.starts_with('.')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

impl Manifest {
    /// Parses and validates. Every tool the app needs must be present.
    pub fn parse(json: &str) -> Result<Manifest, ManifestError> {
        let m: Manifest =
            serde_json::from_str(json).map_err(|e| ManifestError::Json(e.to_string()))?;
        m.validate()?;
        Ok(m)
    }

    pub fn tool(&self, id: ToolId) -> Option<&Tool> {
        self.tools.iter().find(|t| t.id == id)
    }

    /// The artifact for one tool on one platform, e.g. `windows-x86_64`.
    pub fn artifact(&self, id: ToolId, platform: &str) -> Option<&Artifact> {
        self.tool(id)?
            .artifacts
            .iter()
            .find(|a| a.platform == platform)
    }

    fn validate(&self) -> Result<(), ManifestError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(ManifestError::Schema(self.schema_version));
        }
        let mut seen = Vec::new();
        for t in &self.tools {
            if seen.contains(&t.id) {
                return Err(ManifestError::Duplicate(t.id.as_str().into()));
            }
            seen.push(t.id);
            validate_tool(t)?;
        }
        for &id in REQUIRED_TOOLS {
            if self.tool(id).is_none() {
                return Err(ManifestError::Missing(id.as_str()));
            }
            // A required tool runs everywhere the app does.
            for p in PLATFORMS {
                if self.artifact(id, p).is_none() {
                    return Err(ManifestError::Invalid {
                        tool: id.as_str(),
                        reason: format!("no artifact for {p}"),
                    });
                }
            }
        }
        // One archive, one build: ffprobe must be the same download as ffmpeg.
        let (ff, fp) = (
            self.tool(ToolId::Ffmpeg).unwrap(),
            self.tool(ToolId::Ffprobe).unwrap(),
        );
        let same_files = ff.artifacts.len() == fp.artifacts.len()
            && ff.artifacts.iter().all(|a| {
                fp.artifacts.iter().any(|b| {
                    a.platform == b.platform
                        && a.url == b.url
                        && a.sha256 == b.sha256
                        && a.size == b.size
                })
            });
        if ff.version != fp.version || ff.build != fp.build || !same_files {
            return Err(ManifestError::Invalid {
                tool: "ffprobe",
                reason: "must be the same build and archive as ffmpeg".into(),
            });
        }
        Ok(())
    }
}

fn validate_tool(t: &Tool) -> Result<(), ManifestError> {
    let tool = t.id.as_str();
    let bad = |reason: String| ManifestError::Invalid { tool, reason };
    let prefix = t.id.allowed_url_prefix();

    if t.version.is_empty() || !t.version.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return Err(bad(format!("version {:?} is not dotted digits", t.version)));
    }
    if t.license.trim().is_empty() {
        return Err(bad("licence is empty".into()));
    }
    match (&t.build, t.id.is_ffmpeg_family()) {
        (Some(b), true) if ffmpeg_build_parts(b, &t.version).is_some() => {}
        (None, false) => {}
        (b, _) => {
            return Err(bad(format!(
                "build {b:?} does not fit this tool and version"
            )))
        }
    }
    match (&t.id, &t.provenance) {
        (
            ToolId::YtDlp,
            Provenance::GpgSignedSums {
                sums_url,
                signature_url,
                key_fingerprint,
            },
        ) => {
            for u in [sums_url, signature_url] {
                if !u.starts_with(prefix) {
                    return Err(bad(format!("{u} is outside {prefix}")));
                }
            }
            if key_fingerprint != YTDLP_SIGNING_KEY {
                return Err(bad(format!(
                    "signing key {key_fingerprint} is not the pinned yt-dlp key"
                )));
            }
        }
        (ToolId::YtDlp, _) => {
            return Err(bad(
                "yt-dlp must be verified through its signed checksum file".into(),
            ))
        }
        (
            ToolId::Deno | ToolId::Ffmpeg | ToolId::Ffprobe | ToolId::WhisperCli,
            Provenance::PublishedSha256,
        ) => {}
        (_, _) => return Err(bad("unexpected provenance kind".into())),
    }
    if t.artifacts.is_empty() {
        return Err(bad("no artifacts".into()));
    }
    let mut platforms = Vec::new();
    for a in &t.artifacts {
        if !PLATFORMS.contains(&a.platform.as_str()) {
            return Err(bad(format!("unknown platform {}", a.platform)));
        }
        if platforms.contains(&&a.platform) {
            return Err(ManifestError::Duplicate(format!("{tool} {}", a.platform)));
        }
        platforms.push(&a.platform);
        if a.url.contains("..") || a.url.contains('?') || a.url.contains('#') {
            return Err(bad(format!("{} has a query, fragment or ..", a.url)));
        }
        let pinned = if t.id.is_ffmpeg_family() {
            ffmpeg_url_is_pinned(t, a)
        } else {
            // The version must be part of the URL, so a "latest" link cannot sneak in.
            a.url.starts_with(prefix)
                && (a.url.contains(&format!("/{}/", t.version))
                    || a.url.contains(&format!("/v{}/", t.version)))
        };
        if !pinned {
            return Err(bad(format!(
                "{} is not a pinned release URL under {prefix}",
                a.url
            )));
        }
        if !is_sha256(&a.sha256) || !is_sha256(&a.executable_sha256) {
            return Err(bad(format!(
                "{}: SHA-256 must be 64 lowercase hex characters",
                a.platform
            )));
        }
        if a.size == 0 {
            return Err(bad(format!("{}: size is 0", a.platform)));
        }
        if !is_plain_file_name(&a.executable) {
            return Err(bad(format!(
                "{}: executable {:?} is not a plain file name",
                a.platform, a.executable
            )));
        }
        match a.archive.as_deref() {
            None if a.sha256 != a.executable_sha256 || a.archive_path.is_some() => {
                return Err(bad(format!(
                    "{}: a bare executable must have one hash and no archive path",
                    a.platform
                )));
            }
            None | Some("zip") | Some("tar.xz") => {}
            Some(other) => return Err(bad(format!("{}: unsupported archive {other}", a.platform))),
        }
        if let Some(archive) = &a.archive {
            if !a.url.ends_with(&format!(".{archive}")) {
                return Err(bad(format!(
                    "{}: URL does not end in .{archive}",
                    a.platform
                )));
            }
        }
        if let Some(p) = &a.archive_path {
            if !is_archive_path(p, &a.executable) {
                return Err(bad(format!(
                    "{}: archive path {p:?} is not a plain relative path ending in the executable",
                    a.platform
                )));
            }
        }
        validate_companions(a).map_err(bad)?;
    }
    Ok(())
}

/// A `/`-separated relative path of at most four plain parts ending in `last`.
fn is_archive_path(p: &str, last: &str) -> bool {
    let parts: Vec<&str> = p.split('/').collect();
    parts.len() <= 4 && parts.iter().all(|c| is_plain_file_name(c)) && parts.last() == Some(&last)
}

fn validate_companions(a: &Artifact) -> Result<(), String> {
    if a.companions.is_empty() {
        return Ok(());
    }
    let pf = &a.platform;
    if a.archive.is_none() {
        return Err(format!("{pf}: companions need an archive"));
    }
    if a.companions.len() > MAX_COMPANIONS {
        return Err(format!("{pf}: more than {MAX_COMPANIONS} companions"));
    }
    let mut names: Vec<String> = vec![a.executable.to_ascii_lowercase()];
    for c in &a.companions {
        // Windows file names are case-insensitive: two entries that differ
        // only in case would be one file.
        let lower = c.file.to_ascii_lowercase();
        if !is_plain_file_name(&c.file) || !lower.ends_with(COMPANION_EXTENSION) {
            return Err(format!(
                "{pf}: companion {:?} is not a plain .dll name",
                c.file
            ));
        }
        if names.contains(&lower) {
            return Err(format!("{pf}: companion {:?} listed twice", c.file));
        }
        names.push(lower);
        if !is_archive_path(&c.archive_path, &c.file) {
            return Err(format!(
                "{pf}: companion path {:?} is not a plain relative path ending in {:?}",
                c.archive_path, c.file
            ));
        }
        if !is_sha256(&c.sha256) {
            return Err(format!(
                "{pf}: companion {:?} SHA-256 must be 64 lowercase hex characters",
                c.file
            ));
        }
    }
    Ok(())
}

/// Splits a BtbN build string `n9.0.2-39-g7adfbe40dc-20261031` into the git
/// description (`n9.0.2-39-g7adfbe40dc`) and the build date (`20261031`).
/// The description must start with `n<version>`: a release branch build, not
/// a `N-` master snapshot.
fn ffmpeg_build_parts<'a>(build: &'a str, version: &str) -> Option<(&'a str, &'a str)> {
    let (desc, date) = build.rsplit_once('-')?;
    let tag = desc.strip_prefix('n')?;
    let after = tag.strip_prefix(version)?;
    let ok_after = after.is_empty()
        || after.strip_prefix('-').is_some_and(|r| {
            let (n, g) = r.split_once("-g").unwrap_or(("", ""));
            !n.is_empty()
                && n.bytes().all(|b| b.is_ascii_digit())
                && (7..=40).contains(&g.len())
                && g.bytes().all(|b| b.is_ascii_hexdigit())
        });
    (ok_after && date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit()))
        .then_some((desc, date))
}

/// FFmpeg URLs are pinned by a dated BtbN release tag rather than a version
/// directory: `.../download/autobuild-2026-10-31-12-59/ffmpeg-<desc>-win64-lgpl-9.0.zip`.
/// The tag date must be the build date, the file must be this build, the
/// platform must match, and only the LGPL variant is accepted (DEC-03).
fn ffmpeg_url_is_pinned(t: &Tool, a: &Artifact) -> bool {
    let Some((desc, date)) = t
        .build
        .as_deref()
        .and_then(|b| ffmpeg_build_parts(b, &t.version))
    else {
        return false;
    };
    let Some(rest) = a
        .url
        .strip_prefix(t.id.allowed_url_prefix())
        .and_then(|r| r.strip_prefix(FFMPEG_TAG_PREFIX))
    else {
        return false;
    };
    let Some((tag, file)) = rest.split_once('/') else {
        return false;
    };
    let tag_ok = tag.len() == 16
        && tag.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 4 | 7 | 10 | 13) {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
        && tag[..10].replace('-', "") == date;
    let os = match a.platform.as_str() {
        "windows-x86_64" => "win64",
        "linux-x86_64" => "linux64",
        _ => return false,
    };
    let major_minor: String = t.version.split('.').take(2).collect::<Vec<_>>().join(".");
    let expected_stem = format!("ffmpeg-{desc}-{os}-lgpl-{major_minor}.");
    tag_ok && !file.contains('/') && file.starts_with(&expected_stem) && is_plain_file_name(file)
}
