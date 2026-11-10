//! Transcripts of an item: list them, read one with its segments, get the
//! site's captions, delete one (STATE.md F6.2; SPEC AI-A01, §8.4, §11.5;
//! ADR-0027).
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
use vv_engine::{captions, Dirs};
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

fn last_captions_job(c: &Connection, staging: &Path, item_id: &str) -> Option<JobDto> {
    jobs::list(c, &[], 500)
        .ok()?
        .into_iter()
        .filter(|j| j.kind == JobKind::FetchCaptions && j.item_id.as_deref() == Some(item_id))
        .max_by_key(|j| (j.created_at, j.id.clone()))
        .map(|j| job_dto(c, staging, &j))
}

pub fn for_item(
    c: &Connection,
    dirs: &Dirs,
    item_id: &str,
) -> Result<ItemTranscripts, DomainError> {
    let it = repo::get_item(c, item_id)
        .map_err(|_| internal("item-read"))?
        .ok_or_else(|| DomainError::new(ErrorCode::InputRejected, "item-not-found"))?;
    let transcripts = transcripts::list_for_item(c, item_id)
        .map_err(db_err)?
        .iter()
        .map(dto)
        .collect();
    Ok(ItemTranscripts {
        item_id: item_id.to_owned(),
        transcripts,
        captions_job: last_captions_job(c, &dirs.staging, item_id),
        can_fetch_captions: it.canonical_url.is_some() && it.removed_at.is_none(),
    })
}

#[tauri::command]
pub async fn list_transcripts<R: Runtime>(
    app: AppHandle<R>,
    item_id: String,
) -> Result<ItemTranscripts, DomainError> {
    blocking(app, move |st| {
        let d = dirs(st)?;
        for_item(&st.get()?.reader()?, &d, &item_id)
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
    let r = blocking(app, move |st| {
        let d = dirs(st)?;
        st.get()?.write(|c| {
            let view = for_item(c, &d, &item_id)?;
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
            for_item(c, &d, &item_id)
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
    blocking(app, move |st| {
        let d = dirs(st)?;
        st.get()?.write(|c| {
            let t = transcripts::get(c, &transcript_id)
                .map_err(db_err)?
                .ok_or_else(|| {
                    DomainError::new(ErrorCode::InputRejected, "transcript-not-found")
                })?;
            transcripts::delete(c, &t.id).map_err(db_err)?;
            for_item(c, &d, &t.origin.item_id)
        })
    })
    .await
}
