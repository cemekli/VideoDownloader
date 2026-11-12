//! Lexical search over `search_document` (SPEC §7.4 SEA-01…05, §16.1;
//! STATE.md F1.6, ADR-0012).
//!
//! - User text never reaches SQL or FTS syntax: every term is quoted as an
//!   FTS5 string, operators (`AND`, `NEAR`, `*`, `:`, `^`, quotes) are plain
//!   words, and an input without any letter or digit searches nothing
//!   instead of failing (SEA-04, AC-31).
//! - Turkish: the tokenizer (`unicode61 remove_diacritics 2`) already folds
//!   case, `İ`, NFD forms and `ç ğ ö ş ü`; the one letter it keeps apart is
//!   dotless `ı`, so both the index and the query map `ı` to `i`. That is a
//!   one-to-one character map, so snippets are rebuilt from the original
//!   text and stay real substrings (§16.1). `tests/fixtures/search-tr.json`
//!   records what matches and the accepted limits.
//! - Ranking: BM25 with title > tags = notes > description = creator, then
//!   newest first and id (stable pages).
//! - Transcripts (F6.4, SEA-06, §16.2, ADR-0030): an item also matches when
//!   the query is found in one of its `ready` or `partial` transcripts. Its
//!   transcript BM25 is added to the metadata BM25 with
//!   [`TRANSCRIPT_WEIGHT`]. The moments are grouped under the item, taken
//!   from one transcript, and each opens at the real start of the segment
//!   that holds the first matched word.

use super::repo::{self, LibraryItem};
use super::{ItemKind, MetadataStatus, SourcePlatform};
use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection};

/// Highlight markers. Removed from all indexed text, so they cannot occur
/// in a document.
const HL_START: char = '\u{1}';
const HL_END: char = '\u{2}';

/// BM25 weights in column order: title, description, creator, notes, tags.
pub const WEIGHTS: [f64; 5] = [10.0, 2.0, 2.0, 5.0, 5.0];

/// Weight of the best transcript chunk's BM25 next to the metadata BM25.
/// Below the description's: words said in a video count, but a title or a
/// note that names the topic ranks first.
pub const TRANSCRIPT_WEIGHT: f64 = 1.0;

/// Moments listed under one item; [`TranscriptMatches::total`] counts all.
pub const MAX_MOMENTS: usize = 5;

/// Transcript states that are searched. `partial` ones carry the range
/// they cover (§16.2); `stale`, `failed` and the rest are not searched.
const SEARCHED: &str = "('ready', 'partial')";

/// Text as stored in the index: markers removed, dotless `ı` → `i`.
/// Character for character the same length as [`display`] of the input.
pub fn index_text(s: &str) -> String {
    s.chars()
        .filter(|&c| c != HL_START && c != HL_END)
        .map(|c| if c == 'ı' { 'i' } else { c })
        .collect()
}

fn display(s: &str) -> String {
    s.chars()
        .filter(|&c| c != HL_START && c != HL_END)
        .collect()
}

/// The FTS5 query for user text, or `None` when nothing is searchable.
/// `prefix_last`: the last word also matches longer words (search as you
/// type) unless the input ends with a space or a closing quote.
pub fn build_match(text: &str, prefix_last: bool) -> Option<String> {
    let mut terms: Vec<(String, bool)> = Vec::new(); // (text, quoted)
    let mut cur = String::new();
    let mut in_quote = false;
    for ch in text.chars() {
        match ch {
            '"' => {
                if in_quote || !cur.is_empty() {
                    terms.push((std::mem::take(&mut cur), in_quote));
                }
                in_quote = !in_quote;
            }
            c if c.is_whitespace() && !in_quote => {
                if !cur.is_empty() {
                    terms.push((std::mem::take(&mut cur), false));
                }
            }
            c => cur.push(c),
        }
    }
    let open_tail = !cur.is_empty() && !in_quote;
    if !cur.is_empty() {
        terms.push((cur, in_quote));
    }
    let terms: Vec<(String, bool)> = terms
        .into_iter()
        .filter(|(t, _)| t.chars().any(char::is_alphanumeric))
        .collect();
    if terms.is_empty() {
        return None;
    }
    let last = terms.len() - 1;
    let q = terms
        .iter()
        .enumerate()
        .map(|(i, (t, quoted))| {
            let s = format!("\"{}\"", index_text(t).replace('"', "\"\""));
            if prefix_last && i == last && open_tail && !quoted {
                s + "*"
            } else {
                s
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    Some(q)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArchiveFilter {
    /// Normal views (§7.3: archive hides from the normal list).
    #[default]
    Exclude,
    Include,
    Only,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filters {
    pub platforms: Vec<SourcePlatform>,
    pub kinds: Vec<ItemKind>,
    pub metadata_status: Vec<MetadataStatus>,
    pub favorite_only: bool,
    pub archived: ArchiveFilter,
    /// Only items added at or after this UTC ms ("Recently Added", §9.2).
    pub added_since: Option<i64>,
    /// Items carrying any of these tags.
    pub tag_ids: Vec<String>,
    /// Items in any of these manual collections (direct membership).
    pub collection_ids: Vec<String>,
    /// Inbox (§7.3): items in no manual collection.
    pub inbox: bool,
    /// Recently removed instead of the live library (§7.3).
    pub removed: bool,
    pub min_duration_ms: Option<i64>,
    pub max_duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    /// Only with a query; without one it falls back to `AddedAt`.
    Relevance,
    AddedAt,
    PublishedAt,
    Title,
    Duration,
    UpdatedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sort {
    pub field: SortField,
    pub descending: bool,
}

impl Default for Sort {
    /// §16.1: `added_at`, newest first.
    fn default() -> Self {
        Sort {
            field: SortField::AddedAt,
            descending: true,
        }
    }
}

pub const MAX_PAGE: u32 = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub text: String,
    pub prefix_last: bool,
    pub filters: Filters,
    pub sort: Sort,
    pub limit: u32,
    pub offset: u32,
}

impl Default for SearchQuery {
    fn default() -> Self {
        SearchQuery {
            text: String::new(),
            prefix_last: true,
            filters: Filters::default(),
            sort: Sort::default(),
            limit: 50,
            offset: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Title,
    Description,
    Creator,
    Notes,
    Tags,
    /// Words said in the video (F6.4); never in [`FIELDS`].
    Transcript,
}

/// Column order of `search_document`, and the order a snippet prefers.
pub const FIELDS: [Field; 5] = [
    Field::Title,
    Field::Description,
    Field::Creator,
    Field::Notes,
    Field::Tags,
];
const SNIPPET_PREFERENCE: [Field; 5] = [
    Field::Title,
    Field::Notes,
    Field::Tags,
    Field::Description,
    Field::Creator,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetPart {
    pub text: String,
    pub highlight: bool,
}

/// A real substring of one field, split into plain and matched parts. The
/// UI renders parts as text, never as HTML (SEA-03).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snippet {
    pub field: Field,
    pub parts: Vec<SnippetPart>,
    pub cut_start: bool,
    pub cut_end: bool,
}

impl Snippet {
    pub fn text(&self) -> String {
        self.parts.iter().map(|p| p.text.as_str()).collect()
    }
}

/// One place in a transcript where the query was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moment {
    /// The segment holding the first matched word; opening the moment
    /// seeks to its start (PLAY-06).
    pub segment_id: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub snippet: Snippet,
}

/// The query's matches in one transcript of an item (SEA-06).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptMatches {
    pub transcript_id: String,
    pub language: String,
    /// `source_captions`, `generated_local` or `imported`.
    pub provenance: String,
    /// A partial transcript: only up to `covered_ms` was processed (§16.2).
    pub partial: bool,
    pub covered_ms: Option<i64>,
    /// The first [`MAX_MOMENTS`] matches in time order.
    pub moments: Vec<Moment>,
    /// All matching chunks of this transcript.
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub item: LibraryItem,
    /// Fields the query matched, in column order (match source, SEA-03);
    /// [`Field::Transcript`] last.
    pub matched: Vec<Field>,
    pub snippet: Option<Snippet>,
    pub transcript: Option<TranscriptMatches>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryNotice {
    /// Text was given but had no letter or digit; the filtered library is
    /// shown instead (SEA-04, AC-31).
    NoSearchableTerms,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPage {
    pub total: u64,
    pub hits: Vec<Hit>,
    pub notice: Option<QueryNotice>,
}

fn filter_sql(f: &Filters, params: &mut Vec<Value>) -> String {
    let mut w = vec![if f.removed {
        "i.removed_at IS NOT NULL".to_owned()
    } else {
        "i.removed_at IS NULL".to_owned()
    }];
    let mut ids = |sql: &str, vals: &[String], params: &mut Vec<Value>| {
        if !vals.is_empty() {
            let ph = vals.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            params.extend(vals.iter().map(|v| Value::Text(v.clone())));
            w.push(sql.replace("{}", &ph));
        }
    };
    ids(
        "EXISTS (SELECT 1 FROM library_item_tag lt WHERE lt.item_id = i.id AND lt.tag_id IN ({}))",
        &f.tag_ids,
        params,
    );
    ids(
        "EXISTS (SELECT 1 FROM library_item_collection lc WHERE lc.item_id = i.id AND lc.collection_id IN ({}))",
        &f.collection_ids,
        params,
    );
    if f.inbox {
        w.push(
            "NOT EXISTS (SELECT 1 FROM library_item_collection lc WHERE lc.item_id = i.id)".into(),
        );
    }
    if let Some(d) = f.min_duration_ms {
        params.push(Value::Integer(d));
        w.push("i.duration_ms >= ?".into());
    }
    if let Some(d) = f.max_duration_ms {
        params.push(Value::Integer(d));
        w.push("i.duration_ms <= ?".into());
    }
    let mut list = |col: &str, vals: Vec<&'static str>, params: &mut Vec<Value>| {
        if !vals.is_empty() {
            let ph = vals.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            params.extend(vals.into_iter().map(|v| Value::Text(v.to_owned())));
            w.push(format!("{col} IN ({ph})"));
        }
    };
    list(
        "i.source_platform",
        f.platforms.iter().map(|p| p.as_str()).collect(),
        params,
    );
    list(
        "i.kind",
        f.kinds.iter().map(|k| k.as_str()).collect(),
        params,
    );
    list(
        "i.metadata_status",
        f.metadata_status.iter().map(|m| m.as_str()).collect(),
        params,
    );
    if f.favorite_only {
        w.push("i.is_favorite = 1".into());
    }
    if let Some(t) = f.added_since {
        params.push(Value::Integer(t));
        w.push("i.added_at >= ?".into());
    }
    match f.archived {
        ArchiveFilter::Exclude => w.push("i.archived_at IS NULL".into()),
        ArchiveFilter::Include => {}
        ArchiveFilter::Only => w.push("i.archived_at IS NOT NULL".into()),
    }
    w.join(" AND ")
}

fn order_sql(sort: Sort, has_query: bool) -> String {
    let dir = if sort.descending { "DESC" } else { "ASC" };
    let keyed = |key: &str| format!("({key}) IS NULL, {key} {dir}, i.id");
    match sort.field {
        SortField::Relevance if has_query => "rank, i.added_at DESC, i.id".into(),
        SortField::Relevance | SortField::AddedAt => format!("i.added_at {dir}, i.id"),
        SortField::UpdatedAt => format!("i.updated_at {dir}, i.id"),
        SortField::Duration => keyed("i.duration_ms"),
        SortField::PublishedAt => {
            keyed("COALESCE(i.published_at, unixepoch(i.published_date) * 1000)")
        }
        SortField::Title => {
            keyed("(SELECT lower(t.title) FROM search_document t WHERE t.rowid = i.seq)")
        }
    }
}

pub fn search(c: &Connection, q: &SearchQuery) -> super::Result<SearchPage> {
    let fts = build_match(&q.text, q.prefix_last);
    let notice =
        (fts.is_none() && !q.text.trim().is_empty()).then_some(QueryNotice::NoSearchableTerms);
    let limit = q.limit.clamp(1, MAX_PAGE);

    let mut params: Vec<Value> = Vec::new();
    // `?1` is the match, used by both indexes; filters bind `?2…` in order.
    let (with, from, select_extra) = match &fts {
        Some(m) => {
            params.push(Value::Text(m.clone()));
            let w = WEIGHTS.map(|x| x.to_string()).join(", ");
            (
                format!(
                    "WITH m AS MATERIALIZED (SELECT rowid AS seq, bm25(search_document, {w}) AS rank \
                       FROM search_document WHERE search_document MATCH ?1), \
                     tc AS MATERIALIZED (SELECT c.item_id, bm25(transcript_search) AS rank \
                       FROM transcript_search JOIN search_chunk c ON c.seq = transcript_search.rowid \
                       JOIN transcript t ON t.id = c.transcript_id \
                       WHERE transcript_search MATCH ?1 AND t.status IN {SEARCHED}), \
                     tm AS MATERIALIZED (SELECT item_id, min(rank) AS rank FROM tc GROUP BY item_id) "
                ),
                "library_item i LEFT JOIN m ON m.seq = i.seq LEFT JOIN tm ON tm.item_id = i.id \
                 WHERE (m.seq IS NOT NULL OR tm.item_id IS NOT NULL) AND "
                    .to_owned(),
                format!(
                    ", ifnull(m.rank, 0) + {TRANSCRIPT_WEIGHT:?} * ifnull(tm.rank, 0) AS rank, \
                     m.seq IS NOT NULL, tm.item_id IS NOT NULL"
                ),
            )
        }
        None => (
            String::new(),
            "library_item i WHERE ".to_owned(),
            String::new(),
        ),
    };
    let wh = filter_sql(&q.filters, &mut params);

    let total: i64 = c.query_row(
        &format!("{with}SELECT count(*) FROM {from}{wh}"),
        params_from_iter(params.iter()),
        |r| r.get(0),
    )?;

    let order = order_sql(q.sort, fts.is_some());
    let sql = format!(
        "{with}SELECT i.id, i.seq{select_extra} FROM {from}{wh} ORDER BY {order} LIMIT {limit} OFFSET {}",
        q.offset
    );
    let mut stmt = c.prepare(&sql)?;
    // (id, seq, metadata matched, transcript matched)
    let rows = stmt
        .query_map(params_from_iter(params.iter()), |r| {
            let (meta, tr) = if fts.is_some() {
                (r.get::<_, bool>(3)?, r.get::<_, bool>(4)?)
            } else {
                (false, false)
            };
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, meta, tr))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let moments = match &fts {
        Some(m) => {
            let ids: Vec<&str> = rows.iter().filter(|r| r.3).map(|r| r.0.as_str()).collect();
            transcript_matches(c, m, &ids)?
        }
        None => Default::default(),
    };
    let mut moments = moments;

    let mut hits = Vec::with_capacity(rows.len());
    for (id, seq, meta, _) in rows {
        let item = repo::get_item(c, &id)?.ok_or(super::LibraryError::NotFound)?;
        let (mut matched, snippet) = match (&fts, meta) {
            (Some(m), true) => metadata_snippet(c, m, seq, &item)?,
            _ => (Vec::new(), None),
        };
        let transcript = moments.remove(&id);
        if transcript.is_some() {
            matched.push(Field::Transcript);
        }
        hits.push(Hit {
            item,
            matched,
            snippet,
            transcript,
        });
    }
    Ok(SearchPage {
        total: total as u64,
        hits,
        notice,
    })
}

/// Matched fields and the snippet of one item's metadata.
fn metadata_snippet(
    c: &Connection,
    m: &str,
    seq: i64,
    item: &LibraryItem,
) -> super::Result<(Vec<Field>, Option<Snippet>)> {
    let hl_cols = (0..FIELDS.len())
        .map(|k| format!("highlight(search_document, {k}, char(1), char(2))"))
        .collect::<Vec<_>>()
        .join(", ");
    let hl: Vec<String> = c.query_row(
        &format!(
            "SELECT {hl_cols} FROM search_document WHERE search_document MATCH ?1 AND rowid = ?2"
        ),
        rusqlite::params![m, seq],
        |r| (0..FIELDS.len()).map(|k| r.get(k)).collect(),
    )?;
    let text = repo::search_text(item);
    let originals = [
        &text.title,
        &text.description,
        &text.creator,
        &text.notes,
        &text.tags,
    ];
    let matched: Vec<Field> = FIELDS
        .iter()
        .zip(&hl)
        .filter(|(_, h)| h.contains(HL_START))
        .map(|(f, _)| *f)
        .collect();
    let snippet = SNIPPET_PREFERENCE
        .iter()
        .find(|f| matched.contains(f))
        .and_then(|&f| {
            let k = FIELDS.iter().position(|x| *x == f)?;
            Some(make_snippet(f, &hl[k], originals[k]))
        });
    Ok((matched, snippet))
}

/// The moments of `items` where the match `m` is found, grouped per item.
/// Each item's moments come from one transcript: a `ready` one before a
/// `partial` one, then the newest, so the same words said once are not
/// listed twice from site captions and a local transcript.
fn transcript_matches(
    c: &Connection,
    m: &str,
    items: &[&str],
) -> super::Result<std::collections::HashMap<String, TranscriptMatches>> {
    let mut out = std::collections::HashMap::new();
    if items.is_empty() {
        return Ok(out);
    }
    let ph = items.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT c.item_id, t.id, t.language, t.provenance, t.status, t.covered_ms, \
             c.start_segment_id, c.end_segment_id, c.text, \
             highlight(transcript_search, 0, char(1), char(2)) \
         FROM transcript_search JOIN search_chunk c ON c.seq = transcript_search.rowid \
         JOIN transcript t ON t.id = c.transcript_id \
         WHERE transcript_search MATCH ?1 AND t.status IN {SEARCHED} AND c.item_id IN ({ph}) \
         ORDER BY c.item_id, t.status = 'ready' DESC, t.updated_at DESC, t.id, c.start_ms"
    );
    let mut params: Vec<Value> = vec![Value::Text(m.to_owned())];
    params.extend(items.iter().map(|s| Value::Text((*s).to_owned())));
    let mut st = c.prepare(&sql)?;
    let mut rows = st.query(params_from_iter(params.iter()))?;
    while let Some(r) = rows.next()? {
        let item: String = r.get(0)?;
        let tid: String = r.get(1)?;
        let entry = out.entry(item).or_insert_with(|| TranscriptMatches {
            transcript_id: tid.clone(),
            language: String::new(),
            provenance: String::new(),
            partial: false,
            covered_ms: None,
            moments: Vec::new(),
            total: 0,
        });
        if entry.transcript_id != tid {
            continue; // another transcript of the same item
        }
        if entry.total == 0 {
            entry.language = r.get(2)?;
            entry.provenance = r.get(3)?;
            entry.partial = r.get::<_, String>(4)? == "partial";
            entry.covered_ms = r.get(5)?;
        }
        entry.total += 1;
        if entry.moments.len() >= MAX_MOMENTS {
            continue;
        }
        let (first, last): (String, String) = (r.get(6)?, r.get(7)?);
        let text: String = r.get(8)?;
        let hl: String = r.get(9)?;
        if let Some(mo) = moment(c, &tid, &first, &last, &text, &hl)? {
            entry.moments.push(mo);
        }
    }
    Ok(out)
}

/// The segment of a chunk that holds the first matched word, with a
/// snippet around it. A chunk's text is its segments joined by one space
/// (`transcripts::write_chunks`), so a character offset names a segment.
fn moment(
    c: &Connection,
    transcript_id: &str,
    first: &str,
    last: &str,
    text: &str,
    highlighted: &str,
) -> super::Result<Option<Moment>> {
    let mut st = c.prepare_cached(
        "SELECT s.id, s.start_ms, s.end_ms, s.text FROM transcript_segment s \
         WHERE s.transcript_id = ?1 \
           AND s.ordinal BETWEEN (SELECT ordinal FROM transcript_segment WHERE id = ?2) \
                             AND (SELECT ordinal FROM transcript_segment WHERE id = ?3) \
         ORDER BY s.ordinal",
    )?;
    let segs = st
        .query_map(rusqlite::params![transcript_id, first, last], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let at = highlighted
        .chars()
        .filter(|&ch| ch != HL_END)
        .position(|ch| ch == HL_START)
        .unwrap_or(0);
    let mut end = 0;
    let mut pick = segs.last();
    for s in &segs {
        end += s.3.chars().count() + 1; // the joining space
        if at < end {
            pick = Some(s);
            break;
        }
    }
    Ok(pick.map(|s| Moment {
        segment_id: s.0.clone(),
        start_ms: s.1,
        end_ms: s.2,
        snippet: make_snippet(Field::Transcript, highlighted, text),
    }))
}

/// Characters of context kept before the first match and in total.
const BEFORE: usize = 40;
const WINDOW: usize = 160;

/// Rebuilds the highlighted index text with the original characters and
/// cuts a window around the first match.
fn make_snippet(field: Field, highlighted: &str, original: &str) -> Snippet {
    let orig: Vec<char> = display(original).chars().collect();
    // (char, highlighted?) in original characters.
    let mut chars: Vec<(char, bool)> = Vec::with_capacity(orig.len());
    let mut on = false;
    let mut k = 0;
    for c in highlighted.chars() {
        match c {
            HL_START => on = true,
            HL_END => on = false,
            c => {
                // Same length by construction; fall back to the index char.
                chars.push((orig.get(k).copied().unwrap_or(c), on));
                k += 1;
            }
        }
    }
    let first = chars.iter().position(|(_, h)| *h).unwrap_or(0);
    let start = first.saturating_sub(BEFORE);
    let end = (start + WINDOW).min(chars.len());
    let mut parts: Vec<SnippetPart> = Vec::new();
    for &(c, h) in &chars[start..end] {
        match parts.last_mut() {
            Some(p) if p.highlight == h => p.text.push(c),
            _ => parts.push(SnippetPart {
                text: c.to_string(),
                highlight: h,
            }),
        }
    }
    Snippet {
        field,
        parts,
        cut_start: start > 0,
        cut_end: end < chars.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_text_never_becomes_fts_syntax() {
        assert_eq!(
            build_match("hello world", false).unwrap(),
            r#""hello" "world""#
        );
        assert_eq!(build_match("hello wor", true).unwrap(), r#""hello" "wor"*"#);
        assert_eq!(build_match("hello ", true).unwrap(), r#""hello""#);
        assert_eq!(
            build_match(r#"a "b c" d"#, true).unwrap(),
            r#""a" "b c" "d"*"#
        );
        assert_eq!(
            build_match(r#""open phrase"#, true).unwrap(),
            r#""open phrase""#
        );
        assert_eq!(
            build_match("NOT x AND", false).unwrap(),
            r#""NOT" "x" "AND""#
        );
        assert_eq!(build_match("ışık", false).unwrap(), r#""işik""#);
        assert_eq!(build_match("col:x^ y*", false).unwrap(), r#""col:x^" "y*""#);
        for empty in ["", "   ", "'", "\"\"", "* - ( ) :", "\"", "AND"] {
            let r = build_match(empty, true);
            if empty == "AND" {
                assert_eq!(r.unwrap(), r#""AND"*"#);
            } else {
                assert_eq!(r, None, "{empty:?}");
            }
        }
    }

    #[test]
    fn index_text_keeps_length() {
        for s in ["ışık", "İstanbul", "a\u{1}b\u{2}c", "e\u{301}"] {
            assert_eq!(
                index_text(s).chars().count(),
                display(s).chars().count(),
                "{s:?}"
            );
        }
    }

    #[test]
    fn snippet_restores_original_characters() {
        let s = make_snippet(Field::Notes, "\u{1}işik\u{2} hizi", "ışık hızı");
        assert_eq!(s.text(), "ışık hızı");
        assert_eq!(
            s.parts[0],
            SnippetPart {
                text: "ışık".into(),
                highlight: true
            }
        );
        assert!(!s.cut_start && !s.cut_end);
        let long = format!("{} \u{1}hedef\u{2} {}", "a".repeat(100), "b".repeat(300));
        let orig = format!("{} hedef {}", "a".repeat(100), "b".repeat(300));
        let s = make_snippet(Field::Description, &long, &orig);
        assert!(s.cut_start && s.cut_end);
        assert_eq!(s.text().chars().count(), WINDOW);
        assert!(orig.contains(&s.text()), "a real substring");
    }
}
