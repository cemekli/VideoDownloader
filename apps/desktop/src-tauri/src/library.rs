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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
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

/// Items looked up per round of the thumbnail worker.
const THUMBNAIL_BATCH: u32 = 32;

/// FFmpeg runs at once for thumbnails: half the cores, 1 to 4.
fn thumbnail_workers() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get() / 2)
        .unwrap_or(1)
        .clamp(1, 4)
}

pub struct Library {
    /// Shared with the download engine's store (`engine::AppStore`).
    writer: Arc<Mutex<Database>>,
    db_path: PathBuf,
    data_dir: PathBuf,
    cache_dir: PathBuf,
    tools: Option<MediaTools>,
}

#[derive(Default)]
pub struct LibraryState {
    inner: OnceLock<Library>,
    /// Never set today; the import API takes a cancel flag and the UI gets a
    /// Cancel button once imports report progress.
    cancel: AtomicBool,
    /// This app run. Undo reaches only operations of the current run (§7.3).
    session: OnceLock<String>,
    /// The thumbnail worker runs; `thumbs_again` asks it for one more pass.
    thumbs_running: AtomicBool,
    thumbs_again: AtomicBool,
    /// Checked JSON imports waiting for the user's choices (F2.5), by token.
    pub(crate) imports:
        Mutex<std::collections::HashMap<String, crate::data_commands::PendingImport>>,
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
            writer: Arc::new(Mutex::new(db)),
            db_path,
            data_dir: data_dir.to_path_buf(),
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

    pub fn session(&self) -> &str {
        self.session.get_or_init(vv_persistence::library::new_id)
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

    /// Runs `f` on the database itself (backups and restore need it).
    pub fn with_db<T>(
        &self,
        f: impl FnOnce(&mut Database) -> Result<T, DomainError>,
    ) -> Result<T, DomainError> {
        let mut db = self
            .writer
            .lock()
            .map_err(|_| internal("library-poisoned"))?;
        f(&mut db)
    }

    /// The writer, for the download engine (one writer at a time either way).
    pub(crate) fn shared_writer(&self) -> Arc<Mutex<Database>> {
        Arc::clone(&self.writer)
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// `<data>/`: the app data folder (§14.2).
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn has_tools(&self) -> bool {
        self.tools.is_some()
    }

    pub fn tools(&self) -> Option<&MediaTools> {
        self.tools.as_ref()
    }

    /// `<data>/backups/`: manual, pre-restore and pre-migration backups.
    pub fn backup_dir(&self) -> PathBuf {
        self.data_dir.join("backups")
    }

    /// `<data>/media/`: the default folder for managed media (§14.2).
    pub fn default_media_dir(&self) -> PathBuf {
        self.data_dir.join("media")
    }

    /// Imports `paths`. Holds the writer for the whole run; reads go on in
    /// parallel. Thumbnails follow in the background ([`thumbnails`]).
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
        Ok(ImportSummary::from(&report))
    }

    /// Start-up pass: storage roots and asset presence (DATA-09). Missing
    /// thumbnails follow in the background ([`thumbnails`]).
    pub fn refresh_storage(&self) {
        let Ok(mut db) = self.writer.lock() else {
            return;
        };
        let _ = vv_import::roots::check_storage(db.writer(), &SystemClock);
        // Retention clean-up of Recently removed (§7.3); managed originals
        // are kept as orphaned media, derived files are deleted.
        let purged = (|| {
            let tx = db.writer().transaction()?;
            let r = vv_persistence::library::removal::purge_expired(
                &tx,
                SystemClock.now_ms(),
                vv_persistence::library::removal::RETENTION_DAYS,
            )?;
            tx.commit()?;
            Ok::<_, vv_persistence::library::LibraryError>(r)
        })();
        if let Ok(r) = purged {
            delete_files(db.writer(), &r.files_to_delete);
        }
    }
}

/// What happened to the files a permanent delete asked to remove.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDeletion {
    pub deleted: u32,
    /// Not deleted: root not reachable, file outside its root, or an OS error.
    pub failed: u32,
}

/// Deletes files given as `(storage_root_id, storage_ref)`, after the
/// database commit. A file is deleted only if it resolves inside its root;
/// a link pointing elsewhere is refused (same rule as the thumbnail
/// protocol). Missing files count as deleted.
pub fn delete_files(c: &Connection, refs: &[(String, String)]) -> FileDeletion {
    let mut out = FileDeletion::default();
    for (root_id, storage_ref) in refs {
        let ok = (|| {
            let root = vv_persistence::library::repo::get_storage_root(c, root_id).ok()??;
            let root = std::fs::canonicalize(&root.path).ok()?;
            let path = vv_import::fsutil::resolve_ref(&root, storage_ref);
            match std::fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(()),
                Err(_) => return None,
                Ok(m) if !m.is_file() => return None,
                Ok(_) => {}
            }
            let real = std::fs::canonicalize(&path).ok()?;
            if !real.starts_with(&root) {
                return None;
            }
            std::fs::remove_file(&real).ok()
        })();
        if ok.is_some() {
            out.deleted += 1;
        } else {
            out.failed += 1;
        }
    }
    out
}

/// Makes the missing thumbnails of local videos in the background (IMP-06).
///
/// The writer is held only to look items up and to record each result;
/// FFmpeg runs without it, so downloads, edits and the engine are not held
/// up by a large import. A failed item is not tried again in this run (it
/// would otherwise come first in every round and block the rest). Each new
/// thumbnail is announced with `item-changed`, so the grid shows it at once.
pub fn thumbnails<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<LibraryState>();
    state.thumbs_again.store(true, Ordering::SeqCst);
    if state.thumbs_running.swap(true, Ordering::SeqCst) {
        return; // the running worker takes another pass
    }
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("vv-thumbnails".into())
        .spawn(move || {
            let state = app.state::<LibraryState>();
            let mut failed = std::collections::HashSet::new();
            while state.thumbs_again.swap(false, Ordering::SeqCst) {
                let Ok(lib) = state.get() else { break };
                lib.thumbnail_pass(&mut failed, |id| {
                    let _ = app.emit(
                        crate::engine::ITEM_EVENT,
                        crate::engine::ItemChanged {
                            item_id: id.to_owned(),
                        },
                    );
                });
            }
            state.thumbs_running.store(false, Ordering::SeqCst);
        });
    if spawned.is_err() {
        state.thumbs_running.store(false, Ordering::SeqCst);
    }
}

impl Library {
    /// Rounds of [`THUMBNAIL_BATCH`] until no item without a thumbnail is
    /// left except those that failed in this run.
    fn thumbnail_pass(
        &self,
        failed: &mut std::collections::HashSet<String>,
        done: impl Fn(&str) + Sync,
    ) {
        use vv_import::thumbnail::{prepare, record, render};
        let Some(tools) = &self.tools else { return };
        loop {
            let round = (|| {
                let mut db = self.writer.lock().ok()?;
                let c = db.writer();
                let cache =
                    vv_import::thumbnail::ensure_cache_root(c, &SystemClock, &self.cache_dir)
                        .ok()?;
                let ids = vv_persistence::library::repo::items_missing_thumbnail(
                    c,
                    THUMBNAIL_BATCH + failed.len() as u32,
                )
                .ok()?;
                Some((cache, ids))
            })();
            let Some((cache, ids)) = round else { return };
            let todo: Vec<String> = ids.into_iter().filter(|id| !failed.contains(id)).collect();
            if todo.is_empty() {
                return;
            }
            // A few FFmpeg runs at once: each decodes one frame, mostly
            // waiting on the disk, so the first pass over a large import
            // goes several times faster without taking the machine.
            let queue = Mutex::new(todo.into_iter());
            let lost = Mutex::new(Vec::new());
            std::thread::scope(|s| {
                for _ in 0..thumbnail_workers() {
                    s.spawn(|| loop {
                        let Some(id) = queue.lock().ok().and_then(|mut q| q.next()) else {
                            return;
                        };
                        let prepared = match self.writer.lock() {
                            Ok(mut db) => prepare(db.writer(), &SystemClock, &cache, &id),
                            Err(_) => return,
                        };
                        let result = prepared.and_then(|p| {
                            let size = render(tools, &p)?;
                            let Ok(mut db) = self.writer.lock() else {
                                return Err(vv_import::thumbnail::ThumbnailError::NoOutput);
                            };
                            record(db.writer(), &SystemClock, tools, p, size)
                        });
                        match result {
                            Ok(_) => done(&id),
                            Err(_) => {
                                if let Ok(mut l) = lost.lock() {
                                    l.push(id);
                                }
                            }
                        }
                    });
                }
            });
            failed.extend(lost.into_inner().unwrap_or_default());
        }
    }
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

/// Pinned tool manifest built into this executable.
pub fn manifest() -> Option<vv_tools::Manifest> {
    vv_tools::Manifest::parse(include_str!("../../../../tools/manifest.json")).ok()
}

/// Folders that may hold the managed tools (`<dir>/<platform>/<id>/...`, the
/// `scripts/fetch-tools.mjs` layout): `VV_TOOLS_DIR` (debug/E2E), then
/// `tools/` next to the executable, then the repository's `tools/bin` in
/// debug builds.
fn tool_dirs() -> Vec<PathBuf> {
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
    dirs
}

/// ffprobe/FFmpeg for this process from the first folder that has them.
/// `None` makes import answer `TOOL_MISSING`.
pub fn find_tools() -> Option<MediaTools> {
    let build = tools_build();
    tool_dirs()
        .iter()
        .find_map(|d| MediaTools::from_tools_dir(d, build.clone()))
}

/// The folder the health check looks at: the first one with a folder for
/// this platform, else `tools/` next to the executable (every tool then
/// reports missing).
pub fn tool_dir() -> Option<PathBuf> {
    let dirs = tool_dirs();
    dirs.iter()
        .find(|d| d.join(vv_engine::toolset::platform()).is_dir())
        .or_else(|| dirs.first())
        .cloned()
}

pub(crate) fn ffmpeg_build() -> String {
    tools_build()
}

/// Runs in `setup` after the library lock.
pub fn open_library<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    let dir = crate::instance::data_dir(app)?;
    let state = app.state::<LibraryState>();
    state.open(&dir, find_tools())?;
    crate::engine::start(app);
    let handle = app.clone();
    std::thread::spawn(move || {
        if let Ok(lib) = handle.state::<LibraryState>().get() {
            lib.refresh_storage();
        }
        thumbnails(&handle);
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
            Ok(summary) => {
                thumbnails(&handle);
                ImportStatus::Finished { summary }
            }
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

    /// Real FFmpeg from `tools/bin` (skipped when the tools were not fetched).
    #[test]
    fn thumbnails_are_made_in_parallel_and_failures_do_not_block_the_rest() {
        let Some(tools) = find_tools() else {
            eprintln!("skipped: no tools in tools/bin");
            return;
        };
        let d = tempfile::tempdir().unwrap();
        let videos = d.path().join("videos");
        std::fs::create_dir_all(&videos).unwrap();
        let media = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fixtures/media");
        let good = std::fs::read(media.join("mp4-h264-aac.mp4")).unwrap();
        for i in 0..6 {
            // Distinct bytes, so none is a duplicate of another.
            let mut b = good.clone();
            b.extend_from_slice(&[i as u8; 16]);
            std::fs::write(videos.join(format!("clip{i}.mp4")), b).unwrap();
        }
        let st = LibraryState::default();
        st.open(&d.path().join("data"), Some(tools)).unwrap();
        let lib = st.get().unwrap();
        let s = lib
            .import(std::slice::from_ref(&videos), true, &AtomicBool::new(false))
            .unwrap();
        assert_eq!(s.imported, 6, "{s:?}");
        // One original vanishes after the import: its thumbnail fails.
        std::fs::remove_file(videos.join("clip0.mp4")).unwrap();
        let made = Mutex::new(Vec::new());
        let mut failed = std::collections::HashSet::new();
        lib.thumbnail_pass(&mut failed, |id| made.lock().unwrap().push(id.to_owned()));
        assert_eq!(made.lock().unwrap().len(), 5);
        assert_eq!(failed.len(), 1);
        // A second pass tries nothing again.
        lib.thumbnail_pass(&mut failed, |_| panic!("nothing left to make"));
    }
}
