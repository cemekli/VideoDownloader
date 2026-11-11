//! Transcripts of an item: list them, read one with its segments, get the
//! site's captions, transcribe the item's file on this PC, delete one
//! (STATE.md F6.2; SPEC AI-A01, AI-A02, §8.4, §11.5; ADR-0027, ADR-0028).
//!
//! The webview sees the text, times, language and where a transcript came
//! from (site captions, manual or automatic; later the local model). It
//! never sees a path or a caption URL.

use crate::download_commands::{job_dto, JobDto};
use crate::engine::EngineState;
use crate::library::{internal, LibraryState};
use crate::library_commands::blocking;
use serde::Serialize;
use std::path::Path;
use tauri::{AppHandle, Manager, Runtime};
use vv_domain::error::{DomainError, ErrorCode};
use vv_engine::{captions, models, transcribe, Dirs};
use vv_persistence::jobs::{self, JobKind};
use vv_persistence::library::repo;
use vv_persistence::library::{Clock, SystemClock};
use vv_persistence::rusqlite::Connection;
use vv_persistence::settings;
use vv_persistence::transcripts::{self, Transcript, TranscriptError};

/// Most segments returned at once; a long lecture has a few thousand.
pub const MAX_SEGMENTS: usize = 20_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptDto {
    pub id: String,
    pub item_id: String,
    pub language: String,
    /// `source_captions`, `generated_local` or `imported`.
    pub provenance: String,
    pub provider: String,
    /// `manual` or `automatic` for site captions.
    pub caption_kind: Option<String>,
    pub model_id: Option<String>,
    /// `ready`, `partial`, ... (§11.5).
    pub status: String,
    pub covered_ms: Option<i64>,
    pub segment_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentDto {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptDetail {
    pub transcript: TranscriptDto,
    pub segments: Vec<SegmentDto>,
    /// More segments exist than [`MAX_SEGMENTS`].
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemTranscripts {
    pub item_id: String,
    pub transcripts: Vec<TranscriptDto>,
    /// The item's last `fetch_captions` job, if any.
    pub captions_job: Option<JobDto>,
    /// The item has a link to fetch captions from.
    pub can_fetch_captions: bool,
    /// The item's last local `transcription` job, if any.
    pub transcribe_job: Option<JobDto>,
    /// The label of the model that job uses (`Base`, `Medium`, ...).
    pub transcribe_model: Option<String>,
    /// How long that job has run (to now while open, to its end once done),
    /// from when a worker took it; `None` while it waits in the queue.
    pub transcribe_elapsed_ms: Option<i64>,
    /// Whether "Transcribe on this PC" can run, and with what.
    pub local: LocalTranscription,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalTranscription {
    /// Everything is there: a media file, a ready model and whisper-cli.
    pub ready: bool,
    /// Why not: `no-media` (download it first), `no-model` (Settings →
    /// Transcription), `no-tool` (whisper-cli missing or failed its check).
    pub reason: Option<&'static str>,
    /// Models the user agreed to and that passed their check.
    pub models: Vec<ModelChoice>,
    /// `auto`, `tr`, `en`.
    pub languages: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelChoice {
    pub id: String,
    pub label: String,
}

fn now() -> i64 {
    SystemClock.now_ms()
}

fn dirs(st: &LibraryState) -> Result<Dirs, DomainError> {
    Ok(Dirs::under(st.get()?.data_dir()))
}

fn wake<R: Runtime>(app: &AppHandle<R>) {
    if let Some(e) = app.try_state::<EngineState>() {
        e.wake();
    }
}

fn db_err(e: TranscriptError) -> DomainError {
    match e {
        TranscriptError::NotFound => {
            DomainError::new(ErrorCode::InputRejected, "transcript-not-found")
        }
        _ => internal("transcript-db"),
    }
}

pub fn dto(t: &Transcript) -> TranscriptDto {
    TranscriptDto {
        id: t.id.clone(),
        item_id: t.origin.item_id.clone(),
        language: t.origin.language.clone(),
        provenance: t.origin.provenance.as_str().to_owned(),
        provider: t.origin.provider.clone(),
        caption_kind: t.origin.caption_kind.map(|k| k.as_str().to_owned()),
        model_id: t.origin.model_id.clone(),
        status: t.status.as_str().to_owned(),
        covered_ms: t.covered_ms,
        segment_count: t.segment_count,
        created_at: t.created_at,
        updated_at: t.updated_at,
    }
}

fn last_raw_job(c: &Connection, item_id: &str, kind: JobKind) -> Option<jobs::Job> {
    jobs::list(c, &[], 500)
        .ok()?
        .into_iter()
        .filter(|j| j.kind == kind && j.item_id.as_deref() == Some(item_id))
        .max_by_key(|j| (j.created_at, j.id.clone()))
}

fn last_job(c: &Connection, staging: &Path, item_id: &str, kind: JobKind) -> Option<JobDto> {
    last_raw_job(c, item_id, kind).map(|j| job_dto(c, staging, &j))
}

/// Run time of a job from its latest attempt's start to its end (or `now`).
fn job_elapsed_ms(c: &Connection, job: &jobs::Job, now: i64) -> Option<i64> {
    let started = jobs::last_attempt_started_at(c, &job.id).ok().flatten()?;
    Some((job.finished_at.unwrap_or(now) - started).max(0))
}

/// The catalog label of the model a transcription job runs with.
fn job_model_label(job: &jobs::Job) -> Option<String> {
    let p = serde_json::from_value::<transcribe::Payload>(job.payload.clone()).ok()?;
    let cat = models::catalog();
    Some(
        cat.models
            .iter()
            .find(|m| m.id == p.model_id)
            .map_or(p.model_id, |m| m.label.clone()),
    )
}

/// whisper-cli passed its health check (ADR-0028).
fn whisper_ready<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<EngineState>()
        .and_then(|e| e.report())
        .is_some_and(|r| r.is_ok(vv_tools::ToolId::WhisperCli))
}

/// Ready models, smallest first (the catalog's order).
fn usable_models(c: &Connection, dirs: &Dirs) -> Vec<ModelChoice> {
    models::catalog()
        .models
        .iter()
        .filter(|m| transcribe::usable_model(c, &dirs.models, &m.id).is_some())
        .map(|m| ModelChoice {
            id: m.id.clone(),
            label: m.label.clone(),
        })
        .collect()
}

fn local(c: &Connection, dirs: &Dirs, item_id: &str, whisper: bool) -> LocalTranscription {
    let models = usable_models(c, dirs);
    let reason = if transcribe::source_asset(c, item_id).is_none() {
        Some("no-media")
    } else if models.is_empty() {
        Some("no-model")
    } else if !whisper {
        Some("no-tool")
    } else {
        None
    };
    LocalTranscription {
        ready: reason.is_none(),
        reason,
        models,
        languages: transcribe::LANGUAGES.to_vec(),
    }
}

pub fn for_item(
    c: &Connection,
    dirs: &Dirs,
    item_id: &str,
    whisper: bool,
) -> Result<ItemTranscripts, DomainError> {
    let it = repo::get_item(c, item_id)
        .map_err(|_| internal("item-read"))?
        .ok_or_else(|| DomainError::new(ErrorCode::InputRejected, "item-not-found"))?;
    let transcripts = transcripts::list_for_item(c, item_id)
        .map_err(db_err)?
        .iter()
        .map(dto)
        .collect();
    let asr = last_raw_job(c, item_id, JobKind::Transcription);
    Ok(ItemTranscripts {
        item_id: item_id.to_owned(),
        transcripts,
        captions_job: last_job(c, &dirs.staging, item_id, JobKind::FetchCaptions),
        can_fetch_captions: it.canonical_url.is_some() && it.removed_at.is_none(),
        transcribe_job: asr.as_ref().map(|j| job_dto(c, &dirs.staging, j)),
        transcribe_model: asr.as_ref().and_then(job_model_label),
        transcribe_elapsed_ms: asr.as_ref().and_then(|j| job_elapsed_ms(c, j, now())),
        local: local(c, dirs, item_id, whisper && it.removed_at.is_none()),
    })
}

#[tauri::command]
pub async fn list_transcripts<R: Runtime>(
    app: AppHandle<R>,
    item_id: String,
) -> Result<ItemTranscripts, DomainError> {
    let whisper = whisper_ready(&app);
    blocking(app, move |st| {
        let d = dirs(st)?;
        for_item(&st.get()?.reader()?, &d, &item_id, whisper)
    })
    .await
}

#[tauri::command]
pub async fn get_transcript<R: Runtime>(
    app: AppHandle<R>,
    transcript_id: String,
) -> Result<TranscriptDetail, DomainError> {
    blocking(app, move |st| {
        let c = st.get()?.reader()?;
        let t = transcripts::get(&c, &transcript_id)
            .map_err(db_err)?
            .ok_or_else(|| DomainError::new(ErrorCode::InputRejected, "transcript-not-found"))?;
        let all = transcripts::segments(&c, &t.id).map_err(db_err)?;
        let truncated = all.len() > MAX_SEGMENTS;
        let segments = all
            .into_iter()
            .take(MAX_SEGMENTS)
            .map(|s| SegmentDto {
                start_ms: s.start_ms,
                end_ms: s.end_ms,
                text: s.text,
            })
            .collect();
        Ok(TranscriptDetail {
            transcript: dto(&t),
            segments,
            truncated,
        })
    })
    .await
}

/// Queues the site-captions job for an item (AI-A01). One open job per
/// item; asking again while it runs returns the same job.
#[tauri::command]
pub async fn fetch_captions<R: Runtime>(
    app: AppHandle<R>,
    item_id: String,
) -> Result<ItemTranscripts, DomainError> {
    let a = app.clone();
    let whisper = whisper_ready(&app);
    let r = blocking(app, move |st| {
        let d = dirs(st)?;
        st.get()?.write(|c| {
            let view = for_item(c, &d, &item_id, whisper)?;
            if !view.can_fetch_captions {
                return Err(DomainError::new(
                    ErrorCode::InputRejected,
                    "item-has-no-link",
                ));
            }
            if settings::offline_mode(c).unwrap_or(false) {
                return Err(DomainError::new(ErrorCode::OfflineMode, "captions-offline"));
            }
            captions::enqueue(c, &item_id, now()).map_err(|_| internal("captions-job"))?;
            for_item(c, &d, &item_id, whisper)
        })
    })
    .await;
    wake(&a);
    r
}

/// Queues a local transcription of the item's media file with a model the
/// user agreed to (AI-A02, ADR-0028). Works offline. One open job per item,
/// model and language; asking again while it runs returns the same job.
#[tauri::command]
pub async fn transcribe_locally<R: Runtime>(
    app: AppHandle<R>,
    item_id: String,
    model_id: String,
    language: String,
) -> Result<ItemTranscripts, DomainError> {
    let a = app.clone();
    let whisper = whisper_ready(&app);
    let r = blocking(app, move |st| {
        let d = dirs(st)?;
        st.get()?.write(|c| {
            if !transcribe::LANGUAGES.contains(&language.as_str()) {
                return Err(DomainError::new(
                    ErrorCode::InputRejected,
                    "language-not-offered",
                ));
            }
            let view = for_item(c, &d, &item_id, whisper)?;
            match view.local.reason {
                Some("no-media") => {
                    return Err(DomainError::new(
                        ErrorCode::FileMissing,
                        "item-has-no-media",
                    ))
                }
                Some("no-tool") => {
                    return Err(DomainError::new(
                        ErrorCode::ToolMissing,
                        "whisper-not-ready",
                    ))
                }
                _ => {}
            }
            if !view.local.models.iter().any(|m| m.id == model_id) {
                return Err(DomainError::new(ErrorCode::ModelMissing, "model-not-ready"));
            }
            transcribe::enqueue(c, &item_id, &model_id, &language, now())
                .map_err(|_| internal("transcription-job"))?;
            for_item(c, &d, &item_id, whisper)
        })
    })
    .await;
    wake(&a);
    r
}

/// Deletes one transcript with its segments and search rows. The media and
/// the item stay (§26.2).
#[tauri::command]
pub async fn delete_transcript<R: Runtime>(
    app: AppHandle<R>,
    transcript_id: String,
) -> Result<ItemTranscripts, DomainError> {
    let whisper = whisper_ready(&app);
    blocking(app, move |st| {
        let d = dirs(st)?;
        st.get()?.write(|c| {
            let t = transcripts::get(c, &transcript_id)
                .map_err(db_err)?
                .ok_or_else(|| {
                    DomainError::new(ErrorCode::InputRejected, "transcript-not-found")
                })?;
            transcripts::delete(c, &t.id).map_err(db_err)?;
            for_item(c, &d, &t.origin.item_id, whisper)
        })
    })
    .await
}
