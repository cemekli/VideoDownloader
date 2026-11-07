//! Benchmark runner on the generated data (BM-01…03; ADR-0009).
//!
//! **Prototype tables.** The library schema arrives in STATE.md F1.1, so
//! these run on `bench_*` tables inside a database opened exactly like the
//! app's (`vv_persistence::Database`: WAL, `synchronous=FULL`). The numbers
//! are a baseline for the method and the machine, not a measurement of the
//! app; F4.5 repeats them on the real schema.

use crate::rng::Rng;
use crate::{words, Dataset};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stat {
    pub id: &'static str,
    pub what: &'static str,
    pub nfr: &'static str,
    pub budget_ms: f64,
    pub samples: usize,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
    /// Rows the queries returned in total, so an empty result set cannot
    /// pass for a fast one.
    pub rows: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub sqlite_version: String,
    pub load_items_ms: f64,
    pub load_segments_ms: f64,
    pub db_bytes: u64,
    pub stats: Vec<Stat>,
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn stat(
    id: &'static str,
    what: &'static str,
    nfr: &'static str,
    budget_ms: f64,
    mut v: Vec<f64>,
    rows: u64,
) -> Stat {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| v[((v.len() as f64 * p).ceil() as usize).clamp(1, v.len()) - 1];
    let round = |x: f64| (x * 1000.0).round() / 1000.0;
    Stat {
        id,
        what,
        nfr,
        budget_ms,
        samples: v.len(),
        p50_ms: round(q(0.50)),
        p95_ms: round(q(0.95)),
        max_ms: round(*v.last().unwrap()),
        rows,
    }
}

const SCHEMA: &str = "
CREATE TABLE bench_items (
  id INTEGER PRIMARY KEY, kind TEXT NOT NULL, platform TEXT NOT NULL, source_url TEXT,
  title TEXT NOT NULL, description TEXT NOT NULL, note TEXT, duration_ms INTEGER,
  language TEXT NOT NULL, created_at INTEGER NOT NULL, favorite INTEGER NOT NULL, archived INTEGER NOT NULL
);
CREATE VIRTUAL TABLE bench_items_fts USING fts5(title, description, note, tags, collections, tokenize = 'unicode61 remove_diacritics 2');
CREATE TABLE bench_segments (
  item_id INTEGER NOT NULL, idx INTEGER NOT NULL, start_ms INTEGER NOT NULL, end_ms INTEGER NOT NULL,
  language TEXT NOT NULL, PRIMARY KEY (item_id, idx)
);
CREATE VIRTUAL TABLE bench_segments_fts USING fts5(text, tokenize = 'unicode61 remove_diacritics 2');
";

fn insert_item(c: &Connection, it: &crate::Item) -> rusqlite::Result<()> {
    c.execute(
        "INSERT INTO bench_items VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            it.id,
            it.kind,
            it.platform,
            it.source_url,
            it.title,
            it.description,
            it.note,
            it.duration_ms.map(|d| d as i64),
            it.language,
            it.created_at as i64,
            it.favorite,
            it.archived
        ],
    )?;
    c.execute(
        "INSERT INTO bench_items_fts (rowid, title, description, note, tags, collections) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![it.id, it.title, it.description, it.note.as_deref().unwrap_or(""), it.tags.join(" "), it.collections.join(" ")],
    )?;
    Ok(())
}

/// Deterministic query mix: one word, two words, a prefix, and words with
/// Turkish letters. Each token is quoted so FTS5 syntax cannot break.
pub fn queries(seed: u64, n: usize) -> Vec<String> {
    let mut r = Rng::new(seed ^ 0x7175_6572_7900_0003);
    let quote = |w: &str| format!("\"{}\"", w.replace('"', "\"\""));
    (0..n)
        .map(|i| {
            let pool = if r.chance(55) { words::TR } else { words::EN };
            match i % 4 {
                0 => quote(r.pick(pool)),
                1 => format!("{} {}", quote(r.pick(pool)), quote(r.pick(pool))),
                2 => {
                    let w: String = r.pick(pool).chars().take(3).collect();
                    format!("{}*", quote(&w))
                }
                _ => quote(r.pick(&[
                    "ışık",
                    "güneş",
                    "İstanbul",
                    "çay",
                    "şehir",
                    "öğle",
                    "üzüm",
                    "ılık",
                ])),
            }
        })
        .collect()
}

pub fn run(data: &Dataset, dir: &Path) -> Result<Report, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("bench.sqlite");
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
    }
    let mut db = vv_persistence::Database::open(&path, &vv_persistence::OpenOptions::default())?;
    let c = db.writer();
    c.execute_batch(SCHEMA)?;
    let sqlite_version: String = c.query_row("SELECT sqlite_version()", [], |r| r.get(0))?;

    // Load: one transaction per table, as an import would.
    let t = Instant::now();
    let tx = c.transaction()?;
    for it in &data.items {
        insert_item(&tx, it)?;
    }
    tx.commit()?;
    let load_items_ms = ms(t);
    let t = Instant::now();
    let tx = c.transaction()?;
    {
        let mut seg = tx.prepare("INSERT INTO bench_segments VALUES (?1, ?2, ?3, ?4, ?5)")?;
        let mut fts = tx.prepare("INSERT INTO bench_segments_fts (rowid, text) VALUES (?1, ?2)")?;
        for (i, s) in data.segments.iter().enumerate() {
            seg.execute(params![
                s.item_id,
                s.index,
                s.start_ms as i64,
                s.end_ms as i64,
                s.language
            ])?;
            fts.execute(params![i as i64 + 1, s.text])?;
        }
    }
    tx.commit()?;
    let load_segments_ms = ms(t);
    c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); ANALYZE;")?;

    let mut stats = Vec::new();

    // BM-01: save one bookmark (row + search document) in its own durable
    // transaction on the full library.
    let mut r = Rng::new(data.params.seed ^ 0x6e65_7700_0000_0004);
    let mut v = Vec::new();
    let base = data.items.len() as u32;
    for k in 1..=300u32 {
        let mut it = data.items[r.below(data.items.len() as u64) as usize].clone();
        it.id = base + k;
        let t = Instant::now();
        let tx = c.transaction()?;
        insert_item(&tx, &it)?;
        tx.commit()?;
        v.push(ms(t));
    }
    stats.push(stat(
        "BM-01",
        "bookmark kaydı: satır + FTS belgesi, tek işlem, synchronous=FULL",
        "NFR-01",
        500.0,
        v,
        300,
    ));

    // BM-02 / BM-03: warm index; every query once to warm, then measured.
    let item_q = "SELECT rowid, snippet(bench_items_fts, -1, '[', ']', '…', 12) FROM bench_items_fts WHERE bench_items_fts MATCH ?1 ORDER BY bm25(bench_items_fts) LIMIT 50";
    let seg_q = "SELECT s.item_id, s.start_ms, snippet(bench_segments_fts, 0, '[', ']', '…', 12) FROM bench_segments_fts JOIN bench_segments s ON s.rowid = bench_segments_fts.rowid WHERE bench_segments_fts MATCH ?1 ORDER BY bm25(bench_segments_fts) LIMIT 50";
    let qs = queries(data.params.seed, 200);
    for (id, what, nfr, budget, sql) in [
        (
            "BM-02",
            "lexical arama, 10.000 öğe, sıcak indeks, ilk 50 sonuç + snippet",
            "NFR-03",
            200.0,
            item_q,
        ),
        (
            "BM-03",
            "döküm araması, 100.000 segment, sıcak indeks, ilk 50 sonuç + snippet",
            "NFR-04",
            500.0,
            seg_q,
        ),
    ] {
        let reader = db.reader()?;
        let mut st = reader.prepare(sql)?;
        for q in &qs {
            st.query_map([q], |_| Ok(()))?.for_each(drop);
        }
        let (mut v, mut rows) = (Vec::new(), 0u64);
        for q in &qs {
            let t = Instant::now();
            let n = st.query_map([q], |_| Ok(()))?.count() as u64;
            v.push(ms(t));
            rows += n;
        }
        stats.push(stat(id, what, nfr, budget, v, rows));
    }
    drop(db);
    let db_bytes = std::fs::metadata(&path)?.len();
    Ok(Report {
        sqlite_version,
        load_items_ms: (load_items_ms * 10.0).round() / 10.0,
        load_segments_ms: (load_segments_ms * 10.0).round() / 10.0,
        db_bytes,
        stats,
    })
}
