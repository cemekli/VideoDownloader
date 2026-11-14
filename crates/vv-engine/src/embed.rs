//! Search by meaning: the embedding provider and the index jobs (SPEC §16.3,
//! DEC-12, AC-24; STATE.md F7.1; ADR-0035).
//!
//! - The provider is llama.cpp's `llama-embedding`, a managed tool like
//!   whisper-cli, with a GGUF model from `tools/embedding-models.json`. One
//!   run embeds a batch of texts: they go in a file, joined by a separator
//!   that cannot occur in them, and come back as JSON on stdout.
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
use crate::proc::{run_streaming, spawn_error_code, StdoutMode};
use crate::resources;
use crate::worker::{Stop, Work};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use vv_domain::error::ErrorCode;
use vv_persistence::jobs::{self, Enqueued, JobError, JobKind, Lease, NewJob, Progress};
use vv_persistence::models::{self as rows, Installation, ModelStatus};
use vv_persistence::rusqlite::Connection;
use vv_persistence::vectors::{self, Generation, GenerationStatus, Producer};
use vv_process::ManagedCommand;
use vv_ytdlp::outcome::Exit;

/// Between texts in the prompt file. Removed from the texts first.
pub const SEPARATOR: &str = "<#vv-sep#>";
/// Texts per tool run: one model load per batch, and a cancel waits at most
/// one batch.
pub const BATCH: usize = 32;
/// Largest stdout accepted from one run (32 vectors of 1024 floats as JSON
/// are about 1 MB).
const STDOUT_MAX: usize = 64 << 20;

/// `llama-embedding` as a job sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbedTool {
    pub exe: PathBuf,
    /// The pinned llama.cpp version.
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Query,
    Passage,
}

/// The text the model sees: the role's prefix, then the text with the
/// separator and control characters removed. An empty text becomes one
/// space (the tool would skip it and shift every vector after it).
pub fn prompt(u: &EmbeddingUse, role: Role, text: &str) -> String {
    let clean: String = text
        .replace(SEPARATOR, " ")
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
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

/// Arguments of one `llama-embedding` run over `prompts` (a file).
pub fn args(model: &Path, prompts: &Path, u: &EmbeddingUse, threads: usize) -> Vec<OsString> {
    let ctx = u.context.to_string();
    let mut a: Vec<OsString> = vec!["-m".into(), model.into(), "-f".into(), prompts.into()];
    for s in [
        "--embd-separator",
        SEPARATOR,
        "--embd-output-format",
        "json",
        "--embd-normalize",
        "2",
        "-c",
        &ctx,
        "-b",
        &ctx,
        "-ub",
        &ctx,
        "-t",
        &threads.to_string(),
        "--no-warmup",
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

/// Reads the tool's JSON: exactly `expected` vectors of `native_dims`, in
/// prompt order, cut to `dims` and normalized.
pub fn parse_output(
    stdout: &[u8],
    expected: usize,
    u: &EmbeddingUse,
) -> Result<Vec<Vec<f32>>, String> {
    let text = String::from_utf8_lossy(stdout);
    let start = text.find('{').ok_or("no JSON on stdout")?;
    let body = text.get(start..).ok_or("no JSON on stdout")?;
    let mut out: Out = serde_json::from_str(body.trim_end()).map_err(|e| e.to_string())?;
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
    Failed(ErrorCode, String),
}

/// Embeds `texts` with one run of the tool. `dir` holds the prompt file.
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
    let file = dir.join("prompts.txt");
    let body = texts
        .iter()
        .map(|t| prompt(u, role, t))
        .collect::<Vec<_>>()
        .join(SEPARATOR);
    std::fs::write(&file, body)
        .map_err(|e| EmbedError::Failed(crate::storage::io_code(&e), "prompt file".into()))?;
    let mut cmd = ManagedCommand::new(&tool.exe);
    cmd.args(args(model, &file, u, threads))
        .current_dir(dir)
        .low_priority();
    let mut last_err = String::new();
    let r = run_streaming(
        cmd,
        StdoutMode::Raw(STDOUT_MAX),
        None,
        |_, l| {
            if !l.trim().is_empty() {
                last_err = l.chars().take(300).collect();
            }
        },
        &mut should_stop,
    );
    let _ = std::fs::remove_file(&file);
    let s = match r {
        Err(e) => return Err(EmbedError::Failed(spawn_error_code(&e), e.to_string())),
        Ok(s) if s.stopped => return Err(EmbedError::Stopped),
        Ok(s) => s,
    };
    if s.exit != Exit::Code(0) || s.stdout_truncated {
        return Err(EmbedError::Failed(ErrorCode::ExtractorFailed, last_err));
    }
    parse_output(&s.stdout, texts.len(), u)
        .map_err(|m| EmbedError::Failed(ErrorCode::ExtractorFailed, m))
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
        let vs = match embed(
            &tool,
            &model_path,
            &u,
            Role::Passage,
            &texts,
            threads,
            dir,
            || w.should_stop(),
        ) {
            Ok(v) => v,
            Err(EmbedError::Stopped) => return stop(cx, w, &g),
            Err(EmbedError::Failed(code, _)) => {
                w.end_error(code, Some("the embedding tool failed"));
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
    fn prompts_cannot_break_the_batch() {
        let u = use_();
        assert_eq!(prompt(&u, Role::Query, "a\nb"), "Q:a b");
        assert_eq!(prompt(&u, Role::Passage, &format!("x{SEPARATOR}y")), "x y");
        assert_eq!(prompt(&u, Role::Passage, "\n\t"), " ");
        assert_eq!(
            prompt(&u, Role::Passage, "İstanbul'da  çay"),
            "İstanbul'da çay"
        );
    }

    #[test]
    fn output_is_cut_normalized_and_in_order() {
        let u = use_();
        let json = br#"llama log line
{"object":"list","data":[
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
    fn argv_keeps_paths_whole() {
        let a = args(
            Path::new("/m dir/model.gguf"),
            Path::new("/s dir/p.txt"),
            &use_(),
            3,
        );
        assert_eq!(a[1], OsString::from("/m dir/model.gguf"));
        assert_eq!(a[3], OsString::from("/s dir/p.txt"));
        let s: Vec<String> = a.iter().map(|x| x.to_string_lossy().into_owned()).collect();
        assert!(s.windows(2).any(|w| w[0] == "-t" && w[1] == "3"));
        assert!(s
            .windows(2)
            .any(|w| w[0] == "--embd-separator" && w[1] == SEPARATOR));
    }
}
