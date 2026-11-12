//! Transcripts, their segments and search chunks (SPEC §11.5, §16.2;
//! STATE.md F6.1, F6.4; ADR-0026).
//!
//! Rules kept here:
//! - Segments are checked before anything is written: `0 <= start < end`,
//!   starts never go back, text is not empty, and no segment ends after the
//!   known media duration plus [`END_TOLERANCE_MS`]. Inside the tolerance
//!   the end is cut to the duration; past it the whole transcript is
//!   refused. Times are never invented.
//! - [`save`] replaces a transcript's segments and rebuilds its chunks and
//!   their index rows in one transaction, so search never sees half of an
//!   old and half of a new transcript.
//! - Each chunk joins neighbouring segments of one transcript and keeps
//!   the real time range of its first and last segment.
//! - Transcripts of the same item with another provenance, language or
//!   model are kept apart (`transcript_identity`).

use crate::library::new_id;
use crate::library::search::index_text;
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};

/// How far past the known duration a segment may end and still be kept
/// (cut to the duration). Caption files and decoders round the last cue.
pub const END_TOLERANCE_MS: i64 = 1_000;

/// A chunk ends once it holds this much text or time.
pub const CHUNK_MAX_CHARS: usize = 400;
pub const CHUNK_MAX_MS: i64 = 30_000;

#[derive(Debug)]
pub enum TranscriptError {
    Sqlite(rusqlite::Error),
    NotFound,
    /// A segment broke a §11.5 rule. The message names the rule and the
    /// segment's position, never its text.
    Invalid(String),
}

impl std::fmt::Display for TranscriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranscriptError::Sqlite(e) => write!(f, "sqlite: {e}"),
            TranscriptError::NotFound => write!(f, "transcript not found"),
            TranscriptError::Invalid(m) => write!(f, "invalid transcript: {m}"),
        }
    }
}

impl std::error::Error for TranscriptError {}

impl From<rusqlite::Error> for TranscriptError {
    fn from(e: rusqlite::Error) -> Self {
        TranscriptError::Sqlite(e)
    }
}

pub type Result<T> = std::result::Result<T, TranscriptError>;

macro_rules! text_enum {
    ($name:ident { $($v:ident = $s:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name { $($v),+ }
        impl $name {
            pub fn as_str(self) -> &'static str { match self { $($name::$v => $s),+ } }
            pub fn parse(s: &str) -> Option<Self> { match s { $($s => Some($name::$v),)+ _ => None } }
        }
    };
}

text_enum!(Provenance {
    SourceCaptions = "source_captions",
    GeneratedLocal = "generated_local",
    Imported = "imported",
});

text_enum!(Status {
    Pending = "pending",
    Processing = "processing",
    Partial = "partial",
    Ready = "ready",
    Failed = "failed",
    Canceled = "canceled",
    Stale = "stale",
});

text_enum!(CaptionKind {
    Manual = "manual",
    Automatic = "automatic",
});

#[derive(Debug, Clone, PartialEq)]
pub struct NewSegment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub confidence: Option<f64>,
    pub speaker: Option<String>,
}

impl NewSegment {
    pub fn new(start_ms: i64, end_ms: i64, text: impl Into<String>) -> Self {
        NewSegment {
            start_ms,
            end_ms,
            text: text.into(),
            confidence: None,
            speaker: None,
        }
    }
}

/// Who made a transcript and from what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    pub item_id: String,
    pub asset_id: Option<String>,
    pub language: String,
    pub provenance: Provenance,
    /// `source_captions`, `whisper_cpp` or `import`.
    pub provider: String,
    pub model_id: Option<String>,
    pub model_version: Option<String>,
    pub source_fingerprint: String,
    pub caption_kind: Option<CaptionKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    pub id: String,
    pub origin: Origin,
    pub status: Status,
    pub covered_ms: Option<i64>,
    pub segment_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub id: String,
    pub ordinal: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub confidence: Option<f64>,
    pub speaker: Option<String>,
}

/// Checks `segs` against §11.5 and returns them as they will be stored:
/// trimmed text, ends inside the tolerance cut to `duration_ms`.
pub fn validate(segs: &[NewSegment], duration_ms: Option<i64>) -> Result<Vec<NewSegment>> {
    let bad = |i: usize, rule: &str| TranscriptError::Invalid(format!("segment {i}: {rule}"));
    let mut out = Vec::with_capacity(segs.len());
    let mut last_start = 0;
    for (i, s) in segs.iter().enumerate() {
        let text = s.text.trim();
        if text.is_empty() {
            return Err(bad(i, "empty text"));
        }
        if text.chars().count() > 10_000 {
            return Err(bad(i, "text too long"));
        }
        if s.start_ms < 0 || s.start_ms >= s.end_ms {
            return Err(bad(i, "needs 0 <= start < end"));
        }
        if s.start_ms < last_start {
            return Err(bad(i, "starts before the previous segment"));
        }
        if s.confidence.is_some_and(|c| !(0.0..=1.0).contains(&c)) {
            return Err(bad(i, "confidence outside 0..1"));
        }
        let mut end = s.end_ms;
        if let Some(d) = duration_ms {
            if s.start_ms >= d {
                return Err(bad(i, "starts after the media ends"));
            }
            if end > d + END_TOLERANCE_MS {
                return Err(bad(i, "ends after the media ends"));
            }
            end = end.min(d);
        }
        last_start = s.start_ms;
        out.push(NewSegment {
            start_ms: s.start_ms,
            end_ms: end,
            text: text.to_owned(),
            confidence: s.confidence,
            speaker: s.speaker.clone(),
        });
    }
    Ok(out)
}

/// Groups consecutive segments into chunks: (first index, last index). A
/// segment that would stretch a chunk past [`CHUNK_MAX_MS`] starts the
/// next one, so a hit's time range stays close to the words it matched.
pub fn chunk_ranges(segs: &[NewSegment]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = 0;
    for (i, s) in segs.iter().enumerate() {
        if i > start && s.end_ms - segs[start].start_ms > CHUNK_MAX_MS {
            out.push((start, i - 1));
            start = i;
            chars = 0;
        }
        chars += s.text.chars().count() + 1;
        if chars >= CHUNK_MAX_CHARS {
            out.push((start, i));
            start = i + 1;
            chars = 0;
        }
    }
    if start < segs.len() {
        out.push((start, segs.len() - 1));
    }
    out
}

const COLS: &str =
    "t.id, t.item_id, t.asset_id, t.language, t.provenance, t.provider, t.model_id, \
     t.model_version, t.source_fingerprint, t.caption_kind, t.status, t.covered_ms, \
     (SELECT count(*) FROM transcript_segment s WHERE s.transcript_id = t.id), \
     t.created_at, t.updated_at";

fn from_row(r: &Row) -> rusqlite::Result<Transcript> {
    let prov: String = r.get(4)?;
    let kind: Option<String> = r.get(9)?;
    let status: String = r.get(10)?;
    Ok(Transcript {
        id: r.get(0)?,
        origin: Origin {
            item_id: r.get(1)?,
            asset_id: r.get(2)?,
            language: r.get(3)?,
            provenance: Provenance::parse(&prov).unwrap_or(Provenance::Imported),
            provider: r.get(5)?,
            model_id: r.get(6)?,
            model_version: r.get(7)?,
            source_fingerprint: r.get(8)?,
            caption_kind: kind.as_deref().and_then(CaptionKind::parse),
        },
        status: Status::parse(&status).unwrap_or(Status::Failed),
        covered_ms: r.get(11)?,
        segment_count: r.get(12)?,
        created_at: r.get(13)?,
        updated_at: r.get(14)?,
    })
}

pub fn get(c: &Connection, id: &str) -> Result<Option<Transcript>> {
    Ok(c.query_row(
        &format!("SELECT {COLS} FROM transcript t WHERE t.id = ?1"),
        [id],
        from_row,
    )
    .optional()?)
}

/// An item's transcripts, oldest first.
pub fn list_for_item(c: &Connection, item_id: &str) -> Result<Vec<Transcript>> {
    let mut st = c.prepare(&format!(
        "SELECT {COLS} FROM transcript t WHERE t.item_id = ?1 ORDER BY t.created_at, t.id"
    ))?;
    let rows = st.query_map([item_id], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn segments(c: &Connection, transcript_id: &str) -> Result<Vec<Segment>> {
    let mut st = c.prepare(
        "SELECT id, ordinal, start_ms, end_ms, text, confidence, speaker \
         FROM transcript_segment WHERE transcript_id = ?1 ORDER BY ordinal",
    )?;
    let rows = st.query_map([transcript_id], |r| {
        Ok(Segment {
            id: r.get(0)?,
            ordinal: r.get(1)?,
            start_ms: r.get(2)?,
            end_ms: r.get(3)?,
            text: r.get(4)?,
            confidence: r.get(5)?,
            speaker: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn find(c: &Connection, o: &Origin) -> Result<Option<String>> {
    Ok(c.query_row(
        "SELECT id FROM transcript WHERE item_id = ?1 AND provenance = ?2 AND language = ?3 \
             AND ifnull(model_id, '') = ifnull(?4, '')",
        params![o.item_id, o.provenance.as_str(), o.language, o.model_id],
        |r| r.get(0),
    )
    .optional()?)
}

/// Writes a transcript and its segments, replacing the segments and chunks
/// of the transcript with the same identity (§16.2). `duration_ms` is the
/// media's known duration, if any. Returns the stored transcript.
pub fn save(
    tx: &Transaction,
    origin: &Origin,
    status: Status,
    covered_ms: Option<i64>,
    segs: &[NewSegment],
    duration_ms: Option<i64>,
    now: i64,
) -> Result<Transcript> {
    if origin.provenance == Provenance::GeneratedLocal
        && (origin.model_id.is_none() || origin.model_version.is_none())
    {
        return Err(TranscriptError::Invalid(
            "a local transcript names its model".into(),
        ));
    }
    let segs = validate(segs, duration_ms)?;
    let id = match find(tx, origin)? {
        Some(id) => {
            // Cascades to the chunks; their trigger drops the index rows.
            tx.execute(
                "DELETE FROM transcript_segment WHERE transcript_id = ?1",
                [&id],
            )?;
            tx.execute(
                "UPDATE transcript SET asset_id = ?2, provider = ?3, model_version = ?4, \
                     source_fingerprint = ?5, caption_kind = ?6, status = ?7, covered_ms = ?8, \
                     updated_at = ?9 WHERE id = ?1",
                params![
                    id,
                    origin.asset_id,
                    origin.provider,
                    origin.model_version,
                    origin.source_fingerprint,
                    origin.caption_kind.map(CaptionKind::as_str),
                    status.as_str(),
                    covered_ms,
                    now
                ],
            )?;
            id
        }
        None => {
            let id = new_id();
            tx.execute(
                "INSERT INTO transcript (id, item_id, asset_id, language, provider, model_id, \
                     model_version, provenance, status, source_fingerprint, covered_ms, \
                     caption_kind, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)",
                params![
                    id,
                    origin.item_id,
                    origin.asset_id,
                    origin.language,
                    origin.provider,
                    origin.model_id,
                    origin.model_version,
                    origin.provenance.as_str(),
                    status.as_str(),
                    origin.source_fingerprint,
                    covered_ms,
                    origin.caption_kind.map(CaptionKind::as_str),
                    now
                ],
            )?;
            id
        }
    };
    let mut seg_ids = Vec::with_capacity(segs.len());
    {
        let mut ins = tx.prepare(
            "INSERT INTO transcript_segment (id, transcript_id, ordinal, start_ms, end_ms, text, \
                 confidence, speaker) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for (i, s) in segs.iter().enumerate() {
            let sid = new_id();
            ins.execute(params![
                sid,
                id,
                i as i64,
                s.start_ms,
                s.end_ms,
                s.text,
                s.confidence,
                s.speaker
            ])?;
            seg_ids.push(sid);
        }
    }
    write_chunks(tx, &id, origin, &segs, &seg_ids)?;
    get(tx, &id)?.ok_or(TranscriptError::NotFound)
}

fn write_chunks(
    tx: &Transaction,
    transcript_id: &str,
    o: &Origin,
    segs: &[NewSegment],
    seg_ids: &[String],
) -> Result<()> {
    let mut ins = tx.prepare(
        "INSERT INTO search_chunk (id, item_id, transcript_id, start_segment_id, end_segment_id, \
             start_ms, end_ms, text, source_type, producer_provider, producer_model_id, \
             producer_model_version) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'transcript', ?9, ?10, ?11)",
    )?;
    let mut fts = tx.prepare("INSERT INTO transcript_search (rowid, text) VALUES (?1, ?2)")?;
    for (a, b) in chunk_ranges(segs) {
        let text = segs[a..=b]
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let end_ms = segs[a..=b].iter().map(|s| s.end_ms).max().unwrap_or(0);
        ins.execute(params![
            new_id(),
            o.item_id,
            transcript_id,
            seg_ids[a],
            seg_ids[b],
            segs[a].start_ms,
            end_ms,
            text,
            o.provider,
            o.model_id,
            o.model_version
        ])?;
        fts.execute(params![tx.last_insert_rowid(), index_text(&text)])?;
    }
    Ok(())
}

/// Sets the state of a transcript whose segments stay as they are (e.g.
/// `stale` after the media changed, `canceled`).
pub fn set_status(c: &Connection, id: &str, status: Status, now: i64) -> Result<()> {
    let n = c.execute(
        "UPDATE transcript SET status = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, status.as_str(), now],
    )?;
    if n == 0 {
        return Err(TranscriptError::NotFound);
    }
    Ok(())
}

/// Deletes a transcript with its segments, chunks and index rows.
pub fn delete(c: &Connection, id: &str) -> Result<()> {
    let n = c.execute("DELETE FROM transcript WHERE id = ?1", [id])?;
    if n == 0 {
        return Err(TranscriptError::NotFound);
    }
    Ok(())
}

/// Where a segment is, to open a search moment (F6.4, §16.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentPlace {
    pub item_id: String,
    pub transcript_id: String,
    pub segment_id: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub status: Status,
}

/// The segment's place, or `None` when it no longer exists (its transcript
/// was made again or deleted). The caller checks the status: a search
/// result opens only into a `ready` or `partial` transcript.
pub fn segment_place(c: &Connection, segment_id: &str) -> Result<Option<SegmentPlace>> {
    Ok(c.query_row(
        "SELECT t.item_id, t.id, s.id, s.start_ms, s.end_ms, t.status \
         FROM transcript_segment s JOIN transcript t ON t.id = s.transcript_id WHERE s.id = ?1",
        [segment_id],
        |r| {
            let status: String = r.get(5)?;
            Ok(SegmentPlace {
                item_id: r.get(0)?,
                transcript_id: r.get(1)?,
                segment_id: r.get(2)?,
                start_ms: r.get(3)?,
                end_ms: r.get(4)?,
                status: Status::parse(&status).unwrap_or(Status::Failed),
            })
        },
    )
    .optional()?)
}
