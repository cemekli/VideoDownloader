//! Search by meaning: the embedding provider and the index jobs (SPEC §16.3,
//! DEC-12, AC-24; STATE.md F7.1; ADR-0035).
//!
//! - The provider is llama.cpp's `llama-server` in embedding mode, a
//!   managed tool like whisper-cli, with a GGUF model from
//!   `tools/embedding-models.json`. llama.cpp no longer ships a one-shot
//!   embedding command, so a job starts one server ([`Server`]): it listens
//!   on 127.0.0.1 only, on a free port, and answers only requests that carry
//!   a random key made for that run. The model is loaded once per job; each
//!   batch is one request. The server is killed with its process tree when
//!   the job ends, is canceled or the app exits (job object, ADR-0008).
//! - Vectors are cut to the catalog's `dims` (the model is Matryoshka
//!   trained) and normalized, then stored by `vv_persistence::vectors`.
//! - [`request_index`] starts a build or fills the active index. A build
//!   for a new model runs beside the active index and replaces it only when
//!   complete; cancel drops the build and leaves the active index as it was
//!   (AC-24). Pause keeps the vectors made so far; the next run goes on.
//! - The jobs share the one heavy-AI slot with transcription (§13.3) and run
//!   below normal priority with the user's processor limit (F6.6).

use crate::context::{Context, Store};
use crate::models::{self, CatalogModel, EmbeddingUse};
use crate::proc::spawn_error_code;
use crate::resources;
use crate::worker::{Stop, Work};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use vv_domain::error::ErrorCode;
use vv_persistence::jobs::{self, Enqueued, JobError, JobKind, Lease, NewJob, Progress};
use vv_persistence::models::{self as rows, Installation, ModelStatus};
use vv_persistence::rusqlite::Connection;
use vv_persistence::vectors::{self, Generation, GenerationStatus, Producer};
use vv_process::{Managed, ManagedCommand};

/// Texts per request: a cancel waits at most one batch.
pub const BATCH: usize = 32;
/// Largest response accepted (32 vectors of 1024 floats as JSON are about
/// 1 MB).
const RESPONSE_MAX: u64 = 64 << 20;
/// Loading the model from disk; a slow disk or antivirus scan can take a while.
pub const START_TIMEOUT: Duration = Duration::from_secs(180);
/// One batch on a slow processor with a low limit.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// How often a waiting job looks at cancel and at the server process.
const POLL: Duration = Duration::from_millis(50);

/// `llama-server` as a job sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbedTool {
    pub exe: PathBuf,
    /// The pinned llama.cpp build.
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Query,
    Passage,
}

/// The text the model sees: the role's prefix, then the text with control
/// characters removed and spaces collapsed. An empty text becomes one space
/// (an empty input is refused by the server).
/// A chunk is at most `CHUNK_MAX_CHARS` (400) long; a longer text (a single
/// very long caption line) is cut so a prompt always fits the model context
/// (`llama-server` refuses an input longer than its batch).
pub const PROMPT_MAX_CHARS: usize = 1000;

pub fn prompt(u: &EmbeddingUse, role: Role, text: &str) -> String {
    let clean: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    let clean: String = clean.chars().take(PROMPT_MAX_CHARS).collect();
    let clean = if clean.is_empty() {
        " ".to_owned()
    } else {
        clean
    };
    match role {
        Role::Query => format!("{}{clean}", u.query_prefix),
        Role::Passage => format!("{}{clean}", u.passage_prefix),
    }
}

/// Arguments of the embedding server for one job: loopback only, the
/// given port and key, one slot, no web page, the model's own pooling and
/// L2-normalized output.
pub fn server_args(
    model: &Path,
    u: &EmbeddingUse,
    threads: usize,
    port: u16,
    key: &str,
) -> Vec<OsString> {
    let ctx = u.context.to_string();
    let mut a: Vec<OsString> = vec!["-m".into(), model.into()];
    for s in [
        "--embedding",
        "--embd-normalize",
        "2",
        "--host",
        "127.0.0.1",
        "--port",
        &port.to_string(),
        "--api-key",
        key,
        "-c",
        &ctx,
        "-b",
        &ctx,
        "-ub",
        &ctx,
        "-t",
        &threads.to_string(),
        "--parallel",
        "1",
        "--no-webui",
    ] {
        a.push(s.into());
    }
    a
}

#[derive(Deserialize)]
struct Out {
    data: Vec<OutItem>,
}

#[derive(Deserialize)]
struct OutItem {
    index: usize,
    embedding: Vec<f32>,
}

/// Reads the server's answer (`/v1/embeddings`): exactly `expected` vectors
/// of `native_dims`, in input order, cut to `dims` and normalized.
pub fn parse_output(
    body: &[u8],
    expected: usize,
    u: &EmbeddingUse,
) -> Result<Vec<Vec<f32>>, String> {
    let mut out: Out = serde_json::from_slice(body).map_err(|e| e.to_string())?;
    if out.data.len() != expected {
        return Err(format!("{} vectors for {expected} texts", out.data.len()));
    }
    out.data.sort_by_key(|d| d.index);
    let mut vs = Vec::with_capacity(expected);
    for (i, d) in out.data.into_iter().enumerate() {
        if d.index != i {
            return Err("vector indexes are not 0..n".into());
        }
        if d.embedding.len() != u.native_dims {
            return Err(format!("vector of {} values", d.embedding.len()));
        }
        let mut v = d.embedding;
        v.truncate(u.dims);
        vectors::normalize(&mut v).map_err(|e| e.to_string())?;
        vs.push(v);
    }
    Ok(vs)
}

/// Why one embedding run did not give vectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedError {
    /// Asked to stop (cancel, pause, lost lease).
    Stopped,
    /// The server stopped before it was ready (tried again once).
    Exited(String),
    Failed(ErrorCode, String),
}

/// A free port on 127.0.0.1. Another program could take it before the
/// server binds it; the server then exits and [`Server::start`] tries again.
fn free_port() -> std::io::Result<u16> {
    Ok(std::net::TcpListener::bind(("127.0.0.1", 0))?
        .local_addr()?
        .port())
}

fn last_line(log: &Path) -> String {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.chars().take(300).collect())
        .unwrap_or_default()
}

/// One embedding server, running until dropped.
pub struct Server {
    proc: Managed,
    base: String,
    key: String,
    agent: ureq::Agent,
    u: EmbeddingUse,
    log: PathBuf,
}

impl Server {
    /// Starts the server for `model` and waits until it has loaded it.
    /// Its log goes to `dir/server.log`.
    pub fn start(
        tool: &EmbedTool,
        model: &Path,
        u: &EmbeddingUse,
        threads: usize,
        dir: &Path,
        mut should_stop: impl FnMut() -> bool,
    ) -> Result<Server, EmbedError> {
        let mut last = EmbedError::Failed(ErrorCode::ExtractorFailed, String::new());
        for _ in 0..2 {
            match Self::start_once(tool, model, u, threads, dir, &mut should_stop) {
                Ok(s) => return Ok(s),
                // The port was taken meanwhile, or another start failure: once more.
                Err(EmbedError::Exited(m)) => {
                    last = EmbedError::Failed(ErrorCode::ExtractorFailed, m)
                }
                Err(e) => return Err(e),
            }
        }
        Err(last)
    }

    fn start_once(
        tool: &EmbedTool,
        model: &Path,
        u: &EmbeddingUse,
        threads: usize,
        dir: &Path,
        should_stop: &mut impl FnMut() -> bool,
    ) -> Result<Server, EmbedError> {
        let io = |e: std::io::Error| EmbedError::Failed(crate::storage::io_code(&e), e.to_string());
        let port = free_port().map_err(io)?;
        // Two random v4 UUIDs: 244 random bits, never written to disk.
        let key = format!("{}{}", new_id(), new_id()).replace('-', "");
        let log = dir.join("server.log");
        let out = std::fs::File::create(&log).map_err(io)?;
        let err = out.try_clone().map_err(io)?;
        let mut cmd = ManagedCommand::new(&tool.exe);
        cmd.args(server_args(model, u, threads, port, &key))
            .current_dir(dir)
            .stdout(Stdio::from(out))
            .stderr(Stdio::from(err))
            .low_priority();
        let proc = cmd
            .spawn()
            .map_err(|e| EmbedError::Failed(spawn_error_code(&e), e.to_string()))?;
        let mut s = Server {
            proc,
            base: format!("http://127.0.0.1:{port}"),
            key,
            // No proxy: ureq reads none unless asked, and this is loopback.
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(2))
                .timeout(REQUEST_TIMEOUT)
                .redirects(0)
                .build(),
            u: u.clone(),
            log,
        };
        let deadline = Instant::now() + START_TIMEOUT;
        loop {
            if should_stop() {
                return Err(EmbedError::Stopped);
            }
            if let Some(code) = s.exited() {
                return Err(EmbedError::Exited(format!(
                    "exit {code}: {}",
                    last_line(&s.log)
                )));
            }
            let r = s
                .agent
                .get(&format!("{}/health", s.base))
                .set("Authorization", &format!("Bearer {}", s.key))
                .timeout(Duration::from_secs(2))
                .call();
            if matches!(&r, Ok(r) if r.status() == 200) {
                return Ok(s);
            }
            if Instant::now() >= deadline {
                return Err(EmbedError::Failed(
                    ErrorCode::ExtractorFailed,
                    "the embedding server did not start".into(),
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// The exit status once the server has stopped by itself.
    fn exited(&mut self) -> Option<String> {
        match self.proc.try_wait() {
            Ok(Some(st)) => Some(st.to_string()),
            Ok(None) => None,
            Err(e) => Some(e.to_string()),
        }
    }

    /// Embeds `texts` with one request. A stop request or the server dying
    /// ends the wait at once (the request thread ends when the server is
    /// killed).
    pub fn embed(
        &mut self,
        role: Role,
        texts: &[&str],
        mut should_stop: impl FnMut() -> bool,
    ) -> Result<Vec<Vec<f32>>, EmbedError> {
        let input: Vec<String> = texts.iter().map(|t| prompt(&self.u, role, t)).collect();
        let body = serde_json::json!({ "input": input, "encoding_format": "float" }).to_string();
        let req = self
            .agent
            .post(&format!("{}/v1/embeddings", self.base))
            .set("Authorization", &format!("Bearer {}", self.key))
            .set("Content-Type", "application/json");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let r = match req.send_string(&body) {
                Ok(resp) => {
                    let mut buf = Vec::new();
                    resp.into_reader()
                        .take(RESPONSE_MAX + 1)
                        .read_to_end(&mut buf)
                        .map(|_| buf)
                        .map_err(|e| e.to_string())
                }
                Err(ureq::Error::Status(code, resp)) => {
                    let t: String = resp
                        .into_string()
                        .unwrap_or_default()
                        .chars()
                        .take(300)
                        .collect();
                    Err(format!("HTTP {code}: {t}"))
                }
                Err(e) => Err(e.to_string()),
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
                    return parse_output(&buf, texts.len(), &self.u)
                        .map_err(|m| EmbedError::Failed(ErrorCode::ExtractorFailed, m))
                }
                Ok(Err(m)) => {
                    let m = match self.exited() {
                        Some(code) => format!("server exit {code}: {}", last_line(&self.log)),
                        None => m,
                    };
                    return Err(EmbedError::Failed(ErrorCode::ExtractorFailed, m));
                }
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
                    if let Some(code) = self.exited() {
                        return Err(EmbedError::Failed(
                            ErrorCode::ExtractorFailed,
                            format!("server exit {code}: {}", last_line(&self.log)),
                        ));
                    }
                }
            }
        }
    }
}

/// Starts a server, embeds `texts` and stops it (a search query; tests).
#[allow(clippy::too_many_arguments)]
pub fn embed(
    tool: &EmbedTool,
    model: &Path,
    u: &EmbeddingUse,
    role: Role,
    texts: &[&str],
    threads: usize,
    dir: &Path,
    mut should_stop: impl FnMut() -> bool,
) -> Result<Vec<Vec<f32>>, EmbedError> {
    let mut s = Server::start(tool, model, u, threads, dir, &mut should_stop)?;
    s.embed(role, texts, should_stop)
}

// ------------------------------------------------------------------ index

/// The installed embedding model searches would use: the first catalog
/// entry that is installed, verified and pinned as on record.
pub fn usable_model(
    c: &Connection,
    models_dir: &Path,
) -> Option<(&'static CatalogModel, Installation)> {
    let cat = models::embedding_catalog();
    cat.models.iter().find_map(|m| {
        let spec = cat.spec(m)?;
        let inst = rows::get(c, &m.id).ok().flatten()?;
        let ok = inst.status == ModelStatus::Ready
            && inst.matches(&spec)
            && models::model_path(models_dir, &inst).is_file();
        ok.then_some((m, inst))
    })
}

/// What the vectors of `m` are made with (§16.3).
pub fn producer(m: &CatalogModel, inst: &Installation) -> Option<Producer> {
    let u = m.embedding.as_ref()?;
    Some(Producer {
        provider: models::EMBEDDING_PROVIDER.into(),
        model_id: m.id.clone(),
        model_version: inst.spec.sha256.clone(),
        dims: u.dims,
    })
}

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
    /// No embedding model is installed and verified.
    NoModel,
    Db,
}

fn new_id() -> String {
    vv_persistence::library::new_id()
}

/// Makes the index match the installed model: with an active index of this
/// model, queues the chunks it is missing; otherwise starts a build beside
/// it (`rebuild` forces one even when the active index is current). Returns
/// the job, `None` when nothing needs doing.
pub fn request_index(
    c: &mut Connection,
    models_dir: &Path,
    rebuild: bool,
    now: i64,
) -> Result<Option<Enqueued>, IndexError> {
    let (m, inst) = usable_model(c, models_dir).ok_or(IndexError::NoModel)?;
    let p = producer(m, &inst).ok_or(IndexError::NoModel)?;
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
/// index of the installed model is active. Never starts a build.
pub fn after_chunks_changed(c: &mut Connection, models_dir: &Path, now: i64) -> Option<Enqueued> {
    let active = vectors::active(c).ok().flatten()?;
    let (m, inst) = usable_model(c, models_dir)?;
    if producer(m, &inst)? != active.producer {
        return None;
    }
    request_index(c, models_dir, false, now).ok().flatten()
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

pub fn staging_dir(staging: &Path, job_id: &str) -> PathBuf {
    staging.join(format!("embed-{job_id}"))
}

/// The `embedding` and `reindex` jobs: vectors for every chunk the
/// generation is missing, then (a build) the swap.
pub fn run<S: Store>(cx: &Context<S>, lease: Lease) {
    let w = Work::new(cx, lease);
    let Ok(p) = serde_json::from_value::<Payload>(w.lease.job.payload.clone()) else {
        w.end_error(ErrorCode::InternalError, Some("index payload unreadable"));
        return;
    };
    let dir = staging_dir(&cx.dirs.staging, w.id());
    let lost = run_in(cx, &w, &p, &dir);
    if !lost {
        let _ = std::fs::remove_dir_all(&dir);
    }
    cx.events.job_changed(w.id(), None);
}

/// `true` when the lease was lost (another run goes on).
fn run_in<S: Store>(cx: &Context<S>, w: &Work<'_, S>, p: &Payload, dir: &Path) -> bool {
    let g = cx
        .store
        .read(|c| vectors::get(c, &p.generation_id).ok().flatten());
    let Some(g) = g else {
        // Canceled or replaced meanwhile: nothing left to do.
        succeed(cx, w, None);
        return false;
    };
    let found = cx.store.read(|c| usable_model(c, &cx.dirs.models));
    let Some((m, inst)) = found else {
        w.end_error(ErrorCode::ModelMissing, None);
        return false;
    };
    let (Some(u), Some(prod)) = (m.embedding.clone(), producer(m, &inst)) else {
        w.end_error(ErrorCode::ModelMissing, None);
        return false;
    };
    if prod != g.producer {
        // The installed model is not the one this index is made of.
        w.end_error(
            ErrorCode::ModelMissing,
            Some("the index is for another model file"),
        );
        return false;
    }
    let Some(tool) = cx.tools().embedder else {
        w.end_error(ErrorCode::ToolMissing, None);
        return false;
    };
    if std::fs::create_dir_all(dir).is_err() {
        w.end_error(ErrorCode::PathNotWritable, None);
        return false;
    }
    let cpu = cx
        .store
        .read(|c| vv_persistence::settings::get(c).map(|s| s.transcription_cpu))
        .unwrap_or_default();
    let threads = resources::threads(cpu, resources::cores());
    let model_path = models::model_path(&cx.dirs.models, &inst);
    // Started on the first batch: nothing to embed, no model load.
    let mut server: Option<Server> = None;
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
        if server.is_none() {
            match Server::start(&tool, &model_path, &u, threads, dir, || w.should_stop()) {
                Ok(s) => server = Some(s),
                Err(EmbedError::Stopped) => return stop(cx, w, &g),
                Err(EmbedError::Failed(code, _)) => {
                    w.end_error(code, Some("the embedding server did not start"));
                    return false;
                }
                Err(EmbedError::Exited(_)) => {
                    w.end_error(
                        ErrorCode::ExtractorFailed,
                        Some("the embedding server did not start"),
                    );
                    return false;
                }
            }
        }
        let Some(srv) = server.as_mut() else {
            return false;
        };
        let vs = match srv.embed(Role::Passage, &texts, || w.should_stop()) {
            Ok(v) => v,
            Err(EmbedError::Stopped) => {
                drop(server.take());
                return stop(cx, w, &g);
            }
            Err(EmbedError::Failed(code, _)) => {
                w.end_error(code, Some("the embedding tool failed"));
                return false;
            }
            Err(EmbedError::Exited(_)) => {
                w.end_error(
                    ErrorCode::ExtractorFailed,
                    Some("the embedding tool failed"),
                );
                return false;
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
            return false;
        }
    }
    drop(server);
    succeed(
        cx,
        w,
        (g.status == GenerationStatus::Building).then_some(&g),
    );
    // Chunks written after the last check: their own job picks them up.
    let models_dir = cx.dirs.models.clone();
    let now = cx.now();
    cx.store.write(|c| {
        let _ = after_chunks_changed(c, &models_dir, now);
    });
    false
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

fn stop<S: Store>(cx: &Context<S>, w: &Work<'_, S>, g: &Generation) -> bool {
    let s = w.stopped().unwrap_or(Stop::Cancel);
    if s == Stop::Cancel && g.status == GenerationStatus::Building {
        cx.store.write(|c| {
            let _ = vectors::drop_generation(c, &g.id);
        });
    }
    w.end_stopped(s);
    s == Stop::Lost
}

#[cfg(test)]
mod tests {
    use super::*;

    fn use_() -> EmbeddingUse {
        EmbeddingUse {
            native_dims: 4,
            dims: 2,
            query_prefix: "Q:".into(),
            passage_prefix: String::new(),
            context: 512,
        }
    }

    #[test]
    fn prompts_are_clean_and_bounded() {
        let u = use_();
        assert_eq!(prompt(&u, Role::Query, "a\nb"), "Q:a b");
        assert_eq!(prompt(&u, Role::Passage, "x\u{0}y"), "x y");
        assert_eq!(prompt(&u, Role::Passage, "\n\t"), " ");
        assert_eq!(
            prompt(&u, Role::Passage, "İstanbul'da  çay"),
            "İstanbul'da çay"
        );
        let long = "ş".repeat(PROMPT_MAX_CHARS + 50);
        assert_eq!(
            prompt(&u, Role::Passage, &long).chars().count(),
            PROMPT_MAX_CHARS
        );
    }

    #[test]
    fn output_is_cut_normalized_and_in_order() {
        let u = use_();
        let json = br#"{"object":"list","data":[
 {"object":"embedding","index":1,"embedding":[0,2,9,9]},
 {"object":"embedding","index":0,"embedding":[3,4,1,1]}]}
"#;
        let v = parse_output(json, 2, &u).unwrap();
        assert_eq!(v[0], vec![0.6, 0.8]);
        assert_eq!(v[1], vec![0.0, 1.0]);
        assert!(parse_output(json, 3, &u).is_err());
        let short = br#"{"data":[{"index":0,"embedding":[1,2]}]}"#;
        assert!(parse_output(short, 1, &u).is_err());
        let zero = br#"{"data":[{"index":0,"embedding":[0,0,1,1]}]}"#;
        assert!(parse_output(zero, 1, &u).is_err());
        assert!(parse_output(b"error: model not found", 1, &u).is_err());
    }

    #[test]
    fn server_listens_on_loopback_with_a_key_and_no_web_page() {
        let a = server_args(Path::new("/m dir/model.gguf"), &use_(), 3, 41234, "k3y");
        assert_eq!(a[1], OsString::from("/m dir/model.gguf"));
        let s: Vec<String> = a.iter().map(|x| x.to_string_lossy().into_owned()).collect();
        let pair = |f: &str, v: &str| s.windows(2).any(|w| w[0] == f && w[1] == v);
        assert!(pair("--host", "127.0.0.1"));
        assert!(pair("--port", "41234"));
        assert!(pair("--api-key", "k3y"));
        assert!(pair("-t", "3"));
        assert!(pair("--parallel", "1"));
        assert!(s.contains(&"--embedding".to_owned()));
        assert!(s.contains(&"--no-webui".to_owned()));
        // Never a model download or a public address.
        assert!(!s.iter().any(|x| x.starts_with("-hf") || x == "0.0.0.0"));
    }
}
