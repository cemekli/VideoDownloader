//! SPEC §16.1: FTS5 must be available in the SQLite we ship. `bundled` links
//! the same SQLite on every OS; the desktop app re-checks it at runtime
//! (`get_diagnostics`, e2e-desktop) on the real Windows build.

use rusqlite::{params, Connection};

#[test]
fn shipped_sqlite_has_fts5() {
    let info = vv_persistence::sqlite_info().unwrap();
    assert!(info.fts5, "FTS5 missing from SQLite {}", info.version);
    let (major, minor): (u32, u32) = {
        let mut it = info.version.split('.').map(|p| p.parse().unwrap());
        (it.next().unwrap(), it.next().unwrap())
    };
    assert!(
        (major, minor) >= (3, 45),
        "unexpectedly old SQLite {}",
        info.version
    );
}

#[test]
fn fts5_match_bm25_and_parameterised_queries_work() {
    // Smoke test of the features F1.6 builds on; ranking weights come later.
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE VIRTUAL TABLE doc USING fts5(title, notes, tokenize = 'unicode61 remove_diacritics 2');").unwrap();
    let rows = [
        ("PostgreSQL replication deep dive", ""),
        ("Cooking pasta", "replication of grandma's recipe"),
        ("Unrelated", ""),
    ];
    for (t, n) in rows {
        c.execute(
            "INSERT INTO doc (title, notes) VALUES (?1, ?2)",
            params![t, n],
        )
        .unwrap();
    }
    let mut q = c
        .prepare("SELECT title FROM doc WHERE doc MATCH ?1 ORDER BY bm25(doc, 10.0, 1.0), rowid")
        .unwrap();
    let hits: Vec<String> = q
        .query_map(["replication"], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        hits,
        vec!["PostgreSQL replication deep dive", "Cooking pasta"],
        "title match ranks above notes match"
    );

    // User text is bound as a parameter: FTS syntax inside it is quoted, not executed as SQL.
    let quoted = format!("\"{}\"", "x') OR 1=1 --".replace('"', "\"\""));
    let n = q
        .query_map([quoted], |r| r.get::<_, String>(0))
        .unwrap()
        .count();
    assert_eq!(n, 0);
}
