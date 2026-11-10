//! VideoVault SQLite storage (SPEC §11, §15.3, §16.1; ADR-0002).
//!
//! One writer connection per process plus read-only reader connections, WAL
//! journal, foreign keys on, `synchronous = FULL`. Opening a database runs
//! pending migrations, after taking a consistent backup when the database
//! already holds data.

pub mod auth;
pub mod classify;
pub mod jobs;
pub mod library;
pub mod lock;
pub mod migrations;
pub mod models;
pub mod portable;
pub mod restore;
pub mod settings;
pub mod transcripts;

use migrations::{recorded_version, Migration, MigrationError, Migrator, MIGRATIONS};
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub use rusqlite;

/// How long a connection waits for another writer before failing with BUSY.
pub const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub enum PersistenceError {
    Sqlite(rusqlite::Error),
    Migration(MigrationError),
    /// The database is not in WAL mode after asking for it (e.g. network drive).
    WalUnavailable {
        mode: String,
    },
    Backup {
        path: PathBuf,
        source: rusqlite::Error,
    },
    Integrity(String),
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PersistenceError::Sqlite(e) => write!(f, "sqlite: {e}"),
            PersistenceError::Migration(e) => write!(f, "{e}"),
            PersistenceError::WalUnavailable { mode } => {
                write!(f, "WAL journal unavailable (got {mode})")
            }
            PersistenceError::Backup { path, source } => {
                write!(f, "backup to {} failed: {source}", path.display())
            }
            PersistenceError::Integrity(m) => write!(f, "integrity check failed: {m}"),
        }
    }
}

impl std::error::Error for PersistenceError {}

impl From<rusqlite::Error> for PersistenceError {
    fn from(e: rusqlite::Error) -> Self {
        PersistenceError::Sqlite(e)
    }
}

impl From<MigrationError> for PersistenceError {
    fn from(e: MigrationError) -> Self {
        PersistenceError::Migration(e)
    }
}

pub type Result<T> = std::result::Result<T, PersistenceError>;

#[derive(Debug, Clone, Default)]
pub struct OpenOptions {
    /// Where pre-migration backups go (`<data>/backups/`). `None` disables them.
    pub backup_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenReport {
    pub schema_version: u32,
    pub applied: Vec<u32>,
    pub backup: Option<PathBuf>,
}

pub struct Database {
    path: PathBuf,
    writer: Connection,
    report: OpenReport,
}

impl Database {
    /// Opens (creating if needed) and migrates the app database.
    pub fn open(path: &Path, opts: &OpenOptions) -> Result<Database> {
        Self::open_with(path, opts, MIGRATIONS)
    }

    /// Same as [`Database::open`] with an explicit migration list (tests).
    pub fn open_with(path: &Path, opts: &OpenOptions, list: &[Migration]) -> Result<Database> {
        let migrator = Migrator::new(list)?;
        let mut writer = Connection::open(path)?;
        configure_writer(&writer)?;

        let before = migrator.verified_version(&writer)?;
        let backup = if before > 0 && before < migrator.latest() {
            match &opts.backup_dir {
                Some(dir) => Some(backup_before_migration(
                    &writer,
                    dir,
                    before,
                    migrator.latest(),
                )?),
                None => None,
            }
        } else {
            None
        };
        let applied = migrator.migrate_up(&mut writer)?;
        let report = OpenReport {
            schema_version: migrator.latest(),
            applied,
            backup,
        };
        Ok(Database {
            path: path.to_owned(),
            writer,
            report,
        })
    }

    pub fn report(&self) -> &OpenReport {
        &self.report
    }

    pub fn writer(&mut self) -> &mut Connection {
        &mut self.writer
    }

    /// A read-only connection. Readers never block the writer in WAL mode and
    /// see a consistent snapshot per transaction.
    pub fn reader(&self) -> Result<Connection> {
        open_reader(&self.path)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Consistent snapshot of the whole database while it is in use (SPEC §15.3:
    /// no blind file copy). Uses SQLite's online backup API.
    pub fn backup_to(&self, dest: &Path) -> Result<()> {
        backup(&self.writer, dest)
    }

    pub fn integrity_check(&self) -> Result<()> {
        integrity_check(&self.writer)
    }
}

/// A read-only connection to the database at `path`, configured like
/// [`Database::reader`]. Lets readers run while another thread holds the
/// writer (for example during an import).
pub fn open_reader(path: &Path) -> Result<Connection> {
    let c = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI,
    )?;
    c.busy_timeout(BUSY_TIMEOUT)?;
    c.pragma_update(None, "query_only", true)?;
    c.pragma_update(None, "trusted_schema", false)?;
    Ok(c)
}

fn configure_writer(c: &Connection) -> Result<()> {
    c.busy_timeout(BUSY_TIMEOUT)?;
    c.pragma_update(None, "foreign_keys", true)?;
    // Schema objects never run app-defined SQL functions on their own.
    c.pragma_update(None, "trusted_schema", false)?;
    let mode: String = c.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get(0))?;
    // In-memory databases report "memory"; only files must be WAL.
    if !mode.eq_ignore_ascii_case("wal") && !mode.eq_ignore_ascii_case("memory") {
        return Err(PersistenceError::WalUnavailable { mode });
    }
    // FULL: a commit the user saw survives power loss. Writes are small and rare.
    c.pragma_update(None, "synchronous", "FULL")?;
    Ok(())
}

fn backup(src: &Connection, dest: &Path) -> Result<()> {
    let mut out = Connection::open(dest).map_err(|source| PersistenceError::Backup {
        path: dest.to_owned(),
        source,
    })?;
    let b = rusqlite::backup::Backup::new(src, &mut out).map_err(|source| {
        PersistenceError::Backup {
            path: dest.to_owned(),
            source,
        }
    })?;
    b.run_to_completion(256, Duration::from_millis(10), None)
        .map_err(|source| PersistenceError::Backup {
            path: dest.to_owned(),
            source,
        })?;
    drop(b);
    integrity_check(&out)
}

fn backup_before_migration(c: &Connection, dir: &Path, from: u32, to: u32) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)
        .map_err(|e| PersistenceError::Integrity(format!("cannot create backup dir: {e}")))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let path = dir.join(format!("pre-migration-v{from}-to-v{to}-{stamp}.sqlite"));
    debug_assert_eq!(recorded_version(c).ok(), Some(from));
    backup(c, &path)?;
    Ok(path)
}

pub fn integrity_check(c: &Connection) -> Result<()> {
    let r: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if r == "ok" {
        Ok(())
    } else {
        Err(PersistenceError::Integrity(r))
    }
}

/// Facts about the SQLite linked into this binary (SPEC §16.1: verify the
/// shipped SQLite, not whatever the developer machine has).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteInfo {
    pub version: String,
    pub fts5: bool,
}

pub fn sqlite_info() -> Result<SqliteInfo> {
    let c = Connection::open_in_memory()?;
    let version: String = c.query_row("SELECT sqlite_version()", [], |r| r.get(0))?;
    // Creating a table proves the module works, not only that it was compiled in.
    let fts5 = c
        .execute_batch("CREATE VIRTUAL TABLE temp.vv_probe USING fts5(x)")
        .is_ok();
    Ok(SqliteInfo { version, fts5 })
}
