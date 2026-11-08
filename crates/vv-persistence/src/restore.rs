//! Full database backup and restore (SPEC §15.3, §11.7; STATE.md F2.6;
//! AC-50, AC-57; ADR-0017).
//!
//! A backup is a consistent SQLite snapshot made with the online backup API
//! (never a blind file copy). Restoring:
//!
//! 1. loads the backup into memory and checks it there: SQLite integrity, a
//!    VideoVault schema, migration checksums this app knows, not newer than
//!    this app;
//! 2. migrates the in-memory copy up to the current schema and starts the
//!    Recently removed retention again from now (AC-57);
//! 3. saves the current library as a `pre-restore` backup;
//! 4. copies the checked image over the live database in one step.
//!
//! Nothing in the live database changes before step 4, so a bad file leaves
//! the library exactly as it was.

use crate::migrations::{MigrationError, Migrator, MIGRATIONS};
use crate::{integrity_check, Database, PersistenceError};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// File name prefixes in `<data>/backups/` (one folder for every kind).
pub const MANUAL_PREFIX: &str = "backup-";
pub const PRE_RESTORE_PREFIX: &str = "pre-restore-";
pub const PRE_MIGRATION_PREFIX: &str = "pre-migration-";

#[derive(Debug)]
pub enum RestoreError {
    /// Not a readable SQLite file, or it failed `integrity_check`.
    Unreadable,
    /// A SQLite database, but not a VideoVault library.
    NotALibrary,
    /// Written by a newer app (IMPORT_SCHEMA_UNSUPPORTED).
    TooNew {
        backup: u32,
        app: u32,
    },
    /// Its migrations differ from this app's (edited or foreign).
    Mismatch,
    /// Bringing it to the current schema failed (MIGRATION_FAILED).
    Migration(MigrationError),
    Io(String),
    Storage(PersistenceError),
}

impl std::fmt::Display for RestoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RestoreError::Unreadable => write!(f, "backup is not a readable database"),
            RestoreError::NotALibrary => write!(f, "backup is not a VideoVault library"),
            RestoreError::TooNew { backup, app } => {
                write!(f, "backup schema {backup} is newer than this app ({app})")
            }
            RestoreError::Mismatch => write!(f, "backup migrations do not match this app"),
            RestoreError::Migration(e) => write!(f, "{e}"),
            RestoreError::Io(e) => write!(f, "{e}"),
            RestoreError::Storage(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for RestoreError {}

impl From<rusqlite::Error> for RestoreError {
    fn from(e: rusqlite::Error) -> Self {
        RestoreError::Storage(PersistenceError::Sqlite(e))
    }
}

impl From<PersistenceError> for RestoreError {
    fn from(e: PersistenceError) -> Self {
        RestoreError::Storage(e)
    }
}

/// What a backup holds, shown before it is restored (AC-57: the preview
/// names how many removed items start their retention again).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupSummary {
    pub schema_version: u32,
    /// Items not in Recently removed.
    pub items: i64,
    /// Items in Recently removed; their 30 days start again at restore.
    pub removed: i64,
    pub collections: i64,
    pub tags: i64,
    /// Original media files referenced; they are not inside the backup.
    pub media_files: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreReport {
    pub summary: BackupSummary,
    /// Where the library was saved before it was replaced.
    pub pre_restore_backup: PathBuf,
    /// Migrations applied to bring the backup to this app's schema.
    pub migrated: Vec<u32>,
}

fn copy(src: &Connection, dest: &mut Connection) -> rusqlite::Result<()> {
    let b = rusqlite::backup::Backup::new(src, dest)?;
    b.run_to_completion(256, Duration::from_millis(5), None)
}

/// Loads `path` into memory without touching it.
fn load(path: &Path) -> Result<Connection, RestoreError> {
    if !path.is_file() {
        return Err(RestoreError::Unreadable);
    }
    let src = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| RestoreError::Unreadable)?;
    let mut mem = Connection::open_in_memory()?;
    copy(&src, &mut mem).map_err(|_| RestoreError::Unreadable)?;
    integrity_check(&mem).map_err(|_| RestoreError::Unreadable)?;
    mem.pragma_update(None, "foreign_keys", true)?;
    mem.pragma_update(None, "trusted_schema", false)?;
    Ok(mem)
}

fn has_table(c: &Connection, name: &str) -> rusqlite::Result<bool> {
    Ok(c.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |_| Ok(()),
    )
    .optional()?
    .is_some())
}

/// Checks the in-memory copy and returns its schema version.
fn check(mem: &Connection) -> Result<u32, RestoreError> {
    if !has_table(mem, "schema_migrations")? || !has_table(mem, "app_meta")? {
        return Err(RestoreError::NotALibrary);
    }
    let migrator = Migrator::new(MIGRATIONS).map_err(RestoreError::Migration)?;
    match migrator.verified_version(mem) {
        Ok(v) if v >= 1 => Ok(v),
        Ok(_) => Err(RestoreError::NotALibrary),
        Err(MigrationError::UnknownVersion { db_version, .. }) => Err(RestoreError::TooNew {
            backup: db_version,
            app: migrator.latest(),
        }),
        Err(MigrationError::ChecksumMismatch { .. }) => Err(RestoreError::Mismatch),
        Err(e) => Err(RestoreError::Migration(e)),
    }
}

fn count(c: &Connection, sql: &str) -> rusqlite::Result<i64> {
    c.query_row(sql, [], |r| r.get(0))
}

fn summarize(mem: &Connection, version: u32) -> Result<BackupSummary, RestoreError> {
    let (items, removed, media) = if has_table(mem, "library_item")? {
        (
            count(
                mem,
                "SELECT count(*) FROM library_item WHERE removed_at IS NULL",
            )?,
            count(
                mem,
                "SELECT count(*) FROM library_item WHERE removed_at IS NOT NULL",
            )?,
            count(
                mem,
                "SELECT count(*) FROM media_asset WHERE role = 'original'",
            )?,
        )
    } else {
        (0, 0, 0)
    };
    let (collections, tags) = if has_table(mem, "collection")? {
        (
            count(mem, "SELECT count(*) FROM collection")?,
            count(mem, "SELECT count(*) FROM tag")?,
        )
    } else {
        (0, 0)
    };
    Ok(BackupSummary {
        schema_version: version,
        items,
        removed,
        collections,
        tags,
        media_files: media,
    })
}

/// Reads a backup without changing anything.
pub fn inspect(path: &Path) -> Result<BackupSummary, RestoreError> {
    let mem = load(path)?;
    let v = check(&mem)?;
    summarize(&mem, v)
}

fn stamp_name(prefix: &str, now_ms: i64) -> String {
    format!("{prefix}{now_ms}.sqlite")
}

/// A manual backup into `dir` (`backup-<ms>.sqlite`), or to `dest` exactly.
pub fn backup_now(db: &Database, dir: &Path, now_ms: i64) -> Result<PathBuf, RestoreError> {
    std::fs::create_dir_all(dir).map_err(|e| RestoreError::Io(e.to_string()))?;
    let path = dir.join(stamp_name(MANUAL_PREFIX, now_ms));
    db.backup_to(&path)?;
    Ok(path)
}

/// Replaces the live library with the backup at `path` (see module docs).
pub fn restore(
    db: &mut Database,
    path: &Path,
    backup_dir: &Path,
    now_ms: i64,
) -> Result<RestoreReport, RestoreError> {
    let mut mem = load(path)?;
    let version = check(&mem)?;
    let summary = summarize(&mem, version)?;
    let migrator = Migrator::new(MIGRATIONS).map_err(RestoreError::Migration)?;
    let migrated = migrator
        .migrate_up(&mut mem)
        .map_err(RestoreError::Migration)?;
    {
        let tx = mem.transaction()?;
        // AC-57: retention counts from the restore, not from the old date.
        tx.execute(
            "UPDATE library_item SET removed_at = ?1 WHERE removed_at IS NOT NULL",
            [now_ms],
        )?;
        // Undo belongs to the run that made the change (§7.3).
        tx.execute("DELETE FROM operation_log", [])?;
        crate::library::repo::rebuild_search_index(&tx).map_err(|_| RestoreError::Unreadable)?;
        tx.commit()?;
    }
    integrity_check(&mem)?;

    std::fs::create_dir_all(backup_dir).map_err(|e| RestoreError::Io(e.to_string()))?;
    let pre = backup_dir.join(stamp_name(PRE_RESTORE_PREFIX, now_ms));
    db.backup_to(&pre)?;

    copy(&mem, db.writer())?;
    integrity_check(db.writer())?;
    Ok(RestoreReport {
        summary,
        pre_restore_backup: pre,
        migrated,
    })
}

/// One file in the backups folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupFile {
    pub path: PathBuf,
    pub kind: BackupKind,
    pub created_ms: i64,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupKind {
    Manual,
    PreRestore,
    PreMigration,
}

impl BackupKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BackupKind::Manual => "manual",
            BackupKind::PreRestore => "pre_restore",
            BackupKind::PreMigration => "pre_migration",
        }
    }
}

/// Backups in `dir`, newest first. Only our own file names are listed.
pub fn list(dir: &Path) -> Vec<BackupFile> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<BackupFile> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let stem = name.strip_suffix(".sqlite")?;
            let (kind, rest) = if let Some(r) = stem.strip_prefix(MANUAL_PREFIX) {
                (BackupKind::Manual, r)
            } else if let Some(r) = stem.strip_prefix(PRE_RESTORE_PREFIX) {
                (BackupKind::PreRestore, r)
            } else {
                let r = stem.strip_prefix(PRE_MIGRATION_PREFIX)?;
                // pre-migration-v<from>-to-v<to>-<ms>
                (BackupKind::PreMigration, r.rsplit('-').next()?)
            };
            let created_ms: i64 = rest.parse().ok()?;
            let meta = e.metadata().ok().filter(|m| m.is_file())?;
            Some(BackupFile {
                path: e.path(),
                kind,
                created_ms,
                size_bytes: meta.len(),
            })
        })
        .collect();
    out.sort_by_key(|b| std::cmp::Reverse(b.created_ms));
    out
}
