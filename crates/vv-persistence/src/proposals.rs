//! AI suggestions for an item (SPEC §11 `AIProposal`, AI-B01…B04; STATE.md
//! F7.3; ADR-0037).
//!
//! - A suggestion is stored, never applied: the item's tags change only
//!   when the user accepts one ([`link_tag`] marks that relation
//!   `ai_accepted` with the proposal's id, AI-B02).
//! - Accepted and rejected rows stay. A value the user rejected (or
//!   accepted, or already has as a tag) is not suggested for that item
//!   again, whatever model asks; [`save_run`] drops it.
//! - [`save_run`] replaces the item's pending suggestions and records the
//!   run's source fingerprint, so the same text with the same model and
//!   API is not asked again (AI-B04), also when nothing new came back.

use crate::library::new_id;
use crate::library::organize::{clean_name, normalize_tag, MAX_TAG_LEN};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

#[derive(Debug)]
pub enum ProposalError {
    Sqlite(rusqlite::Error),
    NotFound,
    /// The proposal was already accepted or rejected.
    Decided,
}

impl std::fmt::Display for ProposalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProposalError::Sqlite(e) => write!(f, "sqlite: {e}"),
            ProposalError::NotFound => write!(f, "no such proposal"),
            ProposalError::Decided => write!(f, "proposal already decided"),
        }
    }
}

impl std::error::Error for ProposalError {}

impl From<rusqlite::Error> for ProposalError {
    fn from(e: rusqlite::Error) -> Self {
        ProposalError::Sqlite(e)
    }
}

pub type Result<T> = std::result::Result<T, ProposalError>;

/// Provider name stored with every proposal.
pub const PROVIDER: &str = "openai_compatible";
/// Most suggestions kept from one run.
pub const MAX_PER_RUN: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pending,
    Accepted,
    Rejected,
}

impl Status {
    fn parse(s: &str) -> Status {
        match s {
            "accepted" => Status::Accepted,
            "rejected" => Status::Rejected,
            _ => Status::Pending,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub id: String,
    pub item_id: String,
    /// `tag` (topics and collections are not suggested yet).
    #[serde(rename = "type")]
    pub kind: String,
    pub value: String,
    pub status: Status,
    pub model_id: String,
    /// The API address.
    pub model_version: String,
    pub created_at: i64,
    pub decided_at: Option<i64>,
}

const COLS: &str =
    "id, item_id, type, value, status, model_id, model_version, created_at, decided_at";

fn row(r: &Row<'_>) -> rusqlite::Result<Proposal> {
    Ok(Proposal {
        id: r.get(0)?,
        item_id: r.get(1)?,
        kind: r.get(2)?,
        value: r.get(3)?,
        status: Status::parse(&r.get::<_, String>(4)?),
        model_id: r.get(5)?,
        model_version: r.get(6)?,
        created_at: r.get(7)?,
        decided_at: r.get(8)?,
    })
}

/// What a suggestion is made from: the item's own words (AI-B01).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Source {
    pub title: String,
    pub description: String,
    /// Passages of the newest usable transcript, spread over the whole
    /// video when it is longer than the budget; empty without one.
    pub transcript: String,
}

impl Source {
    /// Nothing to suggest from.
    pub fn is_empty(&self) -> bool {
        self.title.trim().is_empty()
            && self.description.trim().is_empty()
            && self.transcript.trim().is_empty()
    }
}

fn cut(s: &str, max_bytes: usize) -> String {
    let s = s.trim();
    let mut end = s.len().min(max_bytes);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s.get(..end).unwrap_or_default().to_owned()
}

/// Passages whose total stays within `budget` bytes, evenly spread.
pub fn spread(passages: &[String], budget: usize) -> String {
    let total: usize = passages.iter().map(String::len).sum();
    if total <= budget {
        return passages.join(" ");
    }
    let mut out: Vec<&str> = Vec::new();
    let mut used = 0;
    // Take every n-th passage so the picks cover the start, middle and end.
    let step = (total as f64 / budget as f64).ceil().max(1.0) as usize;
    for p in passages.iter().step_by(step) {
        if used + p.len() + 3 > budget {
            break;
        }
        used += p.len() + 3;
        out.push(p);
    }
    out.join(" … ")
}

/// The source of a live item, `None` when it is missing or removed.
pub fn source(
    c: &Connection,
    item_id: &str,
    title_max: usize,
    description_max: usize,
    transcript_max: usize,
) -> Result<Option<Source>> {
    let head: Option<(Option<String>, Option<String>)> = c
        .query_row(
            "SELECT title, description FROM library_item WHERE id = ?1 AND removed_at IS NULL",
            [item_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((title, description)) = head else {
        return Ok(None);
    };
    let mut s = c.prepare_cached(
        "SELECT text FROM search_chunk WHERE source_type = 'transcript' AND transcript_id = ( \
            SELECT id FROM transcript WHERE item_id = ?1 AND status IN ('ready', 'partial') \
            ORDER BY updated_at DESC, id LIMIT 1) \
         ORDER BY start_ms, seq",
    )?;
    let passages: Vec<String> = s
        .query_map([item_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(Some(Source {
        title: cut(&title.unwrap_or_default(), title_max),
        description: cut(&description.unwrap_or_default(), description_max),
        transcript: spread(&passages, transcript_max),
    }))
}

/// Fingerprint of one run's input (AI-B04): the prompt version, the model,
/// the API address and the text.
pub fn fingerprint(prompt_version: &str, model: &str, url: &str, s: &Source) -> String {
    let mut h = Sha256::new();
    for part in [
        prompt_version,
        model,
        url,
        &s.title,
        &s.description,
        &s.transcript,
    ] {
        h.update((part.len() as u64).to_le_bytes());
        h.update(part.as_bytes());
    }
    h.finalize()
        .iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The fingerprint of the item's last run, if any.
pub fn last_run(c: &Connection, item_id: &str) -> Result<Option<String>> {
    Ok(c.query_row(
        "SELECT source_fingerprint FROM ai_proposal_run WHERE item_id = ?1",
        [item_id],
        |r| r.get(0),
    )
    .optional()?)
}

/// Normalized values that must not be suggested for the item: its tags and
/// every accepted or rejected proposal.
pub fn settled_values(c: &Connection, item_id: &str) -> Result<HashSet<String>> {
    let mut out = HashSet::new();
    let mut s = c.prepare_cached(
        "SELECT t.normalized_name FROM library_item_tag lt JOIN tag t ON t.id = lt.tag_id \
         WHERE lt.item_id = ?1 \
         UNION SELECT normalized_value FROM ai_proposal WHERE item_id = ?1 AND status <> 'pending'",
    )?;
    for v in s.query_map([item_id], |r| r.get::<_, String>(0))? {
        out.insert(v?);
    }
    Ok(out)
}

/// A suggested value as a tag name: trimmed, `#` dropped, spaces collapsed;
/// `None` when empty or too long.
pub fn clean_value(v: &str) -> Option<String> {
    let v = clean_name(v.trim().trim_start_matches('#'));
    let v = v.trim_matches(|c: char| c == '"' || c == '\'' || c == '.' || c == ',');
    if v.is_empty() || v.chars().count() > MAX_TAG_LEN {
        return None;
    }
    Some(v.to_owned())
}

/// Stores one run for an item: its pending suggestions become `values`
/// (cleaned, without settled ones and duplicates, at most
/// [`MAX_PER_RUN`]), and the run's fingerprint is recorded. Returns the
/// item's pending suggestions.
pub fn save_run(
    c: &Connection,
    item_id: &str,
    model_id: &str,
    url: &str,
    fingerprint: &str,
    values: &[String],
    now: i64,
) -> Result<Vec<Proposal>> {
    c.execute(
        "DELETE FROM ai_proposal WHERE item_id = ?1 AND status = 'pending'",
        [item_id],
    )?;
    let mut seen = settled_values(c, item_id)?;
    let mut ins = c.prepare_cached(
        "INSERT INTO ai_proposal (id, item_id, type, value, normalized_value, status, provider, \
         model_id, model_version, source_fingerprint, created_at) \
         VALUES (?1, ?2, 'tag', ?3, ?4, 'pending', ?5, ?6, ?7, ?8, ?9)",
    )?;
    let mut n = 0;
    for v in values {
        if n >= MAX_PER_RUN {
            break;
        }
        let Some(v) = clean_value(v) else { continue };
        let key = normalize_tag(&v);
        if key.is_empty() || !seen.insert(key.clone()) {
            continue;
        }
        ins.execute(params![
            new_id(),
            item_id,
            v,
            key,
            PROVIDER,
            model_id,
            url,
            fingerprint,
            now
        ])?;
        n += 1;
    }
    c.execute(
        "INSERT INTO ai_proposal_run (item_id, source_fingerprint, model_id, model_version, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT (item_id) DO UPDATE SET source_fingerprint = excluded.source_fingerprint, \
         model_id = excluded.model_id, model_version = excluded.model_version, \
         created_at = excluded.created_at",
        params![item_id, fingerprint, model_id, url, now],
    )?;
    pending(c, item_id)
}

/// The item's pending suggestions, in the order they were made.
pub fn pending(c: &Connection, item_id: &str) -> Result<Vec<Proposal>> {
    let mut s = c.prepare_cached(&format!(
        "SELECT {COLS} FROM ai_proposal WHERE item_id = ?1 AND status = 'pending' \
         ORDER BY created_at, rowid"
    ))?;
    let v = s
        .query_map([item_id], row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(v)
}

pub fn get(c: &Connection, id: &str) -> Result<Option<Proposal>> {
    Ok(c.query_row(
        &format!("SELECT {COLS} FROM ai_proposal WHERE id = ?1"),
        [id],
        row,
    )
    .optional()?)
}

/// Marks a pending proposal accepted or rejected.
pub fn decide(c: &Connection, id: &str, accept: bool, now: i64) -> Result<Proposal> {
    let p = get(c, id)?.ok_or(ProposalError::NotFound)?;
    if p.status != Status::Pending {
        return Err(ProposalError::Decided);
    }
    c.execute(
        "UPDATE ai_proposal SET status = ?2, decided_at = ?3 WHERE id = ?1",
        params![id, if accept { "accepted" } else { "rejected" }, now],
    )?;
    get(c, id)?.ok_or(ProposalError::NotFound)
}

/// Marks the item's relation to the tag as coming from an accepted
/// proposal (AI-B02). Only a relation the user did not have before is
/// changed.
pub fn link_tag(
    c: &Connection,
    item_id: &str,
    tag_id: &str,
    proposal_id: &str,
    since: i64,
) -> Result<bool> {
    let n = c.execute(
        "UPDATE library_item_tag SET origin = 'ai_accepted', proposal_id = ?3 \
         WHERE item_id = ?1 AND tag_id = ?2 AND origin = 'user' AND created_at >= ?4",
        params![item_id, tag_id, proposal_id, since],
    )?;
    Ok(n == 1)
}

/// Pending suggestions in the whole library.
pub fn pending_count(c: &Connection) -> Result<i64> {
    Ok(c.query_row(
        "SELECT count(*) FROM ai_proposal p JOIN library_item i ON i.id = p.item_id \
         WHERE p.status = 'pending' AND i.removed_at IS NULL",
        [],
        |r| r.get(0),
    )?)
}

/// Live items, newest first: what "Suggest for all" walks.
pub fn live_items(c: &Connection) -> Result<Vec<String>> {
    let mut s = c.prepare_cached(
        "SELECT id FROM library_item WHERE removed_at IS NULL ORDER BY added_at DESC, id",
    )?;
    let v = s
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(v)
}

/// The most used tag names (the model is asked to prefer them).
pub fn top_tags(c: &Connection, limit: usize) -> Result<Vec<String>> {
    let mut s = c.prepare_cached(
        "SELECT t.name FROM tag t LEFT JOIN library_item_tag lt ON lt.tag_id = t.id \
         GROUP BY t.id ORDER BY count(lt.item_id) DESC, t.normalized_name LIMIT ?1",
    )?;
    let v = s
        .query_map([limit as i64], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spread_keeps_short_text_and_samples_long_text() {
        let short = vec!["a".to_owned(), "b".to_owned()];
        assert_eq!(spread(&short, 100), "a b");
        let long: Vec<String> = (0..100)
            .map(|i| format!("{i:03}{}", "x".repeat(96)))
            .collect();
        let s = spread(&long, 1000);
        assert!(s.len() <= 1000, "{}", s.len());
        assert!(s.starts_with("000"));
        // Not just the start: a later passage is in.
        assert!(s.contains("030") || s.contains("050"), "{s}");
    }

    #[test]
    fn values_are_cleaned() {
        assert_eq!(
            clean_value("  #Savaş  uçağı. ").as_deref(),
            Some("Savaş uçağı")
        );
        assert_eq!(clean_value("\"harrier\"").as_deref(), Some("harrier"));
        assert_eq!(clean_value("  "), None);
        assert_eq!(clean_value(&"x".repeat(101)), None);
    }

    #[test]
    fn fingerprint_changes_with_text_model_and_address() {
        let s = Source {
            title: "t".into(),
            description: "d".into(),
            transcript: "x".into(),
        };
        let f = fingerprint("1", "m", "u", &s);
        assert_eq!(f.len(), 32);
        assert_eq!(f, fingerprint("1", "m", "u", &s));
        assert_ne!(f, fingerprint("2", "m", "u", &s));
        assert_ne!(f, fingerprint("1", "n", "u", &s));
        assert_ne!(f, fingerprint("1", "m", "v", &s));
        let mut t = s.clone();
        t.transcript = "y".into();
        assert_ne!(f, fingerprint("1", "m", "u", &t));
    }
}
