//! Text on screen (STATE.md F7.4; SPEC AI-C01…C04, SEA-08, AC-41, DEC-13;
//! ADR-0039): what AI Processing shows, reading it for all videos or one,
//! and cancel.
//!
//! The OCR is Windows' own: nothing is downloaded or sent anywhere. It
//! reads the first of the user's Windows languages that has an OCR pack;
//! without one, reading is refused with `MODEL_MISSING` and the screen
//! says how to add one.

use crate::download_commands::{job_dto, JobDto};
use crate::engine::EngineState;
use crate::library::LibraryState;
use crate::library_commands::blocking;
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};
use vv_domain::error::{DomainError, ErrorCode};
use vv_engine::screen_text;
use vv_engine::Dirs;
use vv_persistence::jobs::{self, CancelReason, JobKind};
use vv_persistence::library::{Clock, SystemClock};
use vv_persistence::rusqlite::Connection;

fn now() -> i64 {
    SystemClock.now_ms()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenTextDto {
    /// This PC can read text on screen (Windows with an OCR language).
    pub available: bool,
    /// The language it reads (BCP 47), when available.
    pub language: Option<String>,
    /// Live items with a video file.
    pub videos: i64,
    /// Videos read with their current file / that could not be read.
    pub read: i64,
    pub failed: i64,
    /// Videos a run for all would read now.
    pub pending: i64,
    /// Lines of text on screen stored.
    pub lines: i64,
    /// The open or last run for all videos.
    pub job: Option<JobDto>,
}

/// The OCR this PC has, as `(available, language)`.
pub fn ocr_state() -> (bool, Option<String>) {
    match vv_ocr::system() {
        Ok(r) => (true, Some(r.language().to_owned())),
        Err(_) => (false, None),
    }
}

fn last_job(c: &Connection, staging: &std::path::Path) -> Option<JobDto> {
    jobs::list_shown(c, &[JobKind::Ocr], 200)
        .ok()?
        .into_iter()
        .filter(|j| j.item_id.is_none())
        .max_by_key(|j| (!j.status.is_terminal(), j.created_at, j.id.clone()))
        .map(|j| job_dto(c, staging, &j))
}

pub fn screen_text_view(c: &Connection, dirs: &Dirs, ocr: (bool, Option<String>)) -> ScreenTextDto {
    let (read, failed, lines) = vv_persistence::screen_text::counts(c).unwrap_or((0, 0, 0));
    ScreenTextDto {
        available: ocr.0,
        language: ocr.1,
        videos: screen_text::with_video(c),
        read,
        failed,
        pending: screen_text::pending(c).len() as i64,
        lines,
        job: last_job(c, &dirs.staging),
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
pub async fn get_screen_text<R: Runtime>(app: AppHandle<R>) -> Result<ScreenTextDto, DomainError> {
    blocking(app, move |st| {
        let d = dirs(st)?;
        Ok(screen_text_view(&st.get()?.reader()?, &d, ocr_state()))
    })
    .await
}

/// Queues reading the text on screen: of every video not read with its
/// current file (`itemId` null), or of one item again. Without an OCR
/// language on this PC: `MODEL_MISSING`.
#[tauri::command]
pub async fn read_screen_text<R: Runtime>(
    app: AppHandle<R>,
    item_id: Option<String>,
) -> Result<ScreenTextDto, DomainError> {
    let a = app.clone();
    let r = blocking(app, move |st| {
        let d = dirs(st)?;
        let ocr = ocr_state();
        if !ocr.0 {
            return Err(DomainError::new(ErrorCode::ModelMissing, "ocr-unavailable"));
        }
        st.get()?.write(|c| {
            if let Some(id) = &item_id {
                let has = vv_engine::screen_text::video_asset(c, id).is_some();
                if !has {
                    return Err(DomainError::new(ErrorCode::FileMissing, "no-video-file"));
                }
            }
            screen_text::request(c, item_id.as_deref(), now())
                .map_err(|_| crate::library::internal("ocr-enqueue"))?;
            Ok(screen_text_view(c, &d, ocr))
        })
    })
    .await;
    wake(&a);
    r
}

/// Cancels the run for all videos; what it read so far stays.
#[tauri::command]
pub async fn cancel_screen_text<R: Runtime>(
    app: AppHandle<R>,
) -> Result<ScreenTextDto, DomainError> {
    let a = app.clone();
    let r = blocking(app, move |st| {
        let d = dirs(st)?;
        st.get()?.write(|c| {
            let open: Vec<String> = jobs::list_shown(c, &[JobKind::Ocr], 200)
                .unwrap_or_default()
                .into_iter()
                .filter(|j| j.item_id.is_none() && !j.status.is_terminal())
                .map(|j| j.id)
                .collect();
            for id in open {
                let _ = jobs::cancel(c, &id, CancelReason::User, now());
            }
            Ok(screen_text_view(c, &d, ocr_state()))
        })
    })
    .await;
    wake(&a);
    r
}
