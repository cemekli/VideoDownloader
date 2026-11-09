//! Downloads, the job list and storage (STATE.md F3.2–F3.10; SPEC §9.5,
//! §12.3, §13, §14; ADR-0021).
//!
//! The webview names a preset (`best`, `1080`, `720`, `audio`), never a
//! format selector, a URL, a path or a tool argument. Jobs are shown as
//! DTOs with the actions the UI may offer; tool output and paths never
//! leave the backend.

use crate::engine::EngineState;
use crate::library::{internal, LibraryState};
use crate::library_commands::blocking;
use serde::Serialize;
use std::path::Path;
use tauri::{AppHandle, Manager, Runtime};
use vv_domain::error::{DomainError, ErrorCode};
use vv_engine::formats::{self, Preset, PresetId};
use vv_engine::storage::{self, StorageUsage};
use vv_engine::toolset::ToolHealth;
use vv_engine::{download, metadata, storage_move, Dirs};
use vv_persistence::jobs::{self, CancelReason, Job, JobError, JobKind, JobStatus};
use vv_persistence::library::repo;
use vv_persistence::library::{Clock, StorageRootKind, SystemClock};
use vv_persistence::rusqlite::Connection;
use vv_persistence::settings;

fn now() -> i64 {
    SystemClock.now_ms()
}

fn job_err(e: JobError) -> DomainError {
    match e {
        JobError::NotFound => DomainError::new(ErrorCode::InputRejected, "job-not-found"),
        JobError::InvalidTransition { .. } | JobError::Invalid(_) | JobError::Stale => {
            DomainError::new(ErrorCode::InputRejected, "job-action-not-allowed")
        }
        JobError::Sqlite(_) => internal("job-db"),
    }
}

fn code_err(id: &'static str) -> impl Fn(ErrorCode) -> DomainError {
    move |code| DomainError::new(code, id)
}

fn dirs(st: &LibraryState) -> Result<Dirs, DomainError> {
    Ok(Dirs::under(st.get()?.data_dir()))
}

fn wake<R: Runtime>(app: &AppHandle<R>) {
    if let Some(e) = app.try_state::<EngineState>() {
        e.wake();
    }
}

// ---------------------------------------------------------------- tools

/// MT-11: the health of each managed tool, for Diagnostics and the
/// Download button. No paths.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    /// The first check is still running.
    pub checking: bool,
    /// Downloads can start (yt-dlp, FFmpeg and ffprobe passed).
    pub ready: bool,
    /// Why not, when `ready` is false and the check finished.
    pub code: Option<ErrorCode>,
    pub tools: Vec<ToolHealth>,
    pub checked_at: Option<i64>,
}

pub fn tool_status(engine: Option<&EngineState>) -> ToolStatus {
    let report = engine.and_then(|e| e.checked().then(|| e.report()).flatten());
    match report {
        None => ToolStatus {
            checking: engine.is_some_and(|e| e.get().is_some()),
            ready: false,
            code: None,
            tools: Vec::new(),
            checked_at: None,
        },
        Some(r) => {
            let ready = r.ready_for_downloads();
            ToolStatus {
                checking: false,
                ready: ready.is_ok(),
                code: ready.err(),
                checked_at: Some(r.checked_at),
                tools: r.tools,
            }
        }
    }
}

#[tauri::command]
pub fn get_tool_health<R: Runtime>(app: AppHandle<R>) -> ToolStatus {
    tool_status(app.try_state::<EngineState>().as_deref())
}

// ---------------------------------------------------------------- jobs

/// One row of the Downloads screen.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobDto {
    pub id: String,
    /// `download`, `metadata` or `storage_move`.
    pub kind: String,
    pub item_id: Option<String>,
    pub item_title: Option<String>,
    /// §12.3 state, e.g. `running`, `retry_wait`, `failed`.
    pub status: String,
    /// `download_video`, `merge`, `verify`, ... while running.
    pub phase: Option<String>,
    pub bytes_done: Option<i64>,
    /// `None`: the total is unknown and progress is indeterminate (AC-09).
    pub bytes_total: Option<i64>,
    /// yt-dlp's estimate while the total is unknown; shown as "about".
    pub bytes_estimate: Option<i64>,
    pub units_done: Option<i64>,
    pub units_total: Option<i64>,
    pub preset: Option<String>,
    pub attempts: i64,
    pub max_attempts: i64,
    /// §19 code of the last failure.
    pub error_code: Option<String>,
    /// `offline`, `disk`, `auth`, `tool`, `path`, ... while blocked.
    pub blocked_reason: Option<String>,
    pub next_retry_at: Option<i64>,
    /// Bytes of partial files kept for a retry.
    pub partial_bytes: u64,
    pub created_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
    pub can_retry: bool,
    pub can_cancel: bool,
    pub can_remove_partial: bool,
}

fn job_dto(c: &Connection, staging: &Path, j: &Job) -> JobDto {
    let title = j
        .item_id
        .as_deref()
        .and_then(|id| repo::get_item(c, id).ok().flatten())
        .and_then(|it| it.title.or(it.canonical_url));
    let partial = if j.kind == JobKind::Download {
        download::partial_bytes(staging, &j.id)
    } else {
        0
    };
    let preset = (j.kind == JobKind::Download)
        .then(|| j.payload.get("preset").and_then(|p| p.as_str()))
        .flatten()
        .map(str::to_owned);
    let s = j.status;
    JobDto {
        id: j.id.clone(),
        kind: j.kind.as_str().to_owned(),
        item_id: j.item_id.clone(),
        item_title: title,
        status: s.as_str().to_owned(),
        phase: j.progress.phase.clone(),
        bytes_done: j.progress.bytes_done,
        bytes_total: j.progress.bytes_total,
        bytes_estimate: j.progress.bytes_estimate,
        units_done: j.progress.units_done,
        units_total: j.progress.units_total,
        preset,
        attempts: j.attempts,
        max_attempts: j.max_attempts,
        error_code: j.error_code.clone(),
        blocked_reason: j.blocked_reason.clone(),
        next_retry_at: j.next_retry_at,
        partial_bytes: partial,
        created_at: j.created_at,
        updated_at: j.updated_at,
        finished_at: j.finished_at,
        can_retry: matches!(
            s,
            JobStatus::Failed | JobStatus::Canceled | JobStatus::Blocked
        ),
        can_cancel: !s.is_terminal() && s != JobStatus::Canceling,
        can_remove_partial: partial > 0 && matches!(s, JobStatus::Failed | JobStatus::Canceled),
    }
}

/// Jobs shown on the Downloads screen: downloads and media-folder moves,
/// newest first.
pub fn list(c: &Connection, staging: &Path, limit: i64) -> Result<Vec<JobDto>, DomainError> {
    let all = jobs::list(c, &[], limit.saturating_mul(4)).map_err(job_err)?;
    Ok(all
        .iter()
        .filter(|j| matches!(j.kind, JobKind::Download | JobKind::StorageMove))
        .take(limit as usize)
        .map(|j| job_dto(c, staging, j))
        .collect())
}

fn one(c: &Connection, staging: &Path, id: &str) -> Result<JobDto, DomainError> {
    let j = jobs::get(c, id).map_err(job_err)?;
    Ok(job_dto(c, staging, &j))
}

#[tauri::command]
pub async fn list_jobs<R: Runtime>(app: AppHandle<R>) -> Result<Vec<JobDto>, DomainError> {
    blocking(app, |st| {
        let d = dirs(st)?;
        list(&st.get()?.reader()?, &d.staging, 200)
    })
    .await
}

#[tauri::command]
pub async fn cancel_job<R: Runtime>(
    app: AppHandle<R>,
    job_id: String,
) -> Result<JobDto, DomainError> {
    let a = app.clone();
    let r = blocking(app, move |st| {
        let d = dirs(st)?;
        st.get()?.write(|c| {
            jobs::cancel(c, &job_id, CancelReason::User, now()).map_err(job_err)?;
            one(c, &d.staging, &job_id)
        })
    })
    .await;
    wake(&a);
    r
}

/// Retry after a failure or cancel; a blocked job is released (the user
/// fixed the disk, signed in, ...). Partial files are reused.
#[tauri::command]
pub async fn retry_job<R: Runtime>(
    app: AppHandle<R>,
    job_id: String,
) -> Result<JobDto, DomainError> {
    let a = app.clone();
    let r = blocking(app, move |st| {
        let d = dirs(st)?;
        st.get()?.write(|c| {
            let j = jobs::get(c, &job_id).map_err(job_err)?;
            if j.status == JobStatus::Blocked {
                if j.blocked_reason.as_deref() == Some("offline")
                    && settings::offline_mode(c).unwrap_or(false)
                {
                    return Err(DomainError::new(ErrorCode::OfflineMode, "retry-offline"));
                }
                jobs::unblock(c, &job_id, now()).map_err(job_err)?;
            } else {
                jobs::retry(c, &job_id, now()).map_err(job_err)?;
            }
            one(c, &d.staging, &job_id)
        })
    })
    .await;
    wake(&a);
    r
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialRemoved {
    pub freed_bytes: u64,
}

/// "Delete partial file" on a failed or cancelled download (§13.3).
#[tauri::command]
pub async fn remove_partial<R: Runtime>(
    app: AppHandle<R>,
    job_id: String,
) -> Result<PartialRemoved, DomainError> {
    blocking(app, move |st| {
        let d = dirs(st)?;
        let c = st.get()?.reader()?;
        let j = jobs::get(&c, &job_id).map_err(job_err)?;
        if !matches!(j.status, JobStatus::Failed | JobStatus::Canceled) {
            return Err(DomainError::new(
                ErrorCode::InputRejected,
                "partial-job-not-finished",
            ));
        }
        let freed = download::remove_partial(&d.staging, &j).map_err(code_err("partial-remove"))?;
        Ok(PartialRemoved { freed_bytes: freed })
    })
    .await
}

// ---------------------------------------------------------------- download

/// What the Download dialog shows for one item.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadOptions {
    pub item_id: String,
    /// `pending`, `fetching`, `ready`, `failed` (metadata, §11).
    pub metadata_status: String,
    /// Format data from the last metadata fetch exists.
    pub formats_known: bool,
    pub presets: Vec<Preset>,
    pub offline: bool,
    pub tools_ready: bool,
    pub storage: StorageUsage,
    /// An open download of this item, if any.
    pub active_job_id: Option<String>,
    /// The page was a live stream at the last metadata fetch: not
    /// downloaded (SPEC §6); every preset is unavailable.
    pub live: bool,
}

fn media_dir(c: &Connection, d: &Dirs) -> std::path::PathBuf {
    settings::get(c)
        .ok()
        .and_then(|s| s.active_download_root_id)
        .and_then(|id| repo::get_storage_root(c, &id).ok().flatten())
        .map(|r| std::path::PathBuf::from(r.path))
        .unwrap_or_else(|| d.default_media.clone())
}

fn options(
    c: &Connection,
    d: &Dirs,
    item_id: &str,
    tools_ready: bool,
) -> Result<DownloadOptions, DomainError> {
    let it = repo::get_item(c, item_id)
        .map_err(|_| internal("item-read"))?
        .ok_or_else(|| DomainError::new(ErrorCode::InputRejected, "item-not-found"))?;
    let stored = metadata::stored_formats(c, item_id);
    let live = metadata::is_live(c, item_id);
    let presets = formats::presets(stored.as_deref().unwrap_or(&[]))
        .into_iter()
        .map(|(p, _)| Preset {
            available: p.available && !live,
            ..p
        })
        .collect();
    let active_job_id: Option<String> = c
        .query_row(
            "SELECT id FROM job WHERE item_id = ?1 AND kind = 'download' AND status NOT IN ('succeeded','failed','canceled') ORDER BY created_at DESC LIMIT 1",
            [item_id],
            |r| r.get(0),
        )
        .ok();
    Ok(DownloadOptions {
        item_id: item_id.to_owned(),
        metadata_status: it.metadata_status.as_str().to_owned(),
        formats_known: stored.is_some(),
        presets,
        offline: settings::offline_mode(c).unwrap_or(false),
        tools_ready,
        storage: storage::usage(c, d, &media_dir(c, d)),
        active_job_id,
        live,
    })
}

fn tools_ready<R: Runtime>(app: &AppHandle<R>) -> bool {
    tool_status(app.try_state::<EngineState>().as_deref()).ready
}

#[tauri::command]
pub async fn get_download_options<R: Runtime>(
    app: AppHandle<R>,
    item_id: String,
) -> Result<DownloadOptions, DomainError> {
    let ready = tools_ready(&app);
    blocking(app, move |st| {
        let d = dirs(st)?;
        options(&st.get()?.reader()?, &d, &item_id, ready)
    })
    .await
}

/// Checks that an item can be downloaded: it exists, is not removed and
/// has a source URL.
fn downloadable(c: &Connection, item_id: &str) -> Result<(), DomainError> {
    let it = repo::get_item(c, item_id)
        .map_err(|_| internal("item-read"))?
        .ok_or_else(|| DomainError::new(ErrorCode::InputRejected, "item-not-found"))?;
    if it.removed_at.is_some() || it.canonical_url.is_none() {
        return Err(DomainError::new(
            ErrorCode::InputRejected,
            "item-not-downloadable",
        ));
    }
    Ok(())
}

pub fn start(
    c: &mut Connection,
    staging: &Path,
    item_id: &str,
    preset: &str,
) -> Result<JobDto, DomainError> {
    let preset = PresetId::parse(preset)
        .ok_or_else(|| DomainError::new(ErrorCode::InputRejected, "preset-unknown"))?;
    downloadable(c, item_id)?;
    if metadata::is_live(c, item_id) {
        return Err(DomainError::new(ErrorCode::InputRejected, "live-stream"));
    }
    if settings::offline_mode(c).unwrap_or(false) {
        return Err(DomainError::new(ErrorCode::OfflineMode, "download-offline"));
    }
    let e = download::enqueue(c, item_id, preset, now()).map_err(job_err)?;
    one(c, staging, &e.id)
}

/// Queues a download of `item_id` with a preset (§9.5). The same item and
/// preset while one is open answers the open job (idempotency key).
#[tauri::command]
pub async fn start_download<R: Runtime>(
    app: AppHandle<R>,
    item_id: String,
    preset: String,
) -> Result<JobDto, DomainError> {
    let a = app.clone();
    let r = blocking(app, move |st| {
        let d = dirs(st)?;
        st.get()?.write(|c| start(c, &d.staging, &item_id, &preset))
    })
    .await;
    wake(&a);
    r
}

/// Fetches metadata again (the "Refresh metadata" action, §9.4).
#[tauri::command]
pub async fn fetch_metadata<R: Runtime>(
    app: AppHandle<R>,
    item_id: String,
) -> Result<String, DomainError> {
    let a = app.clone();
    let r = blocking(app, move |st| {
        st.get()?.write(|c| {
            downloadable(c, &item_id)?;
            if settings::offline_mode(c).unwrap_or(false) {
                return Err(DomainError::new(ErrorCode::OfflineMode, "metadata-offline"));
            }
            Ok(metadata::enqueue(c, &item_id, now()).map_err(job_err)?.id)
        })
    })
    .await;
    wake(&a);
    r
}

// ---------------------------------------------------------------- storage

#[tauri::command]
pub async fn get_storage_usage<R: Runtime>(app: AppHandle<R>) -> Result<StorageUsage, DomainError> {
    blocking(app, |st| {
        let d = dirs(st)?;
        let c = st.get()?.reader()?;
        Ok(storage::usage(&c, &d, &media_dir(&c, &d)))
    })
    .await
}

/// Sets or clears the managed media quota (DATA-07, AC-33).
#[tauri::command]
pub async fn set_storage_quota<R: Runtime>(
    app: AppHandle<R>,
    bytes: Option<u64>,
) -> Result<StorageUsage, DomainError> {
    blocking(app, move |st| {
        if bytes == Some(0) {
            return Err(DomainError::new(ErrorCode::InputRejected, "quota-zero"));
        }
        let d = dirs(st)?;
        st.get()?.write(|c| {
            storage::set_quota(c, bytes, now()).map_err(code_err("quota-write"))?;
            Ok(storage::usage(c, &d, &media_dir(c, &d)))
        })
    })
    .await
}

/// Queues moving managed media from the active folder to `dir` (DATA-08).
pub fn queue_move(
    c: &mut Connection,
    d: &Dirs,
    dir: &Path,
    staging: &Path,
) -> Result<JobDto, DomainError> {
    storage::writable(dir).map_err(code_err("move-target-not-writable"))?;
    let clock = SystemClock;
    let src = storage::active_media_root(c, &clock, d).map_err(code_err("move-source"))?;
    let label = dir
        .file_name()
        .map_or_else(|| "Media".to_owned(), |n| n.to_string_lossy().into_owned());
    let dst = storage::ensure_root(c, &clock, dir, StorageRootKind::ManagedMedia, &label)
        .map_err(code_err("move-target"))?;
    if dst.id == src.id {
        return Err(DomainError::new(
            ErrorCode::InputRejected,
            "move-same-folder",
        ));
    }
    let e = storage_move::enqueue(c, &src.id, &dst.id, clock.now_ms()).map_err(job_err)?;
    one(c, staging, &e.id)
}

/// Settings → Storage → "Move media folder": the new folder is picked in a
/// native dialog here; the move runs as a resumable job (AC-43).
#[tauri::command]
pub async fn move_media_folder<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<JobDto>, DomainError> {
    use tauri_plugin_dialog::DialogExt;
    let a = app.clone();
    let r = blocking(app.clone(), move |st| {
        let picked = match crate::e2e_picked() {
            Some(p) => p.into_iter().next(),
            None => {
                let mut dialog = app.dialog().file();
                if let Some(w) = app.get_webview_window("main") {
                    dialog = dialog.set_parent(&w);
                }
                dialog
                    .blocking_pick_folder()
                    .and_then(|p| p.into_path().ok())
            }
        };
        let Some(dir) = picked else { return Ok(None) };
        let d = dirs(st)?;
        st.get()?
            .write(|c| queue_move(c, &d, &dir, &d.staging))
            .map(Some)
    })
    .await;
    wake(&a);
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use vv_persistence::library::bookmark;
    use vv_persistence::{Database, OpenOptions};

    fn setup() -> (tempfile::TempDir, Database, Dirs, String) {
        let d = tempfile::tempdir().unwrap();
        let mut db = Database::open(&d.path().join("l.sqlite"), &OpenOptions::default()).unwrap();
        let dirs = Dirs::under(d.path());
        std::fs::create_dir_all(&dirs.staging).unwrap();
        let id = match bookmark::save_bookmark(
            db.writer(),
            &SystemClock,
            "https://youtu.be/vvTestVid01",
        )
        .unwrap()
        .outcome
        {
            bookmark::SaveOutcome::Created(it) => it.id,
            bookmark::SaveOutcome::Existing { item, .. } => item.id,
        };
        (d, db, dirs, id)
    }

    #[test]
    fn start_refuses_unknown_presets_offline_and_answers_the_open_job() {
        let (_d, mut db, dirs, id) = setup();
        let c = db.writer();
        let e = start(c, &dirs.staging, &id, "4k").unwrap_err();
        assert_eq!(e.code, ErrorCode::InputRejected);
        let j = start(c, &dirs.staging, &id, "720").unwrap();
        assert_eq!((j.kind.as_str(), j.status.as_str()), ("download", "queued"));
        assert_eq!(j.preset.as_deref(), Some("720"));
        assert!(j.can_cancel && !j.can_retry && !j.can_remove_partial);
        assert_eq!(start(c, &dirs.staging, &id, "720").unwrap().id, j.id);
        settings::update(
            c,
            &serde_json::from_value(serde_json::json!({"offlineMode": true})).unwrap(),
            1,
        )
        .unwrap();
        let e = start(c, &dirs.staging, &id, "best").unwrap_err();
        assert_eq!(e.code, ErrorCode::OfflineMode);
    }

    #[test]
    fn a_live_stream_is_not_downloaded() {
        let (_d, mut db, dirs, id) = setup();
        let c = db.writer();
        c.execute(
            "INSERT INTO remote_formats (item_id, fetched_at, tool_version, formats, is_live) VALUES (?1, 1, 'x', '[]', 1)",
            [&id],
        )
        .unwrap();
        let o = options(c, &dirs, &id, true).unwrap();
        assert!(o.live && o.presets.iter().all(|p| !p.available));
        let e = start(c, &dirs.staging, &id, "best").unwrap_err();
        assert_eq!(e.code, ErrorCode::InputRejected);
    }

    #[test]
    fn a_failed_download_with_partial_bytes_offers_retry_and_delete() {
        let (_d, mut db, dirs, id) = setup();
        let c = db.writer();
        let j = start(c, &dirs.staging, &id, "best").unwrap();
        jobs::cancel(c, &j.id, CancelReason::User, 2).unwrap();
        let part = download::staging_dir(&dirs.staging, &j.id);
        std::fs::create_dir_all(&part).unwrap();
        std::fs::write(part.join("media.mp4.part"), [0u8; 10]).unwrap();
        let l = list(c, &dirs.staging, 10).unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].status, "canceled");
        assert_eq!(l[0].partial_bytes, 10);
        assert!(l[0].can_retry && l[0].can_remove_partial && !l[0].can_cancel);
        let o = options(c, &dirs, &id, false).unwrap();
        assert_eq!(o.presets.len(), 4);
        assert!(!o.formats_known && o.active_job_id.is_none());
    }

    #[test]
    fn tool_status_without_an_engine_is_not_ready() {
        let s = tool_status(None);
        assert!(!s.ready && !s.checking && s.tools.is_empty());
    }
}
