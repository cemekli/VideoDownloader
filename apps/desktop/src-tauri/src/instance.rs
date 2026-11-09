//! One app process per library (INV-13, SPEC §13.2, AC-38, ADR-0008).
//!
//! Two layers:
//! 1. `tauri-plugin-single-instance`: a second launch hands its arguments to
//!    the running app (which brings its window to the front) and exits with
//!    code 0 before any window or database exists.
//! 2. [`LibraryLock`] in the data directory: catches what the plugin cannot
//!    see (another build or identifier, a portable copy, another session on
//!    the same folder). Without the lock the process exits with
//!    [`EXIT_LIBRARY_LOCKED`] and never opens the database.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Runtime};
use vv_persistence::lock::{LibraryLock, LockError};

/// Exit code when another process holds the library lock.
pub const EXIT_LIBRARY_LOCKED: i32 = 3;

/// Overrides the data directory. Honoured only in debug and E2E builds, so
/// tests never touch the runner's real `%APPDATA%`.
pub const DATA_DIR_ENV: &str = "VV_DATA_DIR";

#[derive(Default)]
pub struct InstanceState {
    second_launches: AtomicU32,
    lock: Mutex<Option<LibraryLock>>,
}

impl InstanceState {
    pub fn second_launches(&self) -> u32 {
        self.second_launches.load(Ordering::SeqCst)
    }

    pub fn holds_library_lock(&self) -> bool {
        self.lock.lock().unwrap().is_some()
    }
}

pub fn data_dir<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<PathBuf> {
    if cfg!(any(debug_assertions, feature = "e2e-cdp")) {
        if let Some(d) = std::env::var_os(DATA_DIR_ENV) {
            return Ok(PathBuf::from(d));
        }
    }
    app.path().app_data_dir()
}

/// Takes the library lock or ends the process. Runs in `setup`, before the
/// database or any worker exists.
pub fn take_library_lock<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    let dir = data_dir(app)?;
    match LibraryLock::acquire(&dir) {
        Ok(lock) => {
            *app.state::<InstanceState>().lock.lock().unwrap() = Some(lock);
            Ok(())
        }
        Err(LockError::Held { pid }) => {
            // No window yet; the running instance keeps the library.
            eprintln!(
                "VideoVault: library is open in another process{}; exiting",
                pid.map(|p| format!(" (pid {p})")).unwrap_or_default()
            );
            std::process::exit(EXIT_LIBRARY_LOCKED);
        }
        Err(e) => Err(Box::new(e)),
    }
}

/// Runs in the first instance when a second one starts. The arguments (a
/// URL or file to add) are handed to intake once that exists (STATE.md F1);
/// they may hold paths, so they are not logged.
pub fn on_second_launch<R: Runtime>(app: &AppHandle<R>, _args: Vec<String>, _cwd: String) {
    app.state::<InstanceState>()
        .second_launches
        .fetch_add(1, Ordering::SeqCst);
    crate::diag::log("second launch handed over: window restored and focused");
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}
