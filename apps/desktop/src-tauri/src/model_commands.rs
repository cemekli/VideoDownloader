//! Transcription models: list the catalog, agree to and start a download,
//! check and remove a model (STATE.md F6.1; SPEC §17.1, AC-20; ADR-0026).
//!
//! The webview sees what the user decides on: name, languages, size,
//! licence, source repository, the memory whisper.cpp lists for it, the
//! install state and the download job. It never sees a path. A download
//! starts only through [`download_model`] with `agreed: true`, which the
//! consent dialog sends after the user confirmed.

use crate::download_commands::{job_dto, JobDto};
use crate::engine::EngineState;
use crate::library::LibraryState;
use crate::library_commands::blocking;
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};
use vv_domain::error::{DomainError, ErrorCode};
use vv_engine::models::{self, RequestError};
use vv_engine::{storage, Dirs};
use vv_persistence::jobs::{self, CancelReason, JobKind};
use vv_persistence::library::{Clock, SystemClock};
use vv_persistence::models as rows;
use vv_persistence::rusqlite::Connection;

fn now() -> i64 {
    SystemClock.now_ms()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDto {
    pub id: String,
    pub label: String,
    pub languages: String,
    /// `None` while the catalog entry is not pinned (cannot be downloaded).
    pub size_bytes: Option<i64>,
    pub license: String,
    /// The repository the file comes from (shown in the consent dialog).
    pub source: String,
    /// What whisper.cpp's README lists; not a measurement of ours.
    pub upstream_memory_mb: i64,
    /// `not_installed`, `pending`, `ready`, `missing` or `corrupt`.
    pub status: String,
    pub consent_at: Option<i64>,
    pub verified_at: Option<i64>,
    /// The last download job of this model, if any.
    pub job: Option<JobDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelOverview {
    pub models: Vec<ModelDto>,
    /// Free space where models are stored, when known.
    pub free_bytes: Option<u64>,
}

fn last_job(c: &Connection, staging: &std::path::Path, id: &str) -> Option<JobDto> {
    let prefix = format!("model_download:{id}:");
    jobs::list(c, &[], 500)
        .ok()?
        .into_iter()
        .filter(|j| j.kind == JobKind::ModelDownload && j.idempotency_key.starts_with(&prefix))
        .max_by_key(|j| (j.created_at, j.id.clone()))
        .map(|j| job_dto(c, staging, &j))
}

pub fn overview(c: &Connection, dirs: &Dirs) -> ModelOverview {
    let cat = models::catalog();
    let models = cat
        .models
        .iter()
        .map(|m| {
            let inst = rows::get(c, &m.id).ok().flatten();
            ModelDto {
                id: m.id.clone(),
                label: m.label.clone(),
                languages: m.languages.clone(),
                size_bytes: cat.spec(m).map(|s| s.size_bytes),
                license: cat.license.clone(),
                source: cat.repository.clone(),
                upstream_memory_mb: m.upstream_memory_mb,
                status: inst
                    .as_ref()
                    .map_or("not_installed", |i| i.status.as_str())
                    .to_owned(),
                consent_at: inst.as_ref().map(|i| i.consent_at),
                verified_at: inst.as_ref().and_then(|i| i.verified_at),
                job: last_job(c, &dirs.staging, &m.id),
            }
        })
        .collect();
    ModelOverview {
        models,
        free_bytes: storage::free_bytes(&dirs.models),
    }
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
pub async fn list_models<R: Runtime>(app: AppHandle<R>) -> Result<ModelOverview, DomainError> {
    blocking(app, |st| {
        let d = dirs(st)?;
        Ok(overview(&st.get()?.reader()?, &d))
    })
    .await
}

/// The user confirmed the consent dialog for `model_id` (§17.1). Without
/// `agreed` nothing is written or downloaded (AC-20).
#[tauri::command]
pub async fn download_model<R: Runtime>(
    app: AppHandle<R>,
    model_id: String,
    agreed: bool,
) -> Result<ModelOverview, DomainError> {
    if !agreed {
        return Err(DomainError::new(
            ErrorCode::InputRejected,
            "model-consent-required",
        ));
    }
    let a = app.clone();
    let r = blocking(app, move |st| {
        let d = dirs(st)?;
        st.get()?.write(|c| {
            models::agree_and_queue(c, models::catalog(), &model_id, now()).map_err(
                |e| match e {
                    RequestError::UnknownModel | RequestError::NotPinned => {
                        DomainError::new(ErrorCode::InputRejected, "model-not-available")
                    }
                    RequestError::Db => crate::library::internal("model-db"),
                },
            )?;
            Ok(overview(c, &d))
        })
    })
    .await;
    wake(&a);
    r
}

/// Hashes an installed model again (slow for big files).
#[tauri::command]
pub async fn verify_model<R: Runtime>(
    app: AppHandle<R>,
    model_id: String,
) -> Result<ModelOverview, DomainError> {
    blocking(app, move |st| {
        let d = dirs(st)?;
        st.get()?.write(|c| {
            models::verify(c, &d.models, &model_id, now())
                .ok_or_else(|| DomainError::new(ErrorCode::ModelMissing, "model-not-installed"))?;
            Ok(overview(c, &d))
        })
    })
    .await
}

/// Cancels an open download and removes the model, its files and the
/// consent. The library keeps working without it (§26.2).
#[tauri::command]
pub async fn remove_model<R: Runtime>(
    app: AppHandle<R>,
    model_id: String,
) -> Result<ModelOverview, DomainError> {
    let a = app.clone();
    let r = blocking(app, move |st| {
        let d = dirs(st)?;
        st.get()?.write(|c| {
            let prefix = format!("model_download:{model_id}:");
            let open: Vec<String> = jobs::list(c, &[], 500)
                .unwrap_or_default()
                .into_iter()
                .filter(|j| {
                    j.kind == JobKind::ModelDownload
                        && j.idempotency_key.starts_with(&prefix)
                        && !j.status.is_terminal()
                })
                .map(|j| j.id)
                .collect();
            for id in open {
                let _ = jobs::cancel(c, &id, CancelReason::User, now());
            }
            models::remove(c, &d.models, &model_id);
            Ok(overview(c, &d))
        })
    })
    .await;
    wake(&a);
    r
}
