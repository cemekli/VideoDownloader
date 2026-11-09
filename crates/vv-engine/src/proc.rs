//! Running managed tools with bounded output (DL-05, SPEC §14.1).
//!
//! Every run goes through `vv_process::ManagedCommand`: clean environment,
//! whole process tree in one job, killed on timeout, cancel or drop. Pipes
//! are read on their own threads so a full pipe never stalls the child, and
//! nothing is kept beyond a fixed limit.

use std::ffi::OsString;
use std::io::{self, Read};
use std::path::Path;
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use vv_domain::error::ErrorCode;
use vv_process::ManagedCommand;
use vv_ytdlp::lines::{LineSplitter, MAX_LINE_BYTES};
use vv_ytdlp::outcome::{Exit, Stream};

/// What a short run (version query, probe) left behind.
#[derive(Debug, Clone)]
pub struct Captured {
    /// `None` when the process was killed (timeout).
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
}

/// SPEC §19 code for a tool that could not be started at all.
///
/// - Not there: `TOOL_MISSING`.
/// - There but the OS refused to run it: `TOOL_BLOCKED`. That is what an
///   antivirus quarantine or an execution policy looks like: access denied
///   (Windows 5, Unix EACCES), "contains a virus" (Windows 225, 226) or
///   "blocked by group policy" (Windows 1260).
pub fn spawn_error_code(e: &io::Error) -> ErrorCode {
    const WIN_ACCESS_DENIED: i32 = 5;
    const WIN_VIRUS_INFECTED: i32 = 225;
    const WIN_VIRUS_DELETED: i32 = 226;
    const WIN_BLOCKED_BY_POLICY: i32 = 1260;
    if cfg!(windows) {
        if let Some(
            WIN_ACCESS_DENIED | WIN_VIRUS_INFECTED | WIN_VIRUS_DELETED | WIN_BLOCKED_BY_POLICY,
        ) = e.raw_os_error()
        {
            return ErrorCode::ToolBlocked;
        }
    }
    match e.kind() {
        io::ErrorKind::PermissionDenied => ErrorCode::ToolBlocked,
        _ => ErrorCode::ToolMissing,
    }
}

fn drain(r: Option<impl Read + Send + 'static>, max: u64) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut s = Vec::new();
        if let Some(r) = r {
            let mut r = r.take(max);
            let _ = r.read_to_end(&mut s);
            // Keep reading so the child never blocks on a full pipe.
            let _ = io::copy(&mut r.into_inner(), &mut io::sink());
        }
        s
    })
}

/// Runs `exe args` to completion or `timeout`, keeping at most `max` bytes
/// of each stream.
pub fn run_capture(
    exe: &Path,
    args: &[OsString],
    timeout: Duration,
    max: u64,
) -> io::Result<Captured> {
    let mut cmd = ManagedCommand::new(exe);
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    let out = drain(child.take_stdout(), max);
    let err = drain(child.take_stderr(), max);
    let outcome = child.wait_timeout(timeout)?;
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    Ok(match outcome {
        vv_process::Outcome::Exited(s) => Captured {
            code: s.code(),
            stdout,
            stderr,
            timed_out: false,
        },
        vv_process::Outcome::TimedOut => Captured {
            code: None,
            stdout,
            stderr,
            timed_out: true,
        },
    })
}

/// How stdout is read by [`run_streaming`].
#[derive(Debug, Clone, Copy)]
pub enum StdoutMode {
    /// Bounded lines, handed to the callback (downloads: progress lines).
    Lines,
    /// One document of at most this many bytes (metadata JSON); a larger
    /// one is cut and reported as truncated.
    Raw(usize),
}

/// End of a streaming run.
#[derive(Debug)]
pub struct Streamed {
    pub exit: Exit,
    /// We stopped it (cancel, pause, offline, deadline).
    pub stopped: bool,
    pub timed_out: bool,
    /// [`StdoutMode::Raw`] only.
    pub stdout: Vec<u8>,
    pub stdout_truncated: bool,
}

enum Msg {
    Line(Stream, String),
    Raw(Vec<u8>, bool),
    Done,
}

fn line_reader(
    r: Option<impl Read + Send + 'static>,
    stream: Stream,
    tx: mpsc::Sender<Msg>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let Some(mut r) = r else {
            let _ = tx.send(Msg::Done);
            return;
        };
        let mut split = LineSplitter::new(MAX_LINE_BYTES);
        let mut buf = [0u8; 8192];
        loop {
            match r.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    for l in split.push(&buf[..n]) {
                        if tx.send(Msg::Line(stream, l.text)).is_err() {
                            return;
                        }
                    }
                }
            }
        }
        if let Some(l) = split.finish() {
            let _ = tx.send(Msg::Line(stream, l.text));
        }
        let _ = tx.send(Msg::Done);
    })
}

fn raw_reader(
    r: Option<impl Read + Send + 'static>,
    max: usize,
    tx: mpsc::Sender<Msg>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut out = Vec::new();
        let mut truncated = false;
        if let Some(r) = r {
            let mut lim = r.take(max as u64 + 1);
            let _ = lim.read_to_end(&mut out);
            if out.len() > max {
                out.truncate(max);
                truncated = true;
            }
            let _ = io::copy(&mut lim.into_inner(), &mut io::sink());
        }
        let _ = tx.send(Msg::Raw(out, truncated));
        let _ = tx.send(Msg::Done);
    })
}

/// Runs a managed command, handing each bounded line to `on_line` as it
/// arrives. `should_stop` is asked about every 100 ms; when it says yes the
/// whole tree is killed. `deadline` bounds the run as a whole.
pub fn run_streaming(
    mut cmd: ManagedCommand,
    mode: StdoutMode,
    deadline: Option<Duration>,
    mut on_line: impl FnMut(Stream, &str),
    mut should_stop: impl FnMut() -> bool,
) -> io::Result<Streamed> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    let (tx, rx) = mpsc::channel();
    let out = match mode {
        StdoutMode::Lines => line_reader(child.take_stdout(), Stream::Stdout, tx.clone()),
        StdoutMode::Raw(max) => raw_reader(child.take_stdout(), max, tx.clone()),
    };
    let err = line_reader(child.take_stderr(), Stream::Stderr, tx);
    let started = Instant::now();
    let mut open = 2;
    let mut status = None;
    let mut stopped = false;
    let mut timed_out = false;
    let mut raw = (Vec::new(), false);
    let mut last_check = Instant::now();
    while open > 0 || status.is_none() {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Msg::Line(s, l)) => on_line(s, &l),
            Ok(Msg::Raw(b, t)) => raw = (b, t),
            Ok(Msg::Done) => open -= 1,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => open = 0,
        }
        if status.is_none() {
            status = child.try_wait()?;
        }
        if status.is_none() && last_check.elapsed() >= Duration::from_millis(100) {
            last_check = Instant::now();
            let late = deadline.is_some_and(|d| started.elapsed() >= d);
            if late || should_stop() {
                timed_out = late;
                stopped = true;
                child.kill_tree()?;
            }
        }
        if stopped && status.is_none() {
            // kill_tree reaped the child; there is no exit code to read.
            break;
        }
    }
    let _ = out.join();
    let _ = err.join();
    // Lines still in the channel after a stop are dropped on purpose.
    let exit = match status {
        Some(s) => match s.code() {
            Some(c) => Exit::Code(c),
            None => Exit::Killed,
        },
        None => Exit::Killed,
    };
    Ok(Streamed {
        exit,
        stopped,
        timed_out,
        stdout: raw.0,
        stdout_truncated: raw.1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_errors_tell_missing_from_blocked() {
        let missing = io::Error::from(io::ErrorKind::NotFound);
        assert_eq!(spawn_error_code(&missing), ErrorCode::ToolMissing);
        let denied = io::Error::from(io::ErrorKind::PermissionDenied);
        assert_eq!(spawn_error_code(&denied), ErrorCode::ToolBlocked);
        if cfg!(windows) {
            for os in [5, 225, 226, 1260] {
                let e = io::Error::from_raw_os_error(os);
                assert_eq!(
                    spawn_error_code(&e),
                    ErrorCode::ToolBlocked,
                    "os error {os}"
                );
            }
            assert_eq!(
                spawn_error_code(&io::Error::from_raw_os_error(2)),
                ErrorCode::ToolMissing
            );
        }
    }
}
