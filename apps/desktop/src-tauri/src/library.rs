//! The library this process has open (STATE.md F1.7, ADR-0013).
//!
//! Opened in `setup`, right after the library lock (INV-13), at
//! `<data>/db/library.sqlite` (SPEC §14.2). One writer behind a mutex; every
//! read opens its own read-only connection, so the Library screen keeps
//! answering while an import holds the writer.
//!
//! Files reach the import only from the native file dialog or from a drop on
//! the window, both handled here in Rust. The webview never sends a path.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use vv_domain::error::{DomainError, ErrorCode};
use vv_import::import::{FileOutcome, ImportOptions, ImportReport};
use vv_import::MediaTools;
use vv_persistence::library::{Clock, SystemClock};
use vv_persistence::rusqlite::Connection;
use vv_persistence::{Database, OpenOptions};

/// Event the UI listens to for imports it did not start itself (drops).
pub const IMPORT_EVENT: &str = "import-status";

/// Overrides where ffprobe/FFmpeg are looked up (`<dir>/<platform>/...`, the
/// `scripts/fetch-tools.mjs` layout). Honoured only in debug and E2E builds.
pub const TOOLS_DIR_ENV: &str = "VV_TOOLS_DIR";

/// Thumbnails made per background pass; the rest follow on the next pass.
const THUMBNAIL_BATCH: u32 = 200;

pub struct Library {
    writer: Mutex<Database>,
    db_path: PathBuf,
    cache_dir: PathBuf,
    tools: Option<MediaTools>,
}

#[derive(Default)]
pub struct LibraryState {
    inner: OnceLock<Library>,
    /// Never set today; the import API takes a cancel flag and the UI gets a
    /// Cancel button once imports report progress.
    cancel: AtomicBool,
}

fn not_open() -> DomainError {
    DomainError::new(ErrorCode::InternalError, "library-not-open")
}

pub(crate) fn internal(id: &str) -> DomainError {
    DomainError::new(ErrorCode::InternalError, id)
}

impl LibraryState {
    /// Opens (creating and migrating) the database under `data_dir`.
    pub fn open(&self, data_dir: &Path, tools: Option<MediaTools>) -> Result<(), String> {
        let db_dir = data_dir.join("db");
        std::fs::create_dir_all(&db_dir).map_err(|e| format!("cannot create db dir: {e}"))?;
        let db_path = db_dir.join("library.sqlite");
        let opts = OpenOptions {
            backup_dir: Some(data_dir.join("backups")),
        };
        let db = Database::open(&db_path, &opts).map_err(|e| e.to_string())?;
        let lib = Library {
            writer: Mutex::new(db),
            db_path,
            cache_dir: data_dir.join("cache"),
            tools,
        };
        self.inner
            .set(lib)
            .map_err(|_| "library already open".to_owned())
    }

    pub fn get(&self) -> Result<&Library, DomainError> {
        self.inner.get().ok_or_else(not_open)
    }
}

impl Library {
    pub fn reader(&self) -> Result<Connection, DomainError> {
        vv_persistence::open_reader(&self.db_path).map_err(|_| internal("library-reader"))
    }

    /// Runs `f` on the single writer connection (one write at a time).
    pub fn write<T>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T, DomainError>,
    ) -> Result<T, DomainError> {
        let mut db = self
            .writer
            .lock()
            .map_err(|_| internal("library-poisoned"))?;
        f(db.writer())
    }

    pub fn has_tools(&self) -> bool {
        self.tools.is_some()
    }

    /// Imports `paths` and then makes the missing thumbnails. Holds the
    /// writer for the whole run; reads go on in parallel.
    pub fn import(
        &self,
        paths: &[PathBuf],
        recursive: bool,
        cancel: &AtomicBool,
    ) -> Result<ImportSummary, DomainError> {
        let Some(tools) = &self.tools else {
            return Err(DomainError::new(ErrorCode::ToolMissing, "import-no-tools"));
        };
        let mut db = self
            .writer
            .lock()
            .map_err(|_| internal("library-poisoned"))?;
        let opts = ImportOptions {
            recursive,
            ..Default::default()
        };
        let report =
            vv_import::import::import_paths(db.writer(), &SystemClock, tools, paths, &opts, cancel)
                .map_err(|_| internal("import-failed"))?;
        // A thumbnail failure never fails the import (IMP-06).
        let _ = make_thumbnails(db.writer(), &SystemClock, tools, &self.cache_dir);
        Ok(ImportSummary::from(&report))
    }

    /// Start-up pass: storage roots and asset presence (DATA-09), then
    /// thumbnails that are still missing.
    pub fn refresh_storage(&self) {
        let Ok(mut db) = self.writer.lock() else {
            return;
        };
        let _ = vv_import::roots::check_storage(db.writer(), &SystemClock);
        if let Some(tools) = &self.tools {
            let _ = make_thumbnails(db.writer(), &SystemClock, tools, &self.cache_dir);
        }
    }
}

fn make_thumbnails(
    c: &Connection,
    clock: &dyn Clock,
    tools: &MediaTools,
    cache_dir: &Path,
) -> Result<(), vv_persistence::library::LibraryError> {
    let cache = vv_import::thumbnail::ensure_cache_root(c, clock, cache_dir)?;
    vv_import::thumbnail::generate_missing(c, clock, tools, &cache, THUMBNAIL_BATCH)?;
    Ok(())
}

/// Counts for the toast after an import. Per-file reasons stay in the
/// backend; the UI shows the first refusal's code.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    /// `false` when the user closed the dialog without choosing anything.
    pub picked: bool,
    pub imported: u32,
    pub already_imported: u32,
    pub duplicates: u32,
    pub rejected: u32,
    pub skipped: u32,
    pub ignored: u32,
    pub canceled: bool,
    /// §19 code of the first refused file, e.g. `INPUT_REJECTED`.
    pub first_error: Option<ErrorCode>,
}

impl From<&ImportReport> for ImportSummary {
    fn from(r: &ImportReport) -> Self {
        let mut s = ImportSummary {
            picked: true,
            ignored: r.ignored as u32,
            canceled: r.canceled,
            ..Default::default()
        };
        for f in &r.files {
            match &f.outcome {
                FileOutcome::Imported { duplicate_of, .. } => {
                    s.imported += 1;
                    if !duplicate_of.is_empty() {
                        s.duplicates += 1;
                    }
                }
                FileOutcome::AlreadyImported { .. } => s.already_imported += 1,
                FileOutcome::Rejected { code } => {
                    s.rejected += 1;
                    s.first_error.get_or_insert(*code);
                }
                FileOutcome::Skipped { .. } => s.skipped += 1,
            }
        }
        s
    }
}

/// What [`IMPORT_EVENT`] carries.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "phase")]
pub enum ImportStatus {
    Started,
    Finished { summary: ImportSummary },
    Failed { error: DomainError },
}

/// Pinned FFmpeg build string from `tools/manifest.json`, recorded in
/// thumbnail derivations.
fn tools_build() -> String {
    const MANIFEST: &str = include_str!("../../../../tools/manifest.json");
    serde_json::from_str::<serde_json::Value>(MANIFEST)
        .ok()
        .and_then(|m| {
            m["tools"]
                .as_array()?
                .iter()
                .find(|t| t["id"] == "ffmpeg")?
                .get("build")?
                .as_str()
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "unknown".into())
}

/// ffprobe/FFmpeg for this process: `VV_TOOLS_DIR` (debug/E2E), then
/// `tools/` next to the executable, then the repository's `tools/bin` in
/// debug builds. `None` makes import answer `TOOL_MISSING`.
pub fn find_tools() -> Option<MediaTools> {
    let build = tools_build();
    let mut dirs: Vec<PathBuf> = Vec::new();
    if cfg!(any(debug_assertions, feature = "e2e-cdp")) {
        if let Some(d) = std::env::var_os(TOOLS_DIR_ENV) {
            dirs.push(PathBuf::from(d));
        }
    }
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_owned))
    {
        dirs.push(exe_dir.join("tools"));
    }
    if cfg!(debug_assertions) {
        dirs.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tools/bin"));
    }
    dirs.iter()
        .find_map(|d| MediaTools::from_tools_dir(d, build.clone()))
}

/// Runs in `setup` after the library lock.
pub fn open_library<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    let dir = crate::instance::data_dir(app)?;
    let state = app.state::<LibraryState>();
    state.open(&dir, find_tools())?;
    let handle = app.clone();
    std::thread::spawn(move || {
        if let Ok(lib) = handle.state::<LibraryState>().get() {
            lib.refresh_storage();
        }
    });
    Ok(())
}

/// Imports files dropped on the window on a worker thread and reports the
/// result through [`IMPORT_EVENT`].
pub fn import_dropped<R: Runtime>(app: &AppHandle<R>, paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        let _ = handle.emit(IMPORT_EVENT, ImportStatus::Started);
        let state = handle.state::<LibraryState>();
        let status = match state
            .get()
            .and_then(|lib| lib.import(&paths, true, &state.cancel))
        {
            Ok(summary) => ImportStatus::Finished { summary },
            Err(error) => ImportStatus::Failed { error },
        };
        let _ = handle.emit(IMPORT_EVENT, status);
    });
}

/// The cancel flag shared by imports started from this state.
pub fn cancel_flag(state: &LibraryState) -> &AtomicBool {
    &state.cancel
}

#[cfg(test)]
mod tests {
    use super::*;
    use vv_import::fsutil::SkipReason;
    use vv_import::import::FileResult;

    #[test]
    fn summary_counts_each_outcome() {
        let f = |outcome| FileResult {
            path: PathBuf::from("x"),
            outcome,
        };
        let r = ImportReport {
            files: vec![
                f(FileOutcome::Imported {
                    item_id: "a".into(),
                    asset_id: "b".into(),
                    duplicate_of: vec!["c".into()],
                }),
                f(FileOutcome::Imported {
                    item_id: "d".into(),
                    asset_id: "e".into(),
                    duplicate_of: vec![],
                }),
                f(FileOutcome::AlreadyImported {
                    item_id: "a".into(),
                }),
                f(FileOutcome::Rejected {
                    code: ErrorCode::InputRejected,
                }),
                f(FileOutcome::Rejected {
                    code: ErrorCode::MediaCorrupt,
                }),
                f(FileOutcome::Skipped {
                    reason: SkipReason::Link,
                }),
            ],
            ignored: 3,
            canceled: false,
        };
        let s = ImportSummary::from(&r);
        assert_eq!(
            s,
            ImportSummary {
                picked: true,
                imported: 2,
                already_imported: 1,
                duplicates: 1,
                rejected: 2,
                skipped: 1,
                ignored: 3,
                canceled: false,
                first_error: Some(ErrorCode::InputRejected),
            }
        );
        let json = serde_json::to_value(ImportStatus::Finished { summary: s }).unwrap();
        assert_eq!(json["phase"], "finished");
        assert_eq!(json["summary"]["firstError"], "INPUT_REJECTED");
    }

    #[test]
    fn library_opens_under_db_and_reads_in_parallel() {
        let d = tempfile::tempdir().unwrap();
        let st = LibraryState::default();
        assert!(st.get().is_err());
        st.open(d.path(), None).unwrap();
        assert!(d.path().join("db/library.sqlite").is_file());
        let lib = st.get().unwrap();
        assert!(!lib.has_tools());
        let _held = lib.writer.lock().unwrap();
        // A reader works while the writer is held (an import is running).
        let n: i64 = lib
            .reader()
            .unwrap()
            .query_row("SELECT count(*) FROM library_item", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
        assert!(st.open(d.path(), None).is_err(), "opens once");
    }
}
