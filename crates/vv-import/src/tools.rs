//! Running the managed ffprobe/FFmpeg (SPEC §14.1).

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use vv_domain::error::ErrorCode;
use vv_ffmpeg::{ProbeError, Probed};
use vv_process::{ManagedCommand, Outcome};

/// Upper bound for what we keep of a tool's stdout/stderr.
const MAX_OUTPUT: u64 = 1 << 20;

#[derive(Debug, Clone)]
pub struct MediaTools {
    pub ffprobe: PathBuf,
    pub ffmpeg: PathBuf,
    /// Pinned build string (`tools/manifest.json`), recorded in derivations.
    pub build: String,
    pub probe_timeout: Duration,
    pub thumbnail_timeout: Duration,
}

/// `tools/bin/<platform>` directory name for this OS.
pub fn platform_dir() -> &'static str {
    if cfg!(windows) {
        "windows-x86_64"
    } else {
        "linux-x86_64"
    }
}

impl MediaTools {
    pub fn new(ffprobe: PathBuf, ffmpeg: PathBuf, build: impl Into<String>) -> Self {
        MediaTools {
            ffprobe,
            ffmpeg,
            build: build.into(),
            probe_timeout: Duration::from_secs(30),
            thumbnail_timeout: Duration::from_secs(60),
        }
    }

    /// The layout `scripts/fetch-tools.mjs` writes: `<bin>/<platform>/<id>/<exe>`.
    pub fn from_tools_dir(bin: &Path, build: impl Into<String>) -> Option<Self> {
        let exe = if cfg!(windows) { ".exe" } else { "" };
        let dir = bin.join(platform_dir());
        let ffprobe = dir.join("ffprobe").join(format!("ffprobe{exe}"));
        let ffmpeg = dir.join("ffmpeg").join(format!("ffmpeg{exe}"));
        (ffprobe.is_file() && ffmpeg.is_file()).then(|| MediaTools::new(ffprobe, ffmpeg, build))
    }

    /// Restricted ffprobe on one validated absolute path (IMP-02, AC-59).
    pub fn probe(&self, path: &Path) -> Result<Probed, ProbeFailure> {
        let args = vv_ffmpeg::args::probe_args(path).map_err(|_| ProbeFailure::NotAbsolute)?;
        let out = run(&self.ffprobe, args, self.probe_timeout).map_err(ProbeFailure::Tool)?;
        vv_ffmpeg::classify(out.success, &out.stdout, &out.stderr).map_err(ProbeFailure::Probe)
    }
}

#[derive(Debug)]
pub enum RunError {
    /// The tool could not be started (missing, blocked).
    Spawn(std::io::Error),
    TimedOut,
}

#[derive(Debug)]
pub enum ProbeFailure {
    Probe(ProbeError),
    Tool(RunError),
    NotAbsolute,
}

impl ProbeFailure {
    pub fn code(&self) -> ErrorCode {
        match self {
            ProbeFailure::Probe(e) => e.code(),
            ProbeFailure::Tool(RunError::Spawn(_)) => ErrorCode::ToolMissing,
            ProbeFailure::Tool(RunError::TimedOut) => ErrorCode::Timeout,
            ProbeFailure::NotAbsolute => ErrorCode::InternalError,
        }
    }
}

pub(crate) struct RunOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

fn drain(r: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut s = Vec::new();
        if let Some(r) = r {
            let _ = r.take(MAX_OUTPUT).read_to_end(&mut s);
        }
        String::from_utf8_lossy(&s).into_owned()
    })
}

/// Runs a managed tool with pipes read on their own threads (a full pipe
/// must not stall the child), a deadline, and the whole tree killed after.
pub(crate) fn run(
    exe: &Path,
    args: Vec<OsString>,
    timeout: Duration,
) -> Result<RunOutput, RunError> {
    let mut cmd = ManagedCommand::new(exe);
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(RunError::Spawn)?;
    let out = drain(child.take_stdout());
    let err = drain(child.take_stderr());
    let outcome = child.wait_timeout(timeout).map_err(RunError::Spawn)?;
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    match outcome {
        Outcome::Exited(status) => Ok(RunOutput {
            success: status.success(),
            stdout,
            stderr,
        }),
        Outcome::TimedOut => Err(RunError::TimedOut),
    }
}
