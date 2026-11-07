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
}

impl ToolId {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolId::YtDlp => "yt-dlp",
            ToolId::Deno => "deno",
        }
    }

    /// The only place this tool's files may be downloaded from.
    pub fn allowed_url_prefix(self) -> &'static str {
        match self {
            ToolId::YtDlp => "https://github.com/yt-dlp/yt-dlp/releases/download/",
            ToolId::Deno => "https://github.com/denoland/deno/releases/download/",
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
    /// Only a published SHA-256 next to the release asset (Deno).
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
    /// `zip` when the download is an archive; absent for a bare executable.
    #[serde(default)]
    pub archive: Option<String>,
    /// File name of the executable (inside the archive, if any).
    pub executable: String,
    /// SHA-256 of the executable itself; checked before every first run.
    pub executable_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tool {
    pub id: ToolId,
    pub version: String,
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
        for id in [ToolId::YtDlp, ToolId::Deno] {
            if self.tool(id).is_none() {
                return Err(ManifestError::Missing(id.as_str()));
            }
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
        (ToolId::Deno, Provenance::PublishedSha256) => {}
        (ToolId::Deno, _) => return Err(bad("unexpected provenance kind".into())),
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
        // The version must be part of the URL, so a "latest" link cannot sneak in.
        if !a.url.starts_with(prefix)
            || !a.url.contains(&format!("/{}/", t.version))
                && !a.url.contains(&format!("/v{}/", t.version))
        {
            return Err(bad(format!(
                "{} is not a pinned release URL under {prefix}",
                a.url
            )));
        }
        if a.url.contains("..") || a.url.contains('?') || a.url.contains('#') {
            return Err(bad(format!("{} has a query, fragment or ..", a.url)));
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
            None if a.sha256 != a.executable_sha256 => {
                return Err(bad(format!(
                    "{}: a bare executable must have one hash",
                    a.platform
                )));
            }
            None | Some("zip") => {}
            Some(other) => return Err(bad(format!("{}: unsupported archive {other}", a.platform))),
        }
    }
    Ok(())
}
