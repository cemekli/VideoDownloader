//! Text read on screen (SPEC AI-C03, AI-C04, SEA-08, AC-41; STATE.md F7.4;
//! ADR-0039; migration 0017).
//!
//! An item's frames are read once per media file and reading rules
//! ([`Run`]). What was read is stored as `search_chunk` rows with
//! `source_type = 'ocr'`, one per run of frames that showed the same text,
//! with the time it appeared and the time it was gone, and indexed in
//! `transcript_search` so words search finds it. The OCR text is never
//! mixed with speech: transcript queries join `transcript`, which OCR rows
//! have none of, and the search labels it "on screen".

use crate::library::new_id;
use crate::library::search::index_text;
use rusqlite::{params, Connection, OptionalExtension, Row};

pub type Result<T> = std::result::Result<T, rusqlite::Error>;

/// Longest text kept for one chunk (a frame full of small print).
pub const TEXT_MAX: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Ready,
    Failed,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Ready => "ready",
            RunStatus::Failed => "failed",
        }
    }
}

/// How an item's frames were read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub item_id: String,
    pub asset_id: Option<String>,
    pub provider: String,
    pub language: String,
    pub config_version: i64,
    pub source_fingerprint: String,
    pub status: RunStatus,
    pub error_code: Option<String>,
    pub frames: i64,
    pub chunks: i64,
    pub created_at: i64,
}

/// What a run read from, and with what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source<'a> {
    pub item_id: &'a str,
    pub asset_id: Option<&'a str>,
    pub provider: &'a str,
    pub language: &'a str,
    pub config_version: i64,
    pub fingerprint: &'a str,
}

/// Text on screen from `start_ms` until `end_ms`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

/// A stored line with its chunk id (what a search moment names).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredLine {
    pub id: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

fn from_row(r: &Row) -> rusqlite::Result<Run> {
    let status: String = r.get(6)?;
    Ok(Run {
        item_id: r.get(0)?,
        asset_id: r.get(1)?,
        provider: r.get(2)?,
        language: r.get(3)?,
        config_version: r.get(4)?,
        source_fingerprint: r.get(5)?,
        status: if status == "ready" {
            RunStatus::Ready
        } else {
            RunStatus::Failed
        },
        error_code: r.get(7)?,
        frames: r.get(8)?,
        chunks: r.get(9)?,
        created_at: r.get(10)?,
    })
}

const COLS: &str = "item_id, asset_id, provider, language, config_version, source_fingerprint, \
                    status, error_code, frames, chunks, created_at";

pub fn get(c: &Connection, item_id: &str) -> Result<Option<Run>> {
    c.query_row(
        &format!("SELECT {COLS} FROM screen_text_run WHERE item_id = ?1"),
        [item_id],
        from_row,
    )
    .optional()
}

/// Cuts a text to [`TEXT_MAX`] characters.
fn cut(text: &str) -> String {
    text.chars().take(TEXT_MAX).collect()
}

/// Saves a finished run with its lines, replacing the item's earlier run
/// and its text, in one transaction.
pub fn save(
    c: &mut Connection,
    src: &Source<'_>,
    frames: i64,
    lines: &[Line],
    now: i64,
) -> Result<Run> {
    let tx = c.transaction()?;
    clear(&tx, src.item_id)?;
    let lines: Vec<&Line> = lines
        .iter()
        .filter(|l| !l.text.trim().is_empty() && l.start_ms >= 0 && l.end_ms > l.start_ms)
        .collect();
    tx.execute(
        &format!("INSERT INTO screen_text_run ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'ready', NULL, ?7, ?8, ?9)"),
        params![
            src.item_id,
            src.asset_id,
            src.provider,
            src.language,
            src.config_version,
            src.fingerprint,
            frames,
            lines.len() as i64,
            now
        ],
    )?;
    {
        let mut ins = tx.prepare(
            "INSERT INTO search_chunk (id, item_id, start_ms, end_ms, text, source_type, \
                 producer_provider, producer_model_id, producer_model_version) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'ocr', ?6, ?7, ?8)",
        )?;
        let mut fts = tx.prepare("INSERT INTO transcript_search (rowid, text) VALUES (?1, ?2)")?;
        for l in lines {
            let text = cut(l.text.trim());
            ins.execute(params![
                new_id(),
                src.item_id,
                l.start_ms,
                l.end_ms,
                text,
                src.provider,
                src.language,
                src.config_version.to_string()
            ])?;
            fts.execute(params![tx.last_insert_rowid(), index_text(&text)])?;
        }
    }
    tx.commit()?;
    get(c, src.item_id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

/// Records that the item's file could not be read, so a run for all
/// videos does not try it again until the file or the rules change. Text
/// read earlier is removed with it: it was read from another file.
pub fn save_failed(c: &mut Connection, src: &Source<'_>, error_code: &str, now: i64) -> Result<()> {
    let tx = c.transaction()?;
    clear(&tx, src.item_id)?;
    tx.execute(
        &format!("INSERT INTO screen_text_run ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'failed', ?7, 0, 0, ?8)"),
        params![
            src.item_id,
            src.asset_id,
            src.provider,
            src.language,
            src.config_version,
            src.fingerprint,
            error_code,
            now
        ],
    )?;
    tx.commit()
}

fn clear(c: &Connection, item_id: &str) -> Result<()> {
    // The `search_chunk_deleted` trigger removes the index rows.
    c.execute(
        "DELETE FROM search_chunk WHERE item_id = ?1 AND source_type = 'ocr'",
        [item_id],
    )?;
    c.execute("DELETE FROM screen_text_run WHERE item_id = ?1", [item_id])?;
    Ok(())
}

/// Forgets what was read for an item.
pub fn delete(c: &mut Connection, item_id: &str) -> Result<()> {
    let tx = c.transaction()?;
    clear(&tx, item_id)?;
    tx.commit()
}

/// The item's lines in time order.
pub fn lines(c: &Connection, item_id: &str) -> Result<Vec<StoredLine>> {
    let mut st = c.prepare(
        "SELECT id, start_ms, end_ms, text FROM search_chunk \
         WHERE item_id = ?1 AND source_type = 'ocr' ORDER BY start_ms, seq",
    )?;
    let rows = st
        .query_map([item_id], |r| {
            Ok(StoredLine {
                id: r.get(0)?,
                start_ms: r.get(1)?,
                end_ms: r.get(2)?,
                text: r.get(3)?,
            })
        })?
        .collect();
    rows
}

/// Where an on-screen line is (item, start, end), to open a search moment;
/// `None` when it was read again since.
pub fn place(c: &Connection, chunk_id: &str) -> Result<Option<(String, i64, i64)>> {
    c.query_row(
        "SELECT item_id, start_ms, end_ms FROM search_chunk WHERE id = ?1 AND source_type = 'ocr'",
        [chunk_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .optional()
}

/// Live items read (ready or failed) and lines stored.
pub fn counts(c: &Connection) -> Result<(i64, i64, i64)> {
    c.query_row(
        "SELECT \
           (SELECT count(*) FROM screen_text_run r JOIN library_item i ON i.id = r.item_id \
              WHERE i.removed_at IS NULL AND r.status = 'ready'), \
           (SELECT count(*) FROM screen_text_run r JOIN library_item i ON i.id = r.item_id \
              WHERE i.removed_at IS NULL AND r.status = 'failed'), \
           (SELECT count(*) FROM search_chunk c JOIN library_item i ON i.id = c.item_id \
              WHERE i.removed_at IS NULL AND c.source_type = 'ocr')",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
}
