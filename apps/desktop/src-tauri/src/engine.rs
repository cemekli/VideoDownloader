//! The download engine inside the app (STATE.md F3; SPEC §13; ADR-0021).
//!
//! Started in `setup` once the library is open. The tool health check runs
//! on its own thread first (hashing four executables takes a moment), then
//! start-up reconciliation, then the scheduler. Until the check is done the
//! engine has no tools, so a queued job waits rather than run an unchecked
//! executable.
//!
//! The UI hears about changes through [`JOB_EVENT`] and [`ITEM_EVENT`],
//! which carry ids only, and reads the details with commands.

use crate::library::{ffmpeg_build, manifest, tool_dir, LibraryState};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use vv_engine::toolset::{check_tools, ToolPaths, ToolReport};
use vv_engine::{Context, Dirs, Engine, Events, Store, Tools};
use vv_persistence::library::{new_id, Clock, SystemClock};
use vv_persistence::rusqlite::Connection;
use vv_persistence::Database;

/// A job was queued, made progress or ended: `{ jobId, itemId }`.
pub const JOB_EVENT: &str = "job-changed";
/// An item's metadata or media changed: `{ itemId }`.
pub const ITEM_EVENT: &str = "item-changed";
/// The tool health check finished.
pub const TOOLS_EVENT: &str = "tools-checked";

/// The library's writer, shared with the commands. Reads open their own
/// read-only connection so a long import never stalls a heartbeat read.
pub struct AppStore {
    db: Arc<Mutex<Database>>,
    path: PathBuf,
}

impl Store for AppStore {
    fn write<T>(&self, f: impl FnOnce(&mut Connection) -> T) -> T {
        let mut g = self.db.lock().unwrap_or_else(|e| e.into_inner());
        f(g.writer())
    }

    fn read<T>(&self, f: impl FnOnce(&Connection) -> T) -> T {
        match vv_persistence::open_reader(&self.path) {
            Ok(c) => f(&c),
            Err(_) => {
                let mut g = self.db.lock().unwrap_or_else(|e| e.into_inner());
                f(g.writer())
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobChanged {
    pub job_id: String,
    pub item_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemChanged {
    pub item_id: String,
}

struct TauriEvents<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Events for TauriEvents<R> {
    fn job_changed(&self, job_id: &str, item_id: Option<&str>) {
        let _ = self.0.emit(
            JOB_EVENT,
            JobChanged {
                job_id: job_id.to_owned(),
                item_id: item_id.map(str::to_owned),
            },
        );
    }

    fn item_changed(&self, item_id: &str) {
        let _ = self.0.emit(
            ITEM_EVENT,
            ItemChanged {
                item_id: item_id.to_owned(),
            },
        );
    }
}

#[derive(Default)]
pub struct EngineState {
    engine: OnceLock<Engine<AppStore>>,
    /// The tool check has finished at least once.
    checked: AtomicBool,
}

impl EngineState {
    pub fn get(&self) -> Option<&Engine<AppStore>> {
        self.engine.get()
    }

    /// Something was queued or released: schedule now. A no-op before the
    /// engine started (the IPC tests, or a library that failed to open).
    pub fn wake(&self) {
        if let Some(e) = self.get() {
            e.wake();
        }
    }

    pub fn set_offline(&self, offline: bool) {
        if let Some(e) = self.get() {
            e.set_offline(offline);
        }
    }

    pub fn checked(&self) -> bool {
        self.checked.load(Ordering::SeqCst)
    }

    /// The last health report; empty while the first check runs.
    pub fn report(&self) -> Option<ToolReport> {
        self.get().map(|e| e.context().tools().report)
    }
}

fn no_tools() -> Tools {
    Tools {
        report: ToolReport {
            tools: Vec::new(),
            checked_at: 0,
        },
        ytdlp: None,
        media: None,
        ytdlp_version: String::new(),
        whisper: None,
    }
}

/// Creates the engine and starts the check, reconciliation and scheduler
/// on a background thread.
pub fn start<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<LibraryState>();
    let Ok(lib) = state.get() else {
        return;
    };
    let dirs = Dirs::under(lib.data_dir());
    let _ = std::fs::create_dir_all(&dirs.staging);
    let cx = Context {
        store: AppStore {
            db: lib.shared_writer(),
            path: lib.db_path().to_owned(),
        },
        clock: Arc::new(SystemClock) as Arc<dyn Clock>,
        events: Arc::new(TauriEvents(app.clone())) as Arc<dyn Events>,
        dirs,
        tools: RwLock::new(no_tools()),
        policy: vv_netguard::Policy::default(),
        offline: AtomicBool::new(false),
        owner: format!("app-{}-{}", std::process::id(), new_id()),
        secrets: Arc::from(vv_secrets::os_store(&lib.data_dir().join("secrets"))),
    };
    if app
        .state::<EngineState>()
        .engine
        .set(Engine::new(cx))
        .is_err()
    {
        return;
    }
    let handle = app.clone();
    let _ = std::thread::Builder::new()
        .name("vv-engine-start".into())
        .spawn(move || {
            let st = handle.state::<EngineState>();
            let Some(engine) = st.get() else { return };
            crate::diag::timed("tool check", 0, || recheck_tools(engine));
            st.checked.store(true, Ordering::SeqCst);
            let _ = handle.emit(TOOLS_EVENT, ());
            engine.reconcile();
            engine.start();
        });
}

/// Runs the health check (AC-25) and hands only passing tools to jobs.
pub fn recheck_tools(engine: &Engine<AppStore>) {
    let cx = engine.context();
    let now = SystemClock.now_ms();
    let tools = match (manifest(), tool_dir()) {
        (Some(m), Some(dir)) => {
            let paths = ToolPaths::in_dir(&dir, &m);
            let report = check_tools(&m, &paths, now);
            let version = |id| m.tool(id).map(|t| t.version.clone()).unwrap_or_default();
            Tools::from_report(
                report,
                &paths,
                &cx.dirs,
                &ffmpeg_build(),
                &version(vv_tools::ToolId::YtDlp),
                &version(vv_tools::ToolId::WhisperCli),
            )
        }
        (Some(m), None) => Tools {
            report: ToolReport::all_missing(&m, now),
            ..no_tools()
        },
        _ => no_tools(),
    };
    *cx.tools.write().unwrap_or_else(|e| e.into_inner()) = tools;
    engine.unblock_tools();
}
