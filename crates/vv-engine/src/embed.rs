//! Search by meaning: the user's embedding API and the index jobs (SPEC
//! §16.3, DEC-12, AI-C07, AC-24; STATE.md F7.1; ADR-0035).
//!
//! - VideoVault hosts no embedding model. The user gives the address of an
//!   OpenAI-compatible `/v1/embeddings` API (Ollama, LM Studio, llama.cpp's
//!   server, a cloud service), a model name and, when the API wants one,
//!   a key (kept in the OS credential store). [`check`] asks it for one
//!   vector before anything is saved, and the vector length is recorded.
//! - A host other than this PC receives transcript text, so it needs the
//!   user's explicit agreement (`remote_allowed`, AI-C07) and is blocked
//!   in Offline Mode (DATA-05). Loopback addresses (`localhost`, `127.*`,
//!   `::1`; WSL2 forwards Ollama there) never leave the PC and work offline.
//! - An index generation records the provider, the address, the model and
//!   the length (§16.3): another server or model is another generation and
//!   vectors are never mixed. The API cannot prove which weights answer, so
//!   a model changed behind the same name is not detected; the screen says
//!   to rebuild after changing the model.
//! - [`request_index`] starts a build or fills the active index. A build
//!   for a new API or model runs beside the active index and replaces it
//!   only when complete; cancel drops the build and leaves the active
//!   index as it was (AC-24). Pause keeps the vectors made so far.
//! - Requests carry [`BATCH`] passages; each prompt is cleaned and cut to
//!   [`PROMPT_MAX_BYTES`] (some servers refuse, rather than cut, an input
//!   longer than their context). A cancel is seen within 50 ms.

use crate::context::{Context, Store};
use crate::worker::{Stop, Work};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::sync::mpsc;
use std::time::Duration;
use vv_domain::error::ErrorCode;
use vv_persistence::jobs::{self, Enqueued, JobError, JobKind, Lease, NewJob, Progress};
use vv_persistence::rusqlite::Connection;
use vv_persistence::settings::{self, EmbeddingApi};
use vv_persistence::vectors::{self, Generation, GenerationStatus, Producer};
use vv_secrets::{SecretRef, SecretStore};

/// Provider name stored with every generation.
pub const PROVIDER: &str = "openai_compatible";
/// Texts per request: a cancel waits at most one batch.
pub const BATCH: usize = 32;
/// Largest response accepted (32 vectors of 4096 floats as JSON are about
/// 3 MB).
const RESPONSE_MAX: u64 = 64 << 20;
/// One batch on a slow processor (a CPU-only Ollama).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// A server that does not answer at all.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// How often a waiting job looks at cancel.
const POLL: Duration = Duration::from_millis(50);
/// Longest prompt sent. A chunk is at most 400 characters; a longer text
/// (one very long caption line) is cut so it fits a 512-token context
/// (a token is at least one UTF-8 byte).
pub const PROMPT_MAX_BYTES: usize = 1800;

/// The text sent: control characters removed, spaces collapsed, cut at a
/// character boundary. An empty text becomes one space (some servers
/// refuse an empty input).
pub fn prompt(text: &str) -> String {
    let clean: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut end = clean.len().min(PROMPT_MAX_BYTES);
    while !clean.is_char_boundary(end) {
        end -= 1;
    }
    let clean = clean.get(..end).unwrap_or_default().to_owned();
    if clean.is_empty() {
        " ".to_owned()
    } else {
        clean
    }
}

/// Why an address cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlError {
    /// Not an http(s) address.
    Invalid,
    /// A user name or password in the address (keys go in the key field).
    Credentials,
}

/// The base address as stored: http or https, no credentials, query or
/// fragment, no trailing slash. `/embeddings` is put after it.
pub fn normalize_url(s: &str) -> Result<String, UrlError> {
    let u = url::Url::parse(s.trim()).map_err(|_| UrlError::Invalid)?;
    if !matches!(u.scheme(), "http" | "https") || u.host_str().is_none_or(str::is_empty) {
        return Err(UrlError::Invalid);
    }
    if !u.username().is_empty() || u.password().is_some() {
        return Err(UrlError::Credentials);
    }
    if u.query().is_some() || u.fragment().is_some() {
        return Err(UrlError::Invalid);
    }
    let mut out = u.to_string();
    while out.ends_with('/') {
        out.pop();
    }
    Ok(out)
}

/// The address is this PC (text sent there does not leave it).
pub fn is_loopback(url: &str) -> bool {
    let Ok(u) = url::Url::parse(url) else {
        return false;
    };
    match u.host() {
        Some(url::Host::Domain(d)) => d.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

#[derive(Deserialize)]
struct Out {
    data: Vec<OutItem>,
}

#[derive(Deserialize)]
struct OutItem {
    #[serde(default)]
    index: Option<usize>,
    embedding: Vec<f32>,
}

/// Reads an `/embeddings` answer: exactly `expected` vectors in input
/// order, each `dims` long (any length from 8 to 4096 when `dims` is
/// `None`, the first check), normalized.
pub fn parse_output(
    body: &[u8],
    expected: usize,
    dims: Option<usize>,
) -> Result<Vec<Vec<f32>>, String> {
    let out: Out = serde_json::from_slice(body).map_err(|_| "not an embeddings answer")?;
    if out.data.len() != expected {
        return Err(format!("{} vectors for {expected} texts", out.data.len()));
    }
    let mut items: Vec<(usize, Vec<f32>)> = out
        .data
        .into_iter()
        .enumerate()
        .map(|(i, d)| (d.index.unwrap_or(i), d.embedding))
        .collect();
    items.sort_by_key(|d| d.0);
    let want = dims.unwrap_or_else(|| items.first().map_or(0, |d| d.1.len()));
    if !(8..=4096).contains(&want) {
        return Err(format!("vectors of {want} values"));
    }
    let mut vs = Vec::with_capacity(expected);
    for (i, (index, mut v)) in items.into_iter().enumerate() {
        if index != i {
            return Err("vector indexes are not 0..n".into());
        }
        if v.len() != want {
            return Err(format!("a vector of {} values, expected {want}", v.len()));
        }
        vectors::normalize(&mut v).map_err(|e| e.to_string())?;
        vs.push(v);
    }
    Ok(vs)
}

/// Why a request did not give vectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedError {
    /// Asked to stop (cancel, pause, lost lease).
    Stopped,
    /// The code the screen explains, and a short message for the log
    /// (never the key or the text).
    Failed(ErrorCode, String),
}

/// What the screen says about a failed request.
pub fn error_code(e: &EmbedError) -> ErrorCode {
    match e {
        EmbedError::Stopped => ErrorCode::Canceled,
        EmbedError::Failed(c, _) => *c,
    }
}

/// One embedding API, ready for requests.
#[derive(Clone)]
pub struct Api {
    base: String,
    model: String,
    key: Option<String>,
    agent: ureq::Agent,
}

impl std::fmt::Debug for Api {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Api")
            .field("base", &self.base)
            .field("model", &self.model)
            .field("key", &self.key.as_ref().map(|_| "<set>"))
            .finish()
    }
}

impl Api {
    /// An API for `url` and `model`. Refuses a host other than this PC in
    /// Offline Mode, and one the user did not agree to send text to.
    pub fn new(
        url: &str,
        model: &str,
        key: Option<String>,
        remote_allowed: bool,
        offline: bool,
    ) -> Result<Api, EmbedError> {
        let base = normalize_url(url).map_err(|_| {
            EmbedError::Failed(ErrorCode::InvalidUrl, "embedding API address".into())
        })?;
        if !is_loopback(&base) {
            if offline {
                return Err(EmbedError::Failed(
                    ErrorCode::OfflineMode,
                    "the embedding API is not on this PC".into(),
                ));
            }
            if !remote_allowed {
                return Err(EmbedError::Failed(
                    ErrorCode::UrlBlocked,
                    "sending text to this address was not agreed to".into(),
                ));
            }
        }
        let model = model.trim();
        if model.is_empty() || model.len() > 200 {
            return Err(EmbedError::Failed(
                ErrorCode::ModelMissing,
                "model name".into(),
            ));
        }
        Ok(Api {
            base,
            model: model.to_owned(),
            key: key.filter(|k| !k.is_empty()),
            // No redirects: the text goes to the address the user gave.
            agent: ureq::AgentBuilder::new()
                .timeout_connect(CONNECT_TIMEOUT)
                .timeout(REQUEST_TIMEOUT)
                .redirects(0)
                .build(),
        })
    }

    /// The API in the settings, with its key from the credential store.
    pub fn from_settings(
        cfg: &EmbeddingApi,
        secrets: &dyn SecretStore,
        offline: bool,
    ) -> Result<Api, EmbedError> {
        let key = match &cfg.key_ref {
            Some(r) => {
                let r = SecretRef::parse(r).ok_or_else(|| {
                    EmbedError::Failed(ErrorCode::AccessDenied, "API key reference".into())
                })?;
                let s = secrets.get(&r).map_err(|_| {
                    EmbedError::Failed(ErrorCode::AccessDenied, "API key not found".into())
                })?;
                Some(String::from_utf8_lossy(s.expose()).into_owned())
            }
            None => None,
        };
        Api::new(&cfg.url, &cfg.model, key, cfg.remote_allowed, offline)
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// Vectors for `texts` with one request, in order; each `dims` long
    /// (`None`: whatever the first one has). A stop request ends the wait
    /// at once.
    pub fn embed(
        &self,
        texts: &[&str],
        dims: Option<usize>,
        mut should_stop: impl FnMut() -> bool,
    ) -> Result<Vec<Vec<f32>>, EmbedError> {
        let input: Vec<String> = texts.iter().map(|t| prompt(t)).collect();
        let body = serde_json::json!({
            "model": self.model,
            "input": input,
            "encoding_format": "float",
        })
        .to_string();
        let mut req = self
            .agent
            .post(&format!("{}/embeddings", self.base))
            .set("Content-Type", "application/json");
        if let Some(k) = &self.key {
            req = req.set("Authorization", &format!("Bearer {k}"));
        }
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let r = match req.send_string(&body) {
                Ok(resp) => {
                    let mut buf = Vec::new();
                    resp.into_reader()
                        .take(RESPONSE_MAX + 1)
                        .read_to_end(&mut buf)
                        .map(|_| buf)
                        .map_err(|e| (ErrorCode::NetworkUnavailable, e.kind().to_string()))
                }
                Err(ureq::Error::Status(code, _)) => Err((
                    match code {
                        401 | 403 => ErrorCode::AccessDenied,
                        404 => ErrorCode::ModelMissing,
                        429 => ErrorCode::RateLimited,
                        _ => ErrorCode::ExtractorFailed,
                    },
                    format!("HTTP {code}"),
                )),
                Err(ureq::Error::Transport(t)) => {
                    Err((ErrorCode::NetworkUnavailable, t.kind().to_string()))
                }
            };
            let _ = tx.send(r);
        });
        loop {
            match rx.recv_timeout(POLL) {
                Ok(Ok(buf)) if buf.len() as u64 > RESPONSE_MAX => {
                    return Err(EmbedError::Failed(
                        ErrorCode::ExtractorFailed,
                        "answer too large".into(),
                    ))
                }
                Ok(Ok(buf)) => {
                    return parse_output(&buf, texts.len(), dims)
                        .map_err(|m| EmbedError::Failed(ErrorCode::ExtractorFailed, m))
                }
                Ok(Err((code, m))) => return Err(EmbedError::Failed(code, m)),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(EmbedError::Failed(
                        ErrorCode::InternalError,
                        "request thread".into(),
                    ))
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if should_stop() {
                        return Err(EmbedError::Stopped);
                    }
                }
            }
        }
    }
}

/// Asks the API for one vector ("Check" in the settings): its length.
pub fn check(api: &Api) -> Result<usize, EmbedError> {
    let v = api.embed(
        &["VideoVault bağlantı denemesi / connection check"],
        None,
        || false,
    )?;
    Ok(v.first().map_or(0, Vec::len))
}

/// What the vectors of `cfg` are made with (§16.3).
pub fn producer(cfg: &EmbeddingApi) -> Producer {
    Producer {
        provider: PROVIDER.into(),
        model_id: cfg.model.clone(),
        model_version: cfg.url.clone(),
        dims: cfg.dims,
    }
}

/// The API in the settings, if one was set and checked.
pub fn configured(c: &Connection) -> Option<EmbeddingApi> {
    settings::embedding_api(c).ok().flatten()
}

// ------------------------------------------------------------------ index

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Payload {
    pub generation_id: String,
}

pub fn idempotency_key(generation_id: &str) -> String {
    format!("index:{generation_id}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexError {
    /// No embedding API is set.
    NoApi,
    Db,
}

fn new_id() -> String {
    vv_persistence::library::new_id()
}

/// Makes the index match the API in the settings: with an active index of
/// it, queues the chunks it is missing; otherwise starts a build beside it
/// (`rebuild` forces one even when the active index is current). Returns
/// the job, `None` when nothing needs doing.
pub fn request_index(
    c: &mut Connection,
    rebuild: bool,
    now: i64,
) -> Result<Option<Enqueued>, IndexError> {
    let p = producer(&configured(c).ok_or(IndexError::NoApi)?);
    let tx = c.transaction().map_err(|_| IndexError::Db)?;
    let active = vectors::active(&tx).map_err(|_| IndexError::Db)?;
    let current = active.filter(|a| a.producer == p);
    let (g, kind) = match current {
        Some(a) if !rebuild => {
            let (done, total) = vectors::coverage(&tx, a.seq).map_err(|_| IndexError::Db)?;
            if done >= total {
                return Ok(None);
            }
            (a, JobKind::Embedding)
        }
        _ => {
            let mut b =
                vectors::begin_build(&tx, &p, &new_id(), now).map_err(|_| IndexError::Db)?;
            if rebuild && vectors::coverage(&tx, b.seq).map_err(|_| IndexError::Db)?.0 > 0 {
                // "Rebuild" means from nothing; a half build of the same model is dropped.
                vectors::drop_generation(&tx, &b.id).map_err(|_| IndexError::Db)?;
                b = vectors::begin_build(&tx, &p, &new_id(), now).map_err(|_| IndexError::Db)?;
            }
            (b, JobKind::Reindex)
        }
    };
    let mut job = NewJob::new(kind, idempotency_key(&g.id));
    job.payload = serde_json::to_value(Payload {
        generation_id: g.id.clone(),
    })
    .map_err(|_| IndexError::Db)?;
    let q = jobs::enqueue_in(&tx, &job, now).map_err(|_| IndexError::Db)?;
    tx.commit().map_err(|_| IndexError::Db)?;
    Ok(Some(q))
}

/// After new transcript chunks were written: queue their vectors when an
/// index of the API in the settings is active. Never starts a build.
pub fn after_chunks_changed(c: &mut Connection, now: i64) -> Option<Enqueued> {
    let active = vectors::active(c).ok().flatten()?;
    if producer(&configured(c)?) != active.producer {
        return None;
    }
    request_index(c, false, now).ok().flatten()
}

/// Cancels the build under way, if any; the active index stays (AC-24).
pub fn cancel_build(c: &mut Connection, now: i64) -> bool {
    let Ok(Some(b)) = vectors::building(c) else {
        return false;
    };
    let key = idempotency_key(&b.id);
    let open: Vec<String> = jobs::list(c, &[], 500)
        .unwrap_or_default()
        .into_iter()
        .filter(|j| j.idempotency_key == key && !j.status.is_terminal())
        .map(|j| j.id)
        .collect();
    for id in &open {
        let _ = jobs::cancel(c, id, jobs::CancelReason::User, now);
    }
    // A job that is running drops the build when it sees the cancel; one
    // that never ran leaves it to us.
    if open.is_empty() || vectors::building(c).ok().flatten().is_some() {
        let running = jobs::list(c, &[], 500)
            .unwrap_or_default()
            .into_iter()
            .any(|j| j.idempotency_key == key && j.status.is_active());
        if !running {
            let _ = vectors::drop_generation(c, &b.id);
        }
    }
    true
}

/// The `embedding` and `reindex` jobs: vectors for every chunk the
/// generation is missing, then (a build) the swap.
pub fn run<S: Store>(cx: &Context<S>, lease: Lease) {
    let w = Work::new(cx, lease);
    let Ok(p) = serde_json::from_value::<Payload>(w.lease.job.payload.clone()) else {
        w.end_error(ErrorCode::InternalError, Some("index payload unreadable"));
        return;
    };
    run_in(cx, &w, &p);
    cx.events.job_changed(w.id(), None);
}

fn run_in<S: Store>(cx: &Context<S>, w: &Work<'_, S>, p: &Payload) {
    let g = cx
        .store
        .read(|c| vectors::get(c, &p.generation_id).ok().flatten());
    let Some(g) = g else {
        // Canceled or replaced meanwhile: nothing left to do.
        succeed(cx, w, None);
        return;
    };
    let Some(cfg) = cx.store.read(configured) else {
        w.end_error(ErrorCode::ModelMissing, Some("no embedding API is set"));
        return;
    };
    if producer(&cfg) != g.producer {
        // The settings point at another API or model than this index.
        w.end_error(
            ErrorCode::ModelMissing,
            Some("the index is for another embedding API or model"),
        );
        return;
    }
    let api = match Api::from_settings(&cfg, cx.secrets.as_ref(), cx.is_offline()) {
        Ok(a) => a,
        Err(e) => {
            w.end_error(error_code(&e), Some("the embedding API cannot be used"));
            return;
        }
    };
    loop {
        let batch = cx
            .store
            .read(|c| vectors::pending(c, g.seq, BATCH))
            .unwrap_or_default();
        let (done, total) = cx
            .store
            .read(|c| vectors::coverage(c, g.seq))
            .unwrap_or((0, 0));
        w.progress(
            &Progress {
                phase: Some(
                    if g.status == GenerationStatus::Building {
                        "build"
                    } else {
                        "update"
                    }
                    .into(),
                ),
                units_done: Some(done),
                units_total: Some(total),
                ..Default::default()
            },
            batch.is_empty(),
        );
        if batch.is_empty() {
            break;
        }
        if w.should_stop() {
            return stop(cx, w, &g);
        }
        let texts: Vec<&str> = batch.iter().map(|b| b.text.as_str()).collect();
        let vs = match api.embed(&texts, Some(g.producer.dims), || w.should_stop()) {
            Ok(v) => v,
            Err(EmbedError::Stopped) => return stop(cx, w, &g),
            Err(EmbedError::Failed(code, _)) => {
                w.end_error(code, Some("the embedding API failed"));
                return;
            }
        };
        let wrote = cx.store.write(|c| -> Result<(), vectors::VectorError> {
            let tx = c.transaction()?;
            for (b, v) in batch.iter().zip(&vs) {
                vectors::put(&tx, &g, b.chunk_seq, &b.text, v)?;
            }
            tx.commit()?;
            Ok(())
        });
        if wrote.is_err() {
            w.end_error(ErrorCode::InternalError, Some("vectors not written"));
            return;
        }
    }
    succeed(
        cx,
        w,
        (g.status == GenerationStatus::Building).then_some(&g),
    );
    // Chunks written after the last check: their own job picks them up.
    let now = cx.now();
    cx.store.write(|c| {
        let _ = after_chunks_changed(c, now);
    });
}

fn succeed<S: Store>(cx: &Context<S>, w: &Work<'_, S>, build: Option<&Generation>) {
    let now = cx.now();
    let (id, g) = (w.id().to_owned(), w.generation());
    let _ = cx.store.write(|c| {
        jobs::succeed_with(c, &id, g, now, |tx| {
            if let Some(b) = build {
                vectors::activate(tx, &b.id, now)
                    .map_err(|_| JobError::Invalid("activate index"))?;
            }
            Ok(())
        })
    });
}

fn stop<S: Store>(cx: &Context<S>, w: &Work<'_, S>, g: &Generation) {
    let s = w.stopped().unwrap_or(Stop::Cancel);
    if s == Stop::Cancel && g.status == GenerationStatus::Building {
        cx.store.write(|c| {
            let _ = vectors::drop_generation(c, &g.id);
        });
    }
    w.end_stopped(s);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_are_clean_and_bounded() {
        assert_eq!(prompt("a\nb"), "a b");
        assert_eq!(prompt("x\u{0}y"), "x y");
        assert_eq!(prompt("\n\t"), " ");
        assert_eq!(prompt("İstanbul'da  çay"), "İstanbul'da çay");
        // Cut at a character boundary, never inside "ş" (2 bytes).
        let long = format!("a{}", "ş".repeat(PROMPT_MAX_BYTES));
        let p = prompt(&long);
        assert_eq!(p.len(), PROMPT_MAX_BYTES - 1);
        assert!(p.ends_with('ş'));
    }

    #[test]
    fn output_is_in_order_normalized_and_of_one_length() {
        let v8 = |a: f32, b: f32| {
            let mut v = vec![0.0; 8];
            v[0] = a;
            v[1] = b;
            serde_json::to_string(&v).unwrap()
        };
        let json = format!(
            r#"{{"object":"list","data":[
 {{"object":"embedding","index":1,"embedding":{}}},
 {{"object":"embedding","index":0,"embedding":{}}}]}}"#,
            v8(0.0, 2.0),
            v8(3.0, 4.0)
        );
        let v = parse_output(json.as_bytes(), 2, None).unwrap();
        assert_eq!(&v[0][..2], &[0.6, 0.8]);
        assert_eq!(&v[1][..2], &[0.0, 1.0]);
        assert!(parse_output(json.as_bytes(), 2, Some(8)).is_ok());
        assert!(parse_output(json.as_bytes(), 2, Some(1024)).is_err());
        assert!(parse_output(json.as_bytes(), 3, None).is_err());
        // Ollama's answer without `index` is in input order.
        let plain = format!(r#"{{"data":[{{"embedding":{}}}]}}"#, v8(1.0, 0.0));
        assert_eq!(parse_output(plain.as_bytes(), 1, None).unwrap()[0][0], 1.0);
        let short = br#"{"data":[{"index":0,"embedding":[1,2]}]}"#;
        assert!(parse_output(short, 1, None).is_err());
        let zero = format!(r#"{{"data":[{{"embedding":{}}}]}}"#, v8(0.0, 0.0));
        assert!(parse_output(zero.as_bytes(), 1, None).is_err());
        assert!(parse_output(b"error: model not found", 1, None).is_err());
    }

    #[test]
    fn addresses_are_normalized_and_loopback_is_this_pc() {
        assert_eq!(
            normalize_url(" http://localhost:11434/v1/ ").unwrap(),
            "http://localhost:11434/v1"
        );
        assert_eq!(
            normalize_url("https://api.example.com/v1").unwrap(),
            "https://api.example.com/v1"
        );
        assert_eq!(normalize_url("ftp://x/v1"), Err(UrlError::Invalid));
        assert_eq!(normalize_url("localhost:11434"), Err(UrlError::Invalid));
        assert_eq!(normalize_url("http://u:p@h/v1"), Err(UrlError::Credentials));
        assert_eq!(normalize_url("http://h/v1?key=x"), Err(UrlError::Invalid));
        for l in [
            "http://localhost:11434/v1",
            "http://LOCALHOST/v1",
            "http://127.0.0.1:8080/v1",
            "http://127.5.5.5/v1",
            "http://[::1]:11434/v1",
        ] {
            assert!(is_loopback(l), "{l}");
        }
        for r in [
            "http://192.168.1.5:11434/v1",
            "https://api.openai.com/v1",
            "http://localhost.example.com/v1",
            "http://0.0.0.0/v1",
        ] {
            assert!(!is_loopback(r), "{r}");
        }
    }

    #[test]
    fn a_remote_api_needs_agreement_and_is_blocked_offline() {
        let remote = "https://api.example.com/v1";
        assert_eq!(
            error_code(&Api::new(remote, "m", None, false, false).unwrap_err()),
            ErrorCode::UrlBlocked
        );
        assert_eq!(
            error_code(&Api::new(remote, "m", None, true, true).unwrap_err()),
            ErrorCode::OfflineMode
        );
        assert!(Api::new(remote, "m", None, true, false).is_ok());
        // This PC: no agreement needed, works offline.
        assert!(Api::new("http://localhost:11434/v1", "bge-m3", None, false, true).is_ok());
        assert!(Api::new("http://localhost:11434/v1", " ", None, false, true).is_err());
        // The key never shows in a debug print.
        let a = Api::new(
            "http://localhost/v1",
            "m",
            Some("s3cret".into()),
            false,
            false,
        )
        .unwrap();
        assert!(!format!("{a:?}").contains("s3cret"));
    }
}
