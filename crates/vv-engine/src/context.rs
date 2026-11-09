//! What every job handler shares: the database, the tools, the folders and
//! the signals from the app (STATE.md F3; SPEC §13, §14.2).

use crate::toolset::{ToolPaths, ToolReport};
use crate::ytdlp::YtdlpTools;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use vv_import::MediaTools;
use vv_netguard::Policy;
use vv_persistence::library::Clock;
use vv_persistence::rusqlite::Connection;

/// The app's database as the engine uses it: one writer at a time, reads on
/// their own connection. The desktop app implements it on its library
/// mutex; tests on a plain `Mutex<Database>`.
pub trait Store: Send + Sync + 'static {
    fn write<T>(&self, f: impl FnOnce(&mut Connection) -> T) -> T;
    fn read<T>(&self, f: impl FnOnce(&Connection) -> T) -> T;
}

/// Told when a job or an item changed, so the UI can refresh. Carries ids
/// only, never content.
pub trait Events: Send + Sync + 'static {
    fn job_changed(&self, job_id: &str, item_id: Option<&str>);
    fn item_changed(&self, item_id: &str);
}

/// For tests and headless use.
pub struct NoEvents;
impl Events for NoEvents {
    fn job_changed(&self, _: &str, _: Option<&str>) {}
    fn item_changed(&self, _: &str) {}
}

/// Folders of §14.2.
#[derive(Debug, Clone)]
pub struct Dirs {
    /// `<data>/staging/`: partial downloads, never shown as media.
    pub staging: PathBuf,
    /// `<data>/cache/`: thumbnails and Deno's cache.
    pub cache: PathBuf,
    /// Where downloads go when the user picked no folder (`<data>/media`).
    pub default_media: PathBuf,
}

impl Dirs {
    pub fn under(data: &Path) -> Dirs {
        Dirs {
            staging: data.join("staging"),
            cache: data.join("cache"),
            default_media: data.join("media"),
        }
    }

    pub fn deno_dir(&self) -> PathBuf {
        self.cache.join("deno")
    }

    /// Per-job working copies of cookie files (§21.1): a folder only the
    /// current user can read, emptied at every start.
    pub fn cookie_copies(&self) -> PathBuf {
        self.staging.join("cookies")
    }
}

/// Tools that passed the health check, plus the report.
#[derive(Debug, Clone)]
pub struct Tools {
    pub report: ToolReport,
    pub ytdlp: Option<YtdlpTools>,
    pub media: Option<MediaTools>,
    /// The pinned yt-dlp version, recorded in provenance.
    pub ytdlp_version: String,
}

impl Tools {
    /// Builds the runnable set from a report: a tool that failed its check
    /// is never handed to a job.
    pub fn from_report(
        report: ToolReport,
        paths: &ToolPaths,
        dirs: &Dirs,
        ffmpeg_build: &str,
        ytdlp_version: &str,
    ) -> Tools {
        use vv_tools::ToolId;
        let media = (report.is_ok(ToolId::Ffmpeg) && report.is_ok(ToolId::Ffprobe))
            .then(|| MediaTools::new(paths.ffprobe.clone(), paths.ffmpeg.clone(), ffmpeg_build));
        let ytdlp = (report.ready_for_downloads().is_ok()).then(|| YtdlpTools {
            ytdlp: paths.ytdlp.clone(),
            deno: report.is_ok(ToolId::Deno).then(|| paths.deno.clone()),
            ffmpeg: paths.ffmpeg.clone(),
            deno_dir: dirs.deno_dir(),
        });
        Tools {
            report,
            ytdlp,
            media,
            ytdlp_version: ytdlp_version.to_owned(),
        }
    }
}

pub struct Context<S: Store> {
    pub store: S,
    pub clock: Arc<dyn Clock>,
    pub events: Arc<dyn Events>,
    pub dirs: Dirs,
    pub tools: RwLock<Tools>,
    /// Base network policy; `offline` is filled per run from [`Self::offline`].
    pub policy: Policy,
    /// Offline Mode, mirrored from settings so a running job sees it at once.
    pub offline: AtomicBool,
    /// This process; written as the lease owner.
    pub owner: String,
    /// The OS secret store (cookie-file sessions, ADR-0024).
    pub secrets: Arc<dyn vv_secrets::SecretStore>,
}

impl<S: Store> Context<S> {
    pub fn now(&self) -> i64 {
        self.clock.now_ms()
    }

    pub fn is_offline(&self) -> bool {
        self.offline.load(Ordering::SeqCst)
    }

    pub fn run_policy(&self) -> Policy {
        let mut p = self.policy.clone();
        p.offline = self.is_offline();
        p
    }

    pub fn tools(&self) -> Tools {
        self.tools.read().unwrap_or_else(|e| e.into_inner()).clone()
    }
}
