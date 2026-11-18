//! Hybrid search: word search and search by meaning in one ranking (SPEC
//! §7.4 SEA-07, §16.3, AC-40; STATE.md F7.2; ADR-0036).
//!
//! - The word search ([`super::search`]) ranks every matching item (BM25 over
//!   metadata and transcripts). The search by meaning ranks the transcript
//!   chunks of the active index by cosine similarity to the query vector
//!   and keeps those at or above [`MIN_SIMILARITY`]; an item takes the rank
//!   of its best chunk.
//! - The two lists are fused with reciprocal rank fusion (SPEC §16.3):
//!   `1/(RRF_K + words rank) + 1/(RRF_K + meaning rank)`. RRF uses only
//!   ranks, so BM25 and cosine never have to share a scale, and an item
//!   found both ways comes first. Ties keep the word order.
//! - Every hit says why it is there ([`Fusion`]): its rank in each list.
//!   Similarities are not shown: they are not calibrated (§16.3).
//! - Filters, views and Recently removed apply to the meaning side too.
//! - Only with a searchable text and the Relevance sort; otherwise, and
//!   whenever the caller has no query vector (no model, no index), this is
//!   the plain word search, unchanged (AC-40).

use super::search::{
    self, filter_sql, hits, make_snippet, notice_of, ranked, Field, Fusion, Moment, Ranked,
    SearchPage, SearchQuery, SortField, TranscriptMatches, MAX_MOMENTS, MAX_PAGE, SEARCHED,
};
use crate::vectors::{self, Generation};
use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection};
use std::collections::HashMap;

/// RRF constant (Cormack et al. 2009): large enough that the first few
/// ranks of one list do not drown the other list.
pub const RRF_K: f64 = 60.0;

/// Chunks the search by meaning looks at.
pub const MEANING_POOL: usize = 200;

/// Cosine similarity a chunk needs to count as a match by meaning. The
/// nearest chunks of an unrelated library are still "nearest", so without
/// a floor every query would list videos about anything. Provisional: to
/// be set from the TR/EN fixture and a real library with BGE-M3 (ADR-0036,
/// `real_model_hybrid_quality_report`); not a probability.
pub const MIN_SIMILARITY: f32 = 0.50;

/// The query as a vector of the active index's model.
pub struct MeaningQuery<'a> {
    pub generation: &'a Generation,
    pub vector: &'a [f32],
}

/// One transcript chunk close in meaning.
#[derive(Debug, Clone)]
struct Near {
    item_id: String,
    transcript_id: String,
    language: String,
    provenance: String,
    partial: bool,
    covered_ms: Option<i64>,
    segment_id: String,
    start_ms: i64,
    end_ms: i64,
    text: String,
}

/// Items close in meaning, best first, each with its passages (one
/// transcript per item, the one holding its best chunk).
fn by_meaning(
    c: &Connection,
    q: &SearchQuery,
    m: &MeaningQuery<'_>,
) -> super::Result<Vec<(String, TranscriptMatches)>> {
    let hits = vectors::search(c, m.generation, m.vector, MEANING_POOL).map_err(|e| match e {
        vectors::VectorError::Sqlite(e) => super::LibraryError::from(e),
        vectors::VectorError::Invalid(m) => super::LibraryError::Invalid(m),
        vectors::VectorError::NotFound => super::LibraryError::NotFound,
    })?;
    let hits: Vec<_> = hits
        .into_iter()
        .filter(|h| h.score >= MIN_SIMILARITY)
        .collect();
    if hits.is_empty() {
        return Ok(Vec::new());
    }
    // The chunks, in searched transcripts only.
    let ph = hits.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT c.seq, c.item_id, t.id, t.language, t.provenance, t.status, t.covered_ms, \
             c.start_segment_id, c.start_ms, c.end_ms, c.text \
         FROM search_chunk c JOIN transcript t ON t.id = c.transcript_id \
         WHERE c.source_type = 'transcript' AND t.status IN {SEARCHED} AND c.seq IN ({ph})"
    );
    let mut st = c.prepare(&sql)?;
    let mut chunks: HashMap<i64, Near> = st
        .query_map(params_from_iter(hits.iter().map(|h| h.chunk_seq)), |r| {
            Ok((
                r.get::<_, i64>(0)?,
                Near {
                    item_id: r.get(1)?,
                    transcript_id: r.get(2)?,
                    language: r.get(3)?,
                    provenance: r.get(4)?,
                    partial: r.get::<_, String>(5)? == "partial",
                    covered_ms: r.get(6)?,
                    segment_id: r.get(7)?,
                    start_ms: r.get(8)?,
                    end_ms: r.get(9)?,
                    text: r.get(10)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;

    // The items the view and filters allow.
    let mut ids: Vec<String> = chunks.values().map(|n| n.item_id.clone()).collect();
    ids.sort();
    ids.dedup();
    let allowed = allowed(c, q, &ids)?;

    // Best first; per item the transcript of its best chunk.
    let mut order: Vec<String> = Vec::new();
    let mut per: HashMap<String, TranscriptMatches> = HashMap::new();
    for h in &hits {
        let Some(n) = chunks.remove(&h.chunk_seq) else {
            continue;
        };
        if !allowed.contains(&n.item_id) {
            continue;
        }
        let e = per.entry(n.item_id.clone()).or_insert_with(|| {
            order.push(n.item_id.clone());
            TranscriptMatches {
                transcript_id: n.transcript_id.clone(),
                language: n.language.clone(),
                provenance: n.provenance.clone(),
                partial: n.partial,
                covered_ms: n.covered_ms,
                moments: Vec::new(),
                total: 0,
            }
        });
        if e.transcript_id != n.transcript_id {
            continue;
        }
        e.total += 1;
        e.moments.push(Moment {
            segment_id: n.segment_id,
            start_ms: n.start_ms,
            end_ms: n.end_ms,
            snippet: make_snippet(Field::Meaning, &super::search::index_text(&n.text), &n.text),
        });
    }
    Ok(order
        .into_iter()
        .filter_map(|id| {
            let mut t = per.remove(&id)?;
            // The closest passages, shown in time order.
            t.moments.truncate(MAX_MOMENTS);
            t.moments.sort_by_key(|m| m.start_ms);
            Some((id, t))
        })
        .collect())
}

/// The ids among `ids` that the query's filters and view allow.
fn allowed(
    c: &Connection,
    q: &SearchQuery,
    ids: &[String],
) -> super::Result<std::collections::HashSet<String>> {
    if ids.is_empty() {
        return Ok(Default::default());
    }
    let mut params: Vec<Value> = ids.iter().map(|s| Value::Text(s.clone())).collect();
    let ph = ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let wh = filter_sql(&q.filters, &mut params);
    let mut st = c.prepare(&format!(
        "SELECT i.id FROM library_item i WHERE i.id IN ({ph}) AND {wh}"
    ))?;
    let out = st
        .query_map(params_from_iter(params.iter()), |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(out)
}

fn rrf(rank: Option<u32>) -> f64 {
    rank.map_or(0.0, |r| 1.0 / (RRF_K + f64::from(r)))
}

/// One page of the hybrid result; the plain word search when `meaning` is
/// `None`, the text has nothing searchable or the sort is not Relevance.
pub fn search_hybrid(
    c: &Connection,
    q: &SearchQuery,
    meaning: Option<&MeaningQuery<'_>>,
) -> super::Result<SearchPage> {
    let fts = search::build_match(&q.text, q.prefix_last);
    let (Some(m), Some(fts)) = (meaning, fts.as_deref()) else {
        return search::search(c, q);
    };
    if q.sort.field != SortField::Relevance {
        return search::search(c, q);
    }
    let (_, words) = ranked(c, q, Some(fts), None)?;
    let near = by_meaning(c, q, m)?;

    // (fused score, words rank, meaning rank, row)
    let mut rows: HashMap<String, (Option<u32>, Option<u32>, Ranked)> = HashMap::new();
    for (k, r) in words.into_iter().enumerate() {
        rows.insert(r.id.clone(), (Some(k as u32 + 1), None, r));
    }
    let mut passages: HashMap<String, TranscriptMatches> = HashMap::new();
    for (k, (id, t)) in near.into_iter().enumerate() {
        let e = rows.entry(id.clone()).or_insert_with(|| {
            (
                None,
                None,
                Ranked {
                    id: id.clone(),
                    seq: 0,
                    metadata: false,
                    transcript: false,
                },
            )
        });
        e.1 = Some(k as u32 + 1);
        passages.insert(id, t);
    }
    let mut fused: Vec<(f64, Option<u32>, Option<u32>, Ranked)> = rows
        .into_values()
        .map(|(w, m, r)| (rrf(w) + rrf(m), w, m, r))
        .collect();
    fused.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then(a.1.unwrap_or(u32::MAX).cmp(&b.1.unwrap_or(u32::MAX)))
            .then(a.2.unwrap_or(u32::MAX).cmp(&b.2.unwrap_or(u32::MAX)))
            .then(a.3.id.cmp(&b.3.id))
    });
    let total = fused.len() as u64;
    let limit = q.limit.clamp(1, MAX_PAGE) as usize;
    let page: Vec<_> = fused
        .into_iter()
        .skip(q.offset as usize)
        .take(limit)
        .collect();
    let ranks: Vec<Fusion> = page
        .iter()
        .map(|(_, w, m, _)| Fusion {
            words: *w,
            meaning: *m,
        })
        .collect();
    let mut out = hits(c, Some(fts), page.into_iter().map(|p| p.3).collect())?;
    for (h, f) in out.iter_mut().zip(ranks) {
        h.meaning = passages.remove(&h.item.id);
        if h.meaning.is_some() {
            h.matched.push(Field::Meaning);
        }
        h.fusion = Some(f);
    }
    Ok(SearchPage {
        total,
        hits: out,
        notice: notice_of(q, true),
    })
}
