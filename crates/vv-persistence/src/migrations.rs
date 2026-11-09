//! Schema migrations (SPEC §11.7).
//!
//! - Every applied migration is recorded with its version, name and the
//!   SHA-256 of its `up` SQL. A changed checksum stops startup: the code no
//!   longer matches what built this database.
//! - Each migration runs in its own transaction, so a failure leaves the schema
//!   exactly as it was before that migration (no half schema).
//! - A database written by a newer app version is refused, never "fixed".
//! - `down` scripts exist so reversibility can be tested. In production the
//!   supported way back is: old app version + the pre-migration backup.

use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub up: &'static str,
    pub down: Option<&'static str>,
}

/// The app's migrations, in order. Append only; never edit an applied one.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "init",
        up: include_str!("../migrations/0001_init.up.sql"),
        down: Some(include_str!("../migrations/0001_init.down.sql")),
    },
    Migration {
        version: 2,
        name: "library",
        up: include_str!("../migrations/0002_library.up.sql"),
        down: Some(include_str!("../migrations/0002_library.down.sql")),
    },
    Migration {
        version: 3,
        name: "organize",
        up: include_str!("../migrations/0003_organize.up.sql"),
        down: Some(include_str!("../migrations/0003_organize.down.sql")),
    },
    Migration {
        version: 4,
        name: "playback_settings",
        up: include_str!("../migrations/0004_playback_settings.up.sql"),
        down: Some(include_str!(
            "../migrations/0004_playback_settings.down.sql"
        )),
    },
    Migration {
        version: 5,
        name: "jobs",
        up: include_str!("../migrations/0005_jobs.up.sql"),
        down: Some(include_str!("../migrations/0005_jobs.down.sql")),
    },
    Migration {
        version: 6,
        name: "downloads",
        up: include_str!("../migrations/0006_downloads.up.sql"),
        down: Some(include_str!("../migrations/0006_downloads.down.sql")),
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedMigration {
    pub version: u32,
    pub name: String,
    pub checksum: String,
}

#[derive(Debug)]
pub enum MigrationError {
    Sqlite(rusqlite::Error),
    /// Migration list itself is malformed (gap, duplicate, wrong order).
    InvalidList(String),
    /// An applied migration's SQL changed since it ran.
    ChecksumMismatch {
        version: u32,
    },
    /// The database has a migration this app does not know (newer app wrote it).
    UnknownVersion {
        db_version: u32,
        app_version: u32,
    },
    /// `down` requested past a migration without a down script.
    NotReversible {
        version: u32,
    },
    /// A migration failed; its transaction was rolled back.
    Failed {
        version: u32,
        source: rusqlite::Error,
    },
}

impl std::fmt::Display for MigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MigrationError::Sqlite(e) => write!(f, "sqlite: {e}"),
            MigrationError::InvalidList(m) => write!(f, "invalid migration list: {m}"),
            MigrationError::ChecksumMismatch { version } => {
                write!(f, "migration {version} changed after it was applied")
            }
            MigrationError::UnknownVersion {
                db_version,
                app_version,
            } => {
                write!(
                    f,
                    "database is at schema {db_version}, this app knows up to {app_version}"
                )
            }
            MigrationError::NotReversible { version } => {
                write!(f, "migration {version} has no down script")
            }
            MigrationError::Failed { version, source } => write!(
                f,
                "migration {version} failed and was rolled back: {source}"
            ),
        }
    }
}

impl std::error::Error for MigrationError {}

impl From<rusqlite::Error> for MigrationError {
    fn from(e: rusqlite::Error) -> Self {
        MigrationError::Sqlite(e)
    }
}

pub fn checksum(sql: &str) -> String {
    Sha256::digest(sql.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

const CREATE_TABLE: &str = "CREATE TABLE IF NOT EXISTS schema_migrations (
    version       INTEGER PRIMARY KEY NOT NULL,
    name          TEXT NOT NULL,
    checksum      TEXT NOT NULL,
    applied_at_ms INTEGER NOT NULL
) STRICT";

pub struct Migrator<'a> {
    migrations: &'a [Migration],
}

impl<'a> Migrator<'a> {
    pub fn new(migrations: &'a [Migration]) -> Result<Self, MigrationError> {
        for (i, m) in migrations.iter().enumerate() {
            let expected = i as u32 + 1;
            if m.version != expected {
                return Err(MigrationError::InvalidList(format!(
                    "position {i} has version {}, expected {expected}",
                    m.version
                )));
            }
        }
        Ok(Migrator { migrations })
    }

    pub fn latest(&self) -> u32 {
        self.migrations.len() as u32
    }

    pub fn applied(&self, conn: &Connection) -> Result<Vec<AppliedMigration>, MigrationError> {
        conn.execute(CREATE_TABLE, [])?;
        let mut stmt =
            conn.prepare("SELECT version, name, checksum FROM schema_migrations ORDER BY version")?;
        let rows = stmt.query_map([], |r| {
            Ok(AppliedMigration {
                version: r.get(0)?,
                name: r.get(1)?,
                checksum: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Current schema version after verifying every applied checksum.
    pub fn verified_version(&self, conn: &Connection) -> Result<u32, MigrationError> {
        let applied = self.applied(conn)?;
        for a in &applied {
            let Some(m) = self.migrations.get(a.version as usize - 1) else {
                return Err(MigrationError::UnknownVersion {
                    db_version: a.version,
                    app_version: self.latest(),
                });
            };
            if checksum(m.up) != a.checksum {
                return Err(MigrationError::ChecksumMismatch { version: a.version });
            }
        }
        Ok(applied.last().map_or(0, |a| a.version))
    }

    /// Applies every pending migration. Returns the versions applied.
    pub fn migrate_up(&self, conn: &mut Connection) -> Result<Vec<u32>, MigrationError> {
        let current = self.verified_version(conn)?;
        let mut done = Vec::new();
        for m in &self.migrations[current as usize..] {
            let tx = conn.transaction()?;
            let run = tx.execute_batch(m.up).and_then(|_| {
                tx.execute(
                    "INSERT INTO schema_migrations (version, name, checksum, applied_at_ms) VALUES (?1, ?2, ?3, ?4)",
                    params![m.version, m.name, checksum(m.up), now_ms()],
                )
            });
            match run {
                Ok(_) => tx.commit()?,
                Err(source) => {
                    return Err(MigrationError::Failed {
                        version: m.version,
                        source,
                    })
                } // tx drops → rollback
            }
            done.push(m.version);
        }
        Ok(done)
    }

    /// Reverts down to `target` (exclusive). Test and recovery tooling only.
    pub fn migrate_down_to(
        &self,
        conn: &mut Connection,
        target: u32,
    ) -> Result<(), MigrationError> {
        let current = self.verified_version(conn)?;
        for v in (target + 1..=current).rev() {
            let m = &self.migrations[v as usize - 1];
            let down = m.down.ok_or(MigrationError::NotReversible { version: v })?;
            let tx = conn.transaction()?;
            let run = tx
                .execute_batch(down)
                .and_then(|_| tx.execute("DELETE FROM schema_migrations WHERE version = ?1", [v]));
            match run {
                Ok(_) => tx.commit()?,
                Err(source) => return Err(MigrationError::Failed { version: v, source }),
            }
        }
        Ok(())
    }
}

/// Highest version recorded, without checksum checks (used before backups).
pub fn recorded_version(conn: &Connection) -> Result<u32, rusqlite::Error> {
    let exists: Option<String> = conn
        .query_row(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Ok(0);
    }
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |r| r.get(0),
    )
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_migration_list_is_valid_and_reversible() {
        let m = Migrator::new(MIGRATIONS).unwrap();
        assert_eq!(m.latest(), MIGRATIONS.len() as u32);
        assert!(
            MIGRATIONS.iter().all(|m| m.down.is_some()),
            "every F0/F1 migration should be testably reversible"
        );
    }

    #[test]
    fn rejects_gaps_in_the_list() {
        let bad = [Migration {
            version: 2,
            name: "x",
            up: "",
            down: None,
        }];
        assert!(matches!(
            Migrator::new(&bad),
            Err(MigrationError::InvalidList(_))
        ));
    }

    #[test]
    fn checksum_is_stable_sha256() {
        assert_eq!(
            checksum(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
