//! Library process lock (SPEC INV-13, §13.2; STATE.md F0.10, ADR-0008).
//!
//! The single-instance plugin focuses the running window, but it is not
//! enough on its own: a second copy started another way (a different exe
//! path, a crash leaving a half-dead instance, a portable copy) must still
//! not open the same database. So the data directory holds a lock file,
//! locked exclusively for the whole life of the process. The OS releases the
//! lock when the process ends, crash included, so a stale file never blocks.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const LOCK_FILE: &str = "videovault.lock";

#[derive(Debug)]
pub enum LockError {
    /// Another process holds the library. Its PID if it could be read.
    Held {
        pid: Option<u32>,
    },
    Io(std::io::Error),
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::Held { pid: Some(p) } => write!(f, "library is open in process {p}"),
            LockError::Held { pid: None } => write!(f, "library is open in another process"),
            LockError::Io(e) => write!(f, "cannot create the library lock: {e}"),
        }
    }
}

impl std::error::Error for LockError {}

/// Held for as long as the process uses the library. Dropping it unlocks.
#[derive(Debug)]
pub struct LibraryLock {
    _file: File,
    path: PathBuf,
}

impl LibraryLock {
    /// Takes the lock without waiting.
    pub fn acquire(data_dir: &Path) -> Result<LibraryLock, LockError> {
        std::fs::create_dir_all(data_dir).map_err(LockError::Io)?;
        let path = data_dir.join(LOCK_FILE);
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(LockError::Io)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(LockError::Held {
                    pid: read_pid(&path),
                })
            }
            Err(TryLockError::Error(e)) => return Err(LockError::Io(e)),
        }
        // For diagnostics only; the lock itself is what counts.
        file.set_len(0).map_err(LockError::Io)?;
        file.seek(SeekFrom::Start(0)).map_err(LockError::Io)?;
        write!(file, "{}", std::process::id()).map_err(LockError::Io)?;
        file.flush().map_err(LockError::Io)?;
        Ok(LibraryLock { _file: file, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Windows refuses reads of a byte range another process has locked, so the
/// PID is often unreadable there; that is fine, it is only a hint.
fn read_pid(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}
