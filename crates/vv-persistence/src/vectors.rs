//! Vector repository: embedding index generations and chunk vectors (SPEC
//! §16.3, DEC-12, AC-24; STATE.md F7.1; ADR-0035).
//!
//! Rules kept here:
//! - Every vector belongs to a generation, and a generation to exactly one
//!   provider, model, model file and size. Vectors of two generations are
//!   never compared: [`search`] reads one generation only.
//! - At most one generation is `active` (answers searches) and at most one
//!   is `building` beside it. [`activate`] swaps them in one transaction;
//!   a canceled build is removed by [`drop_generation`] and the active one
//!   is untouched, so a half index never answers a search (AC-24).
//! - Vectors are stored L2-normalized as little-endian `f32`, so the dot
//!   product is the cosine similarity. A vector of the wrong length is
//!   refused, never padded.
//! - The search is exact (every vector of the generation is scored). A few
//!   hundred videos make tens of thousands of chunks, which is milliseconds
//!   of arithmetic; an approximate index can come when measurements ask for
//!   it (DEC-12).

use rusqlite::{params, Connection, OptionalExtension, Row};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;

#[derive(Debug)]
pub enum VectorError {
    Sqlite(rusqlite::Error),
    NotFound,
    Invalid(&'static str),
}

impl std::fmt::Display for VectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VectorError::Sqlite(e) => write!(f, "sqlite: {e}"),
            VectorError::NotFound => write!(f, "no such index generation"),
            VectorError::Invalid(m) => write!(f, "invalid vector: {m}"),
        }
    }
}

impl std::error::Error for VectorError {}

impl From<rusqlite::Error> for VectorError {
    fn from(e: rusqlite::Error) -> Self {
        VectorError::Sqlite(e)
    }
}

pub type Result<T> = std::result::Result<T, VectorError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationStatus {
    Building,
    Active,
}

impl GenerationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            GenerationStatus::Building => "building",
            GenerationStatus::Active => "active",
        }
    }
}

/// What a generation's vectors were made with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Producer {
    pub provider: String,
    pub model_id: String,
    /// The model file's pinned SHA-256: a new file is a new generation.
    pub model_version: String,
    pub dims: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generation {
    pub seq: i64,
    pub id: String,
    pub producer: Producer,
    pub status: GenerationStatus,
    pub created_at: i64,
    pub activated_at: Option<i64>,
}

const COLS: &str =
    "seq, id, provider, model_id, model_version, dims, status, created_at, activated_at";

fn from_row(r: &Row) -> rusqlite::Result<Generation> {
    let status: String = r.get(6)?;
    let dims: i64 = r.get(5)?;
    Ok(Generation {
        seq: r.get(0)?,
        id: r.get(1)?,
        producer: Producer {
            provider: r.get(2)?,
            model_id: r.get(3)?,
            model_version: r.get(4)?,
            dims: usize::try_from(dims).unwrap_or(0),
        },
        status: if status == "active" {
            GenerationStatus::Active
        } else {
            GenerationStatus::Building
        },
        created_at: r.get(7)?,
        activated_at: r.get(8)?,
    })
}

fn by_status(c: &Connection, s: GenerationStatus) -> Result<Option<Generation>> {
    Ok(c.query_row(
        &format!("SELECT {COLS} FROM embedding_generation WHERE status = ?1"),
        [s.as_str()],
        from_row,
    )
    .optional()?)
}

/// The generation searches use, if any.
pub fn active(c: &Connection) -> Result<Option<Generation>> {
    by_status(c, GenerationStatus::Active)
}

/// The generation being built beside the active one, if any.
pub fn building(c: &Connection) -> Result<Option<Generation>> {
    by_status(c, GenerationStatus::Building)
}

pub fn get(c: &Connection, id: &str) -> Result<Option<Generation>> {
    Ok(c.query_row(
        &format!("SELECT {COLS} FROM embedding_generation WHERE id = ?1"),
        [id],
        from_row,
    )
    .optional()?)
}

/// Starts a new generation for `p` beside the active one. A build already
/// under way for the same producer is returned as it is (its vectors are
/// kept); one for another producer is dropped first. With no active
/// generation at all the new one is still `building`: it answers searches
/// only once complete.
pub fn begin_build(c: &Connection, p: &Producer, id: &str, now: i64) -> Result<Generation> {
    if !(8..=4096).contains(&p.dims) {
        return Err(VectorError::Invalid("dimension out of range"));
    }
    if let Some(b) = building(c)? {
        if &b.producer == p {
            return Ok(b);
        }
        drop_generation(c, &b.id)?;
    }
    c.execute(
        "INSERT INTO embedding_generation (id, provider, model_id, model_version, dims, status, \
             created_at) VALUES (?1, ?2, ?3, ?4, ?5, 'building', ?6)",
        params![
            id,
            p.provider,
            p.model_id,
            p.model_version,
            p.dims as i64,
            now
        ],
    )?;
    get(c, id)?.ok_or(VectorError::NotFound)
}

/// Makes the building generation `id` the active one; the previous active
/// generation and all its vectors are deleted in the same transaction
/// (call inside one).
pub fn activate(c: &Connection, id: &str, now: i64) -> Result<Generation> {
    let g = get(c, id)?.ok_or(VectorError::NotFound)?;
    if g.status == GenerationStatus::Active {
        return Ok(g);
    }
    c.execute("DELETE FROM embedding_generation WHERE status = 'active'", [])?;
    c.execute(
        "UPDATE embedding_generation SET status = 'active', activated_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    get(c, id)?.ok_or(VectorError::NotFound)
}

/// Removes a generation and its vectors (a canceled build, or the index of
/// a model the user removed).
pub fn drop_generation(c: &Connection, id: &str) -> Result<bool> {
    Ok(c.execute("DELETE FROM embedding_generation WHERE id = ?1", [id])? > 0)
}

/// A chunk still to be embedded for a generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingChunk {
    pub chunk_seq: i64,
    pub text: String,
}

/// Up to `limit` chunks that have no vector in generation `gen_seq` yet,
/// oldest first, so a resumed build continues where it stopped.
pub fn pending(c: &Connection, gen_seq: i64, limit: usize) -> Result<Vec<PendingChunk>> {
    let mut st = c.prepare(
        "SELECT sc.seq, sc.text FROM search_chunk sc \
         WHERE sc.source_type = 'transcript' AND NOT EXISTS \
             (SELECT 1 FROM chunk_embedding e WHERE e.generation_seq = ?1 AND e.chunk_seq = sc.seq) \
         ORDER BY sc.seq LIMIT ?2",
    )?;
    let rows = st.query_map(params![gen_seq, limit as i64], |r| {
        Ok(PendingChunk {
            chunk_seq: r.get(0)?,
            text: r.get(1)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// (chunks with a vector in `gen_seq`, chunks that should have one).
pub fn coverage(c: &Connection, gen_seq: i64) -> Result<(i64, i64)> {
    let total: i64 = c.query_row(
        "SELECT count(*) FROM search_chunk WHERE source_type = 'transcript'",
        [],
        |r| r.get(0),
    )?;
    let done: i64 = c.query_row(
        "SELECT count(*) FROM chunk_embedding WHERE generation_seq = ?1",
        [gen_seq],
        |r| r.get(0),
    )?;
    Ok((done, total))
}

/// Fingerprint of the text a vector was made from (§16.3).
pub fn text_hash(text: &str) -> String {
    let d = Sha256::digest(text.as_bytes());
    d.iter().take(16).map(|b| format!("{b:02x}")).collect()
}

/// Scales `v` to unit length. A zero or non-finite vector is refused.
pub fn normalize(v: &mut [f32]) -> Result<()> {
    if v.iter().any(|x| !x.is_finite()) {
        return Err(VectorError::Invalid("not a finite vector"));
    }
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n <= f32::EPSILON {
        return Err(VectorError::Invalid("zero vector"));
    }
    for x in v.iter_mut() {
        *x /= n;
    }
    Ok(())
}

pub fn encode(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

pub fn decode(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Stores the vector of one chunk. `v` must be `dims` long; it is
/// normalized here. A chunk deleted meanwhile (its transcript was rewritten)
/// is skipped and reported as `false`.
pub fn put(c: &Connection, g: &Generation, chunk_seq: i64, text: &str, v: &[f32]) -> Result<bool> {
    if v.len() != g.producer.dims {
        return Err(VectorError::Invalid("wrong dimension"));
    }
    let mut v = v.to_vec();
    normalize(&mut v)?;
    let n = c.execute(
        "INSERT INTO chunk_embedding (generation_seq, chunk_seq, text_hash, vector) \
         SELECT ?1, seq, ?3, ?4 FROM search_chunk WHERE seq = ?2 \
         ON CONFLICT (generation_seq, chunk_seq) DO UPDATE SET text_hash = excluded.text_hash, \
             vector = excluded.vector",
        params![g.seq, chunk_seq, text_hash(text), encode(&v)],
    )?;
    Ok(n > 0)
}

/// One scored chunk.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub chunk_seq: i64,
    /// Cosine similarity in [-1, 1]. Not a probability and not calibrated
    /// (§16.3): only for ordering.
    pub score: f32,
}

/// The `k` chunks of generation `g` closest to `query` (exact search).
/// `query` must have the generation's dimension; it is normalized here.
pub fn search(c: &Connection, g: &Generation, query: &[f32], k: usize) -> Result<Vec<Hit>> {
    if query.len() != g.producer.dims {
        return Err(VectorError::Invalid("query has the wrong dimension"));
    }
    let mut q = query.to_vec();
    normalize(&mut q)?;
    let mut st =
        c.prepare_cached("SELECT chunk_seq, vector FROM chunk_embedding WHERE generation_seq = ?1")?;
    let mut rows = st.query([g.seq])?;
    let mut top: Vec<Hit> = Vec::with_capacity(k + 1);
    while let Some(r) = rows.next()? {
        let seq: i64 = r.get(0)?;
        let blob = r.get_ref(1)?.as_blob().map_err(rusqlite::Error::from)?;
        if blob.len() != q.len() * 4 {
            continue;
        }
        let score: f32 = blob
            .chunks_exact(4)
            .zip(&q)
            .map(|(b, y)| f32::from_le_bytes([b[0], b[1], b[2], b[3]]) * y)
            .sum();
        if top.len() < k || score > top.last().map_or(f32::MIN, |h| h.score) {
            let at = top
                .iter()
                .position(|h| score > h.score)
                .unwrap_or(top.len());
            top.insert(at, Hit { chunk_seq: seq, score });
            top.truncate(k);
        }
    }
    top.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(Ordering::Equal)
            .then(a.chunk_seq.cmp(&b.chunk_seq))
    });
    Ok(top)
}
