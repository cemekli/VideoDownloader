//! STATE.md F0.5, SPEC §11.7 / §15.3 / DEC-06: migration, backup and
//! connection behaviour against real database files.

use rusqlite::Connection;
use std::path::Path;
use vv_persistence::migrations::{Migration, MigrationError, Migrator, MIGRATIONS};
use vv_persistence::{Database, OpenOptions, PersistenceError};

const M1: Migration = Migration {
    version: 1,
    name: "one",
    up: "CREATE TABLE a (id INTEGER PRIMARY KEY, v TEXT NOT NULL) STRICT;",
    down: Some("DROP TABLE a;"),
};
const M2: Migration = Migration {
    version: 2,
    name: "two",
    up: "CREATE TABLE b (id INTEGER PRIMARY KEY, a_id INTEGER NOT NULL REFERENCES a(id)) STRICT; CREATE INDEX b_a ON b(a_id);",
    down: Some("DROP TABLE b;"),
};

fn schema(c: &Connection) -> Vec<String> {
    let mut s = c
        .prepare("SELECT type || ' ' || name || ' ' || COALESCE(sql, '') FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' AND name <> 'schema_migrations' ORDER BY name")
        .unwrap();
    s.query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn version(c: &Connection) -> u32 {
    c.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |r| r.get(0),
    )
    .unwrap()
}

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn db_path(dir: &Path) -> std::path::PathBuf {
    dir.join("library.sqlite")
}

#[test]
fn fresh_database_migrates_to_latest_and_records_checksums() {
    let d = tmp();
    let mut db = Database::open(&db_path(d.path()), &OpenOptions::default()).unwrap();
    assert_eq!(db.report().schema_version, MIGRATIONS.len() as u32);
    assert_eq!(
        db.report().applied,
        (1..=MIGRATIONS.len() as u32).collect::<Vec<_>>()
    );
    assert_eq!(
        db.report().backup,
        None,
        "nothing to back up on a fresh database"
    );
    let applied = Migrator::new(MIGRATIONS)
        .unwrap()
        .applied(db.writer())
        .unwrap();
    for (a, m) in applied.iter().zip(MIGRATIONS) {
        assert_eq!(a.checksum, vv_persistence::migrations::checksum(m.up));
    }
}

#[test]
fn reopening_is_a_no_op() {
    let d = tmp();
    let p = db_path(d.path());
    drop(Database::open(&p, &OpenOptions::default()).unwrap());
    let opts = OpenOptions {
        backup_dir: Some(d.path().join("backups")),
    };
    let db = Database::open(&p, &opts).unwrap();
    assert!(db.report().applied.is_empty());
    assert_eq!(db.report().backup, None, "no migration, no backup");
}

#[test]
fn every_migration_goes_up_down_and_up_again() {
    // SPEC §11.7: reversible migrations are tested.
    let d = tmp();
    let mut db = Database::open(&db_path(d.path()), &OpenOptions::default()).unwrap();
    let m = Migrator::new(MIGRATIONS).unwrap();
    let full = schema(db.writer());
    m.migrate_down_to(db.writer(), 0).unwrap();
    assert!(
        schema(db.writer()).is_empty(),
        "down to 0 leaves no app objects"
    );
    assert_eq!(version(db.writer()), 0);
    m.migrate_up(db.writer()).unwrap();
    assert_eq!(
        schema(db.writer()),
        full,
        "up after down rebuilds the same schema"
    );
}

#[test]
fn failed_migration_leaves_no_half_schema() {
    let d = tmp();
    let p = db_path(d.path());
    drop(Database::open_with(&p, &OpenOptions::default(), &[M1]).unwrap());
    let broken = Migration {
        version: 2,
        name: "broken",
        up: "CREATE TABLE half (x); CREATE TABLE oops (",
        down: None,
    };
    let err = Database::open_with(&p, &OpenOptions::default(), &[M1, broken])
        .err()
        .expect("must fail");
    assert!(
        matches!(
            err,
            PersistenceError::Migration(MigrationError::Failed { version: 2, .. })
        ),
        "{err}"
    );
    let c = Connection::open(&p).unwrap();
    assert_eq!(version(&c), 1);
    assert!(
        !schema(&c).iter().any(|s| s.contains("half")),
        "first statement of the failed migration was rolled back"
    );
}

#[test]
fn edited_migration_is_detected() {
    let d = tmp();
    let p = db_path(d.path());
    drop(Database::open_with(&p, &OpenOptions::default(), &[M1]).unwrap());
    let edited = Migration {
        up: "CREATE TABLE a (id INTEGER PRIMARY KEY) STRICT;",
        ..M1
    };
    let err = Database::open_with(&p, &OpenOptions::default(), &[edited])
        .err()
        .unwrap();
    assert!(
        matches!(
            err,
            PersistenceError::Migration(MigrationError::ChecksumMismatch { version: 1 })
        ),
        "{err}"
    );
}

#[test]
fn database_from_a_newer_app_is_refused_and_untouched() {
    let d = tmp();
    let p = db_path(d.path());
    drop(Database::open_with(&p, &OpenOptions::default(), &[M1, M2]).unwrap());
    let before = std::fs::read(&p).unwrap();
    let err = Database::open_with(&p, &OpenOptions::default(), &[M1])
        .err()
        .unwrap();
    assert!(
        matches!(
            err,
            PersistenceError::Migration(MigrationError::UnknownVersion {
                db_version: 2,
                app_version: 1
            })
        ),
        "{err}"
    );
    let c = Connection::open(&p).unwrap();
    assert_eq!(version(&c), 2);
    drop(c);
    assert_eq!(std::fs::read(&p).unwrap().len(), before.len());
}

#[test]
fn pending_migration_takes_a_consistent_backup_first() {
    // SPEC §11.7: "Migration öncesi tutarlı yedek alınır."
    let d = tmp();
    let p = db_path(d.path());
    {
        let mut db = Database::open_with(&p, &OpenOptions::default(), &[M1]).unwrap();
        db.writer()
            .execute("INSERT INTO a (v) VALUES ('kept')", [])
            .unwrap();
    }
    let opts = OpenOptions {
        backup_dir: Some(d.path().join("backups")),
    };
    let db = Database::open_with(&p, &opts, &[M1, M2]).unwrap();
    assert_eq!(db.report().applied, vec![2]);
    let backup = db.report().backup.clone().expect("backup taken");
    assert!(backup
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("pre-migration-v1-to-v2-"));
    let b = Connection::open(&backup).unwrap();
    vv_persistence::integrity_check(&b).unwrap();
    assert_eq!(version(&b), 1, "backup is the pre-migration state");
    let v: String = b.query_row("SELECT v FROM a", [], |r| r.get(0)).unwrap();
    assert_eq!(v, "kept");
}

#[test]
fn connection_settings_are_applied() {
    let d = tmp();
    let mut db =
        Database::open_with(&db_path(d.path()), &OpenOptions::default(), &[M1, M2]).unwrap();
    let w = db.writer();
    let mode: String = w
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    let fk: i64 = w
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    let sync: i64 = w.query_row("PRAGMA synchronous", [], |r| r.get(0)).unwrap();
    assert_eq!(
        (mode.as_str(), fk, sync),
        ("wal", 1, 2),
        "WAL, foreign keys on, synchronous FULL"
    );
    let err = w
        .execute("INSERT INTO b (a_id) VALUES (999)", [])
        .unwrap_err();
    assert!(err.to_string().contains("FOREIGN KEY"), "{err}");
}

#[test]
fn readers_see_committed_snapshots_and_cannot_write() {
    let d = tmp();
    let mut db = Database::open_with(&db_path(d.path()), &OpenOptions::default(), &[M1]).unwrap();
    let reader = db.reader().unwrap();
    let count = |c: &Connection| -> i64 {
        c.query_row("SELECT COUNT(*) FROM a", [], |r| r.get(0))
            .unwrap()
    };

    let tx = db.writer().transaction().unwrap();
    tx.execute("INSERT INTO a (v) VALUES ('x')", []).unwrap();
    assert_eq!(
        count(&reader),
        0,
        "reader does not see uncommitted writes and is not blocked"
    );
    tx.commit().unwrap();
    assert_eq!(count(&reader), 1);

    assert!(
        reader
            .execute("INSERT INTO a (v) VALUES ('y')", [])
            .is_err(),
        "reader is read-only"
    );
}

#[test]
fn second_writer_waits_instead_of_failing() {
    // busy_timeout: a short write by another connection must not surface as an error.
    let d = tmp();
    let p = db_path(d.path());
    let mut db = Database::open_with(&p, &OpenOptions::default(), &[M1]).unwrap();
    let tx = db
        .writer()
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute("INSERT INTO a (v) VALUES ('first')", [])
        .unwrap();

    let p2 = p.clone();
    let other = std::thread::spawn(move || {
        let mut db2 = Database::open_with(&p2, &OpenOptions::default(), &[M1]).unwrap();
        db2.writer()
            .execute("INSERT INTO a (v) VALUES ('second')", [])
            .map(|_| ())
    });
    std::thread::sleep(std::time::Duration::from_millis(300));
    tx.commit().unwrap();
    other
        .join()
        .unwrap()
        .expect("second writer succeeded after waiting");
    let n: i64 = db
        .writer()
        .query_row("SELECT COUNT(*) FROM a", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 2);
}

#[test]
fn online_backup_while_a_write_is_open_captures_committed_state() {
    let d = tmp();
    let mut db = Database::open_with(&db_path(d.path()), &OpenOptions::default(), &[M1]).unwrap();
    db.writer()
        .execute("INSERT INTO a (v) VALUES ('committed')", [])
        .unwrap();
    let other = Connection::open(db_path(d.path())).unwrap();
    other
        .execute_batch("BEGIN IMMEDIATE; INSERT INTO a (v) VALUES ('uncommitted');")
        .unwrap();
    let dest = d.path().join("snapshot.sqlite");
    db.backup_to(&dest).unwrap();
    other.execute_batch("ROLLBACK").unwrap();
    let b = Connection::open(&dest).unwrap();
    let vals: Vec<String> = b
        .prepare("SELECT v FROM a")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(vals, vec!["committed".to_string()]);
}
