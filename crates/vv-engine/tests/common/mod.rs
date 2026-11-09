//! Shared test set-up: a library in a temp folder, the fake yt-dlp and the
//! pinned FFmpeg/ffprobe from `tools/bin` (without them the tests that probe
//! media are skipped locally; CI fetches them first).

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};
use vv_engine::toolset::{ToolHealth, ToolReport, ToolState};
use vv_engine::ytdlp::YtdlpTools;
use vv_engine::{Context, Dirs, Engine, Events, Store, Tools};
use vv_import::MediaTools;
use vv_netguard::Policy;
use vv_persistence::library::{bookmark, Clock, FixedClock};
use vv_persistence::rusqlite::Connection;
use vv_persistence::{Database, OpenOptions};

pub const CANARY: &str = "VVCANARY-7f3a91";

pub struct TestStore {
    pub db: Mutex<Database>,
    pub path: PathBuf,
}

impl Store for TestStore {
    fn write<T>(&self, f: impl FnOnce(&mut Connection) -> T) -> T {
        let mut g = self.db.lock().unwrap_or_else(|e| e.into_inner());
        f(g.writer())
    }
    fn read<T>(&self, f: impl FnOnce(&Connection) -> T) -> T {
        let mut g = self.db.lock().unwrap_or_else(|e| e.into_inner());
        f(g.writer())
    }
}

/// Records every event; the canary test checks nothing secret went out.
#[derive(Default)]
pub struct Recorder {
    pub events: Mutex<Vec<String>>,
}

impl Events for Recorder {
    fn job_changed(&self, job_id: &str, item_id: Option<&str>) {
        self.events
            .lock()
            .unwrap()
            .push(format!("job {job_id} {item_id:?}"));
    }
    fn item_changed(&self, item_id: &str) {
        self.events.lock().unwrap().push(format!("item {item_id}"));
    }
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn manifest() -> vv_tools::Manifest {
    vv_tools::Manifest::parse(
        &std::fs::read_to_string(repo_root().join("tools/manifest.json")).unwrap(),
    )
    .unwrap()
}

pub fn real_tool_paths() -> Option<vv_engine::toolset::ToolPaths> {
    let p = vv_engine::toolset::ToolPaths::in_dir(&repo_root().join("tools/bin"), &manifest());
    (p.ytdlp.is_file() && p.ffmpeg.is_file() && p.ffprobe.is_file()).then_some(p)
}

/// FFmpeg/ffprobe, or a message why the test is skipped. CI must have them.
pub fn media_tools() -> Option<MediaTools> {
    match real_tool_paths() {
        Some(p) => Some(MediaTools::new(p.ffprobe, p.ffmpeg, "test")),
        None => {
            assert!(
                std::env::var_os("CI").is_none(),
                "tools/bin missing in CI (run node scripts/fetch-tools.mjs)"
            );
            eprintln!("skipped: no tools/bin (run node scripts/fetch-tools.mjs)");
            None
        }
    }
}

fn ok(id: &'static str) -> ToolHealth {
    ToolHealth {
        id,
        version: "test".into(),
        reported_version: Some("test".into()),
        state: ToolState::Ok,
        code: None,
        license: "test".into(),
        source: "test".into(),
        provenance: "published-sha256",
    }
}

pub fn ok_report() -> ToolReport {
    ToolReport {
        tools: ["yt-dlp", "deno", "ffmpeg", "ffprobe"]
            .into_iter()
            .map(ok)
            .collect(),
        checked_at: 0,
    }
}

pub struct Lib {
    pub dir: tempfile::TempDir,
    pub engine: Engine<TestStore>,
    pub clock: Arc<FixedClock>,
    pub events: Arc<Recorder>,
}

pub struct Setup {
    pub ytdlp: PathBuf,
    pub media: Option<MediaTools>,
    pub policy: Policy,
}

impl Setup {
    pub fn fake() -> Setup {
        Setup {
            ytdlp: PathBuf::from(env!("CARGO_BIN_EXE_vv-fake-ytdlp")),
            media: media_tools(),
            policy: Policy::default(),
        }
    }
}

pub fn library(setup: Setup) -> Lib {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().canonicalize().unwrap();
    let path = data.join("library.sqlite");
    let db = Database::open(&path, &OpenOptions::default()).unwrap();
    let dirs = Dirs::under(&data);
    std::fs::create_dir_all(&dirs.staging).unwrap();
    let clock = Arc::new(FixedClock::new(1_794_000_000_000));
    let events = Arc::new(Recorder::default());
    let ffmpeg = setup
        .media
        .as_ref()
        .map(|m| m.ffmpeg.clone())
        .unwrap_or_else(|| PathBuf::from("ffmpeg-not-installed"));
    let tools = Tools {
        report: ok_report(),
        ytdlp: Some(YtdlpTools {
            ytdlp: setup.ytdlp,
            deno: None,
            ffmpeg,
            deno_dir: dirs.deno_dir(),
        }),
        media: setup.media,
        ytdlp_version: "test".into(),
    };
    let cx = Context {
        store: TestStore {
            db: Mutex::new(db),
            path,
        },
        clock: clock.clone() as Arc<dyn Clock>,
        events: events.clone() as Arc<dyn Events>,
        dirs,
        tools: RwLock::new(tools),
        policy: setup.policy,
        offline: AtomicBool::new(false),
        owner: "test-owner".into(),
    };
    Lib {
        dir,
        engine: Engine::new(cx),
        clock,
        events,
    }
}

impl Lib {
    pub fn cx(&self) -> &Context<TestStore> {
        self.engine.context()
    }

    pub fn save(&self, url: &str) -> String {
        let clock = self.clock.clone();
        self.cx().store.write(|c| {
            match bookmark::save_bookmark(c, clock.as_ref(), url)
                .unwrap()
                .outcome
            {
                bookmark::SaveOutcome::Created(it) => it.id,
                bookmark::SaveOutcome::Existing { item, .. } => item.id,
            }
        })
    }

    pub fn write<T>(&self, f: impl FnOnce(&mut Connection) -> T) -> T {
        self.cx().store.write(f)
    }

    pub fn job(&self, id: &str) -> vv_persistence::jobs::Job {
        self.write(|c| vv_persistence::jobs::get(c, id).unwrap())
    }

    pub fn run(&self) -> usize {
        self.engine.run_once_blocking()
    }

    /// Every text value in every table, for "the canary is nowhere" checks.
    pub fn dump(&self) -> String {
        self.write(|c| {
            let tables: Vec<String> = c
                .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'search_document_%'")
                .unwrap()
                .query_map([], |r| r.get(0))
                .unwrap()
                .flatten()
                .collect();
            let mut out = String::new();
            for t in tables {
                let mut st = c.prepare(&format!("SELECT * FROM \"{t}\"")).unwrap();
                let n = st.column_count();
                let mut rows = st.query([]).unwrap();
                while let Some(r) = rows.next().unwrap() {
                    for i in 0..n {
                        if let Ok(v) = r.get::<_, vv_persistence::rusqlite::types::Value>(i) {
                            out.push_str(&format!("{v:?}\n"));
                        }
                    }
                }
            }
            out
        })
    }
}
