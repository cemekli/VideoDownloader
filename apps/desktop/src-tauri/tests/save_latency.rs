//! NFR-01 on the real schema (STATE.md F1.8): Save Bookmark on a library of
//! 10,000 items, measured from the call to the row the UI shows (commit +
//! list DTO), through the same `save` the `save_url` command runs. The
//! database is opened like the app's (WAL, `synchronous=FULL`).
//!
//! Writes `test-artifacts/nfr-01/report.json`. Debug builds are slower than
//! the shipped app, so a pass here is a pass with margin.

use std::time::Instant;
use vv_desktop::capture_commands::save;
use vv_persistence::library::repo::{self, LibraryItem};
use vv_persistence::library::{ItemKind, SourcePlatform, SystemClock};
use vv_persistence::{Database, OpenOptions};

const LIBRARY: usize = 10_000;
const SAMPLES: usize = 300;
const BUDGET_MS: f64 = 500.0;

#[test]
fn save_bookmark_p95_within_budget_on_10k_items() {
    let d = tempfile::tempdir().unwrap();
    let mut db = Database::open(&d.path().join("library.sqlite"), &OpenOptions::default()).unwrap();
    let c = db.writer();
    let tx = c.transaction().unwrap();
    for i in 0..LIBRARY {
        let mut it = LibraryItem::new(
            ItemKind::Bookmark,
            SourcePlatform::Youtube,
            1_000 + i as i64,
        );
        it.canonical_url = Some(format!("https://www.youtube.com/watch?v=lib{i:08}"));
        it.source_id = Some(format!("lib{i:08}"));
        it.title = Some(format!("Kütüphane videosu {i} deniz dağ şehir"));
        repo::insert_item(&tx, &it).unwrap();
    }
    tx.commit().unwrap();
    c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); ANALYZE;")
        .unwrap();

    let mut v = Vec::with_capacity(SAMPLES);
    for k in 0..SAMPLES {
        let url = format!("https://youtu.be/new{k:08}");
        let t = Instant::now();
        let r = save(c, &SystemClock, &url).unwrap();
        v.push(t.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(r.outcome, "created");
    }
    assert_eq!(repo::count_items(c).unwrap() as usize, LIBRARY + SAMPLES);

    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| v[((v.len() as f64 * p).ceil() as usize).clamp(1, v.len()) - 1];
    let (p50, p95, max) = (q(0.50), q(0.95), v[v.len() - 1]);
    let report = serde_json::json!({
        "id": "NFR-01",
        "what": "save_url core (validate, normalize, duplicate lookup, item + FTS commit, list DTO) on 10,000 items",
        "build": if cfg!(debug_assertions) { "debug" } else { "release" },
        "os": std::env::consts::OS,
        "samples": SAMPLES,
        "budgetMs": BUDGET_MS,
        "p50Ms": p50,
        "p95Ms": p95,
        "maxMs": max,
    });
    let out =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../test-artifacts/nfr-01");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(
        out.join("report.json"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("NFR-01: p50 {p50:.2} ms, p95 {p95:.2} ms, max {max:.2} ms");
    assert!(p95 <= BUDGET_MS, "NFR-01 p95 {p95:.1} ms > {BUDGET_MS} ms");
}
