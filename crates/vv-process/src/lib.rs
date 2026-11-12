//! Child processes for managed tools (SPEC §13.2, §14.1; STATE.md F0.10,
//! ADR-0008).
//!
//! - **Clean environment.** A child gets a short allowlist of variables and
//!   whatever the caller sets explicitly; proxy, `DENO_*`, `PYTHON*` and all
//!   other user variables are dropped (ADR-0005, ADR-0007).
//! - **Whole tree.** yt-dlp starts Deno and may start FFmpeg. On Windows each
//!   child runs in its own Job Object with `KILL_ON_JOB_CLOSE`; the child is
//!   created suspended and only resumed once it is in the job, so nothing it
//!   starts can escape. When the app exits or crashes the OS closes the job
//!   handle and kills the whole tree: no orphan keeps writing to a target.
//!   On Unix the child leads its own process group (development only; a
//!   crash of the app does not kill the group there).
//! - **Timeout and cancel** kill the whole tree, then reap the child.
//! - **Low priority** ([`ManagedCommand::low_priority`]) for long CPU work
//!   such as transcription (F6.6): below-normal priority class on Windows,
//!   nice 10 on Unix, so the PC stays responsive.

mod env;
#[cfg(windows)]
mod windows;

pub use env::{base_env, BASE_ENV_KEYS};

use std::ffi::{OsStr, OsString};
use std::io;
use std::path::Path;
use std::process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// A command that will run with a clean environment inside its own job.
pub struct ManagedCommand {
    cmd: Command,
    low_priority: bool,
}

impl ManagedCommand {
    pub fn new(program: impl AsRef<OsStr>) -> ManagedCommand {
        let mut cmd = Command::new(program);
        cmd.env_clear();
        for (k, v) in base_env() {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::null());
        ManagedCommand {
            cmd,
            low_priority: false,
        }
    }

    pub fn arg(&mut self, a: impl AsRef<OsStr>) -> &mut Self {
        self.cmd.arg(a);
        self
    }

    pub fn args<I, S>(&mut self, a: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.cmd.args(a);
        self
    }

    /// An explicit variable, e.g. `DENO_DIR` pointing into the app cache.
    pub fn env(&mut self, k: impl AsRef<OsStr>, v: impl AsRef<OsStr>) -> &mut Self {
        self.cmd.env(k, v);
        self
    }

    pub fn current_dir(&mut self, d: impl AsRef<Path>) -> &mut Self {
        self.cmd.current_dir(d);
        self
    }

    /// Runs the child (and what it starts) below normal priority.
    pub fn low_priority(&mut self) -> &mut Self {
        self.low_priority = true;
        self
    }

    pub fn stdout(&mut self, s: Stdio) -> &mut Self {
        self.cmd.stdout(s);
        self
    }

    pub fn stderr(&mut self, s: Stdio) -> &mut Self {
        self.cmd.stderr(s);
        self
    }

    pub fn spawn(mut self) -> io::Result<Managed> {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let mut flags = windows::CREATE_SUSPENDED | windows::CREATE_NO_WINDOW;
            if self.low_priority {
                flags |= windows::BELOW_NORMAL_PRIORITY_CLASS;
            }
            self.cmd.creation_flags(flags);
            let mut child = self.cmd.spawn()?;
            match windows::Job::contain(&child) {
                Ok(job) => Ok(Managed {
                    child,
                    job,
                    done: false,
                }),
                Err(e) => {
                    // Never let a process run outside a job.
                    let _ = child.kill();
                    let _ = child.wait();
                    Err(e)
                }
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            self.cmd.process_group(0);
            let child = self.cmd.spawn()?;
            if self.low_priority {
                // Best effort: a failure leaves normal priority.
                // SAFETY: plain syscall on the child's pid.
                unsafe {
                    libc::setpriority(libc::PRIO_PROCESS, child.id() as libc::id_t, 10);
                }
            }
            Ok(Managed { child, done: false })
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Exited(ExitStatus),
    /// The deadline passed; the tree was killed.
    TimedOut,
}

/// A running child and everything it started. Dropping it kills the tree.
pub struct Managed {
    child: Child,
    #[cfg(windows)]
    job: windows::Job,
    done: bool,
}

impl Managed {
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }

    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.stderr.take()
    }

    /// Kills the child and every process it started, then reaps the child.
    pub fn kill_tree(&mut self) -> io::Result<()> {
        if self.done {
            return Ok(());
        }
        #[cfg(windows)]
        self.job.terminate()?;
        #[cfg(unix)]
        {
            // Negative PID: the whole process group the child leads.
            let pgid = self.child.id() as libc::pid_t;
            unsafe { libc::kill(-pgid, libc::SIGKILL) };
        }
        self.child.wait()?;
        self.done = true;
        Ok(())
    }

    /// Non-blocking check. Once the child has exited, whatever it started is
    /// killed too, as in [`Managed::wait_timeout`].
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let Some(status) = self.child.try_wait()? else {
            return Ok(None);
        };
        #[cfg(windows)]
        let _ = self.job.terminate();
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL)
        };
        self.done = true;
        Ok(Some(status))
    }

    /// Waits until the child exits or `timeout` passes (then kills the tree).
    pub fn wait_timeout(&mut self, timeout: Duration) -> io::Result<Outcome> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child.try_wait()? {
                // The child is gone; anything it left behind goes too.
                #[cfg(windows)]
                let _ = self.job.terminate();
                #[cfg(unix)]
                unsafe {
                    libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL)
                };
                self.done = true;
                return Ok(Outcome::Exited(status));
            }
            if Instant::now() >= deadline {
                self.kill_tree()?;
                return Ok(Outcome::TimedOut);
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

impl Drop for Managed {
    fn drop(&mut self) {
        let _ = self.kill_tree();
    }
}

/// Whether a process with this PID is still running (zombies count as gone).
pub fn is_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        windows::is_alive(pid)
    }
    #[cfg(unix)]
    {
        if unsafe { libc::kill(pid as libc::pid_t, 0) } != 0 {
            return false;
        }
        // A killed process whose parent has not reaped it yet is a zombie.
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(s) => s
                .rsplit_once(") ")
                .map(|(_, rest)| !rest.starts_with('Z') && !rest.starts_with('X'))
                .unwrap_or(true),
            Err(_) => true,
        }
    }
}

/// Convenience for tests and diagnostics: `KEY=value` for an `OsString` pair.
pub fn format_env(k: &OsStr, v: &OsStr) -> OsString {
    let mut s = k.to_os_string();
    s.push("=");
    s.push(v);
    s
}
