//! Installed transcription models (SPEC §11.5 `ModelInstallation`, §17.1;
//! STATE.md F6.1; ADR-0026).
//!
//! Rules kept here:
//! - A row is written only by [`consent`]: no row means the user never
//!   agreed and nothing was downloaded (AC-20).
//! - The row records what was agreed to (version, size, hash, licence,
//!   source). A different catalog entry is a new consent, never a silent
//!   upgrade of the old one.
//! - `ready` needs a hash check ([`mark_ready`] takes the verification
//!   time); a failed or missing file is never reported as ready.

use rusqlite::{params, Connection, OptionalExtension, Row};

#[derive(Debug)]
pub enum ModelError {
    Sqlite(rusqlite::Error),
    NotFound,
    Invalid(&'static str),
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelError::Sqlite(e) => write!(f, "sqlite: {e}"),
            ModelError::NotFound => write!(f, "model not installed"),
            ModelError::Invalid(m) => write!(f, "invalid model: {m}"),
        }
    }
}

impl std::error::Error for ModelError {}

impl From<rusqlite::Error> for ModelError {
    fn from(e: rusqlite::Error) -> Self {
        ModelError::Sqlite(e)
    }
}

pub type Result<T> = std::result::Result<T, ModelError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelStatus {
    /// Agreed to; the file is not (yet) verified.
    Pending,
    Ready,
    Missing,
    Corrupt,
}

impl ModelStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelStatus::Pending => "pending",
            ModelStatus::Ready => "ready",
            ModelStatus::Missing => "missing",
            ModelStatus::Corrupt => "corrupt",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "pending" => ModelStatus::Pending,
            "ready" => ModelStatus::Ready,
            "missing" => ModelStatus::Missing,
            "corrupt" => ModelStatus::Corrupt,
            _ => return None,
        })
    }
}

/// What the user agreed to download, copied from the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSpec {
    pub id: String,
    pub provider: String,
    pub version: String,
    pub file_name: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub license: String,
    pub source_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    pub spec: ModelSpec,
    pub status: ModelStatus,
    pub consent_at: i64,
    pub installed_at: Option<i64>,
    pub verified_at: Option<i64>,
    pub updated_at: i64,
}

impl Installation {
    /// The row describes exactly this catalog entry (same file, hash and
    /// source); otherwise using it needs a new consent.
    pub fn matches(&self, spec: &ModelSpec) -> bool {
        &self.spec == spec
    }
}

const COLS: &str = "id, provider, version, file_name, size_bytes, sha256, license, source_url, \
                    status, consent_at, installed_at, verified_at, updated_at";

fn from_row(r: &Row) -> rusqlite::Result<Installation> {
    let status: String = r.get(8)?;
    Ok(Installation {
        spec: ModelSpec {
            id: r.get(0)?,
            provider: r.get(1)?,
            version: r.get(2)?,
            file_name: r.get(3)?,
            size_bytes: r.get(4)?,
            sha256: r.get(5)?,
            license: r.get(6)?,
            source_url: r.get(7)?,
        },
        status: ModelStatus::parse(&status).unwrap_or(ModelStatus::Corrupt),
        consent_at: r.get(9)?,
        installed_at: r.get(10)?,
        verified_at: r.get(11)?,
        updated_at: r.get(12)?,
    })
}

pub fn get(c: &Connection, id: &str) -> Result<Option<Installation>> {
    Ok(c.query_row(
        &format!("SELECT {COLS} FROM model_installation WHERE id = ?1"),
        [id],
        from_row,
    )
    .optional()?)
}

pub fn list(c: &Connection) -> Result<Vec<Installation>> {
    let mut st = c.prepare(&format!(
        "SELECT {COLS} FROM model_installation ORDER BY id"
    ))?;
    let rows = st.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Records the user's agreement to download `spec` (§17.1). A row for an
/// older catalog entry of the same model is replaced and goes back to
/// `pending`: the new file has not been checked.
pub fn consent(c: &Connection, spec: &ModelSpec, now: i64) -> Result<Installation> {
    if spec.sha256.len() != 64 || spec.size_bytes <= 0 {
        return Err(ModelError::Invalid("model is not pinned"));
    }
    if let Some(cur) = get(c, &spec.id)? {
        if cur.matches(spec) {
            return Ok(cur);
        }
    }
    c.execute(
        "INSERT INTO model_installation (id, provider, version, file_name, size_bytes, sha256, \
             license, source_url, status, consent_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending', ?9, ?9) \
         ON CONFLICT (id) DO UPDATE SET provider = excluded.provider, version = excluded.version, \
             file_name = excluded.file_name, size_bytes = excluded.size_bytes, \
             sha256 = excluded.sha256, license = excluded.license, \
             source_url = excluded.source_url, status = 'pending', \
             consent_at = excluded.consent_at, installed_at = NULL, verified_at = NULL, \
             updated_at = excluded.updated_at",
        params![
            spec.id,
            spec.provider,
            spec.version,
            spec.file_name,
            spec.size_bytes,
            spec.sha256,
            spec.license,
            spec.source_url,
            now
        ],
    )?;
    get(c, &spec.id)?.ok_or(ModelError::NotFound)
}

/// The file was hashed at `verified_at` and matched the agreed hash.
pub fn mark_ready(c: &Connection, id: &str, verified_at: i64) -> Result<()> {
    let n = c.execute(
        "UPDATE model_installation SET status = 'ready', \
             installed_at = coalesce(installed_at, ?2), verified_at = ?2, updated_at = ?2 \
         WHERE id = ?1",
        params![id, verified_at],
    )?;
    if n == 0 {
        return Err(ModelError::NotFound);
    }
    Ok(())
}

/// A later check found the file gone (`Missing`) or different (`Corrupt`),
/// or a download is starting again (`Pending`).
pub fn set_status(c: &Connection, id: &str, status: ModelStatus, now: i64) -> Result<()> {
    if status == ModelStatus::Ready {
        return Err(ModelError::Invalid("ready needs a hash check"));
    }
    let n = c.execute(
        "UPDATE model_installation SET status = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, status.as_str(), now],
    )?;
    if n == 0 {
        return Err(ModelError::NotFound);
    }
    Ok(())
}

/// Forgets the model and its consent. The caller deletes the file.
pub fn remove(c: &Connection, id: &str) -> Result<Option<Installation>> {
    let cur = get(c, id)?;
    c.execute("DELETE FROM model_installation WHERE id = ?1", [id])?;
    Ok(cur)
}
