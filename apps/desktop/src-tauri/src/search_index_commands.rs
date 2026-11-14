//! Search by meaning: the embedding model and the index as Settings and AI
//! Processing show them, building, rebuilding and canceling the index
//! (STATE.md F7.1; SPEC §16.3, AC-24; ADR-0035).
//!
//! The model is downloaded through `download_model` like a transcription
//! model (same consent dialog). The webview never sees a path or a vector.

use crate::download_commands::{job_dto, JobDto};
use crate::engine::EngineState;
use crate::library::LibraryState;
use crate::library_commands::blocking;
use crate::model_commands::ModelDto;
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};
use vv_domain::error::{DomainError, ErrorCode};
use vv_engine::embed::{self, IndexError};
use vv_engine::models;
use vv_engine::{resources, Dirs};
use vv_persistence::jobs::{self, JobKind};
use vv_persistence::library::{Clock, SystemClock};
use vv_persistence::rusqlite::Connection;
use vv_persistence::vectors;

fn now() -> i64 {
    SystemClock.now_ms()
}

/// One index generation (§16.3) as the screen shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexDto {
    pub model_id: String,
    /// The model's label when it is still in the catalog.
    pub model_label: Option<String>,
    pub dims: usize,
    /// Chunks with a vector / transcript chunks in the library.
    pub done: i64,
    pub total: i64,
    pub created_at: i64,
    pub activated_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchIndexDto {
    /// The embedding models of the catalog (one for now).
    pub models: Vec<ModelDto>,
    /// The index searches use, if any.
    pub active: Option<IndexDto>,
    /// A build under way beside it (AC-24).
    pub building: Option<IndexDto>,
    /// The open or last index job.
    pub job: Option<JobDto>,
    /// `llama-embedding` passed its check.
    pub tool_ready: bool,
    /// The active index was made with the installed model, so searches by
    /// meaning can run (the query needs the same model).
    pub searchable: bool,
}

fn index_dto(c: &Connection, g: &vectors::Generation) -> IndexDto {
    let (done, total) = vectors::coverage(c, g.seq).unwrap_or((0, 0));
    IndexDto {
        model_id: g.producer.model_id.clone(),
        model_label: models::embedding_catalog()
            .model(&g.producer.model_id)
            .map(|m| m.label.clone()),
        dims: g.producer.dims,
        done,
        total,
        created_at: g.created_at,
        activated_at: g.activated_at,
    }
}

fn last_index_job(c: &Connection, staging: &std::path::Path) -> Option<JobDto> {
    jobs::list(c, &[], 500)
        .ok()?
        .into_iter()
        .filter(|j| matches!(j.kind, JobKind::Embedding | JobKind::Reindex))
        .max_by_key(|j| (!j.status.is_terminal(), j.created_at, j.id.clone()))
        .map(|j| job_dto(c, staging, &j))
}

pub fn search_index(c: &Connection, dirs: &Dirs, tool_ready: bool) -> SearchIndexDto {
    let cat = models::embedding_catalog();
    let mem = resources::memory();
    let active = vectors::active(c).ok().flatten();
    let usable = embed::usable_model(c, &dirs.models).and_then(|(m, i)| embed::producer(m, &i));
    let searchable = tool_ready
        && active
            .as_ref()
            .is_some_and(|a| usable.as_ref() == Some(&a.producer));
    SearchIndexDto {
        models: cat
            .models
            .iter()
            .map(|m| crate::model_commands::model_dto(c, dirs, cat, m, mem))
            .collect(),
        active: active.as_ref().map(|g| index_dto(c, g)),
        building: vectors::building(c)
            .ok()
            .flatten()
            .map(|g| index_dto(c, &g)),
        job: last_index_job(c, &dirs.staging),
        tool_ready,
        searchable,
    }
}

pub fn tool_ready<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<EngineState>()
        .and_then(|e| e.get().map(|e| e.context().tools().embedder.is_some()))
        .unwrap_or(false)
}

fn dirs(st: &LibraryState) -> Result<Dirs, DomainError> {
    Ok(Dirs::under(st.get()?.data_dir()))
}

fn wake<R: Runtime>(app: &AppHandle<R>) {
    if let Some(e) = app.try_state::<EngineState>() {
        e.wake();
    }
}

#[tauri::command]
pub async fn get_search_index<R: Runtime>(
    app: AppHandle<R>,
) -> Result<SearchIndexDto, DomainError> {
    let ready = tool_ready(&app);
    blocking(app, move |st| {
        let d = dirs(st)?;
        Ok(search_index(&st.get()?.reader()?, &d, ready))
    })
    .await
}

/// Builds the index for the installed model, or adds what it misses;
/// `rebuild` starts a new one beside the active index.
#[tauri::command]
pub async fn build_search_index<R: Runtime>(
    app: AppHandle<R>,
    rebuild: bool,
) -> Result<SearchIndexDto, DomainError> {
    let ready = tool_ready(&app);
    let a = app.clone();
    let r = blocking(app, move |st| {
        let d = dirs(st)?;
        st.get()?.write(|c| {
            embed::request_index(c, &d.models, rebuild, now()).map_err(|e| match e {
                IndexError::NoModel => {
                    DomainError::new(ErrorCode::ModelMissing, "embedding-model-missing")
                }
                IndexError::Db => crate::library::internal("index-db"),
            })?;
            Ok(search_index(c, &d, ready))
        })
    })
    .await;
    wake(&a);
    r
}

/// Cancels the build under way; the active index stays as it was (AC-24).
#[tauri::command]
pub async fn cancel_search_index<R: Runtime>(
    app: AppHandle<R>,
) -> Result<SearchIndexDto, DomainError> {
    let ready = tool_ready(&app);
    let a = app.clone();
    let r = blocking(app, move |st| {
        let d = dirs(st)?;
        st.get()?.write(|c| {
            embed::cancel_build(c, now());
            Ok(search_index(c, &d, ready))
        })
    })
    .await;
    wake(&a);
    r
}
