//! Library lock (INV-13, AC-38; ADR-0008): a second holder is refused, in
//! the same process and in another one, and a killed holder frees the lock.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use vv_persistence::lock::{LibraryLock, LockError, LOCK_FILE};

const CHILD_ENV: &str = "VV_LOCK_TEST_CHILD_DIR";

/// Not a real test on its own: when started by `another_process_*` with
/// CHILD_ENV set, takes the lock, says so and waits to be killed.
#[test]
fn lock_holder_child() {
    let Some(dir) = std::env::var_os(CHILD_ENV) else {
        // Started without a directory by child_processes_do_not_inherit_the_lock:
        // just stay alive for a while.
        if std::env::args().any(|a| a == "lock_holder_child") {
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
        return;
    };
    let _lock = LibraryLock::acquire(std::path::Path::new(&dir)).expect("child takes the lock");
    println!("VV-LOCKED");
    std::thread::sleep(std::time::Duration::from_secs(120));
}

#[test]
fn second_holder_in_the_same_process_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let first = LibraryLock::acquire(dir.path()).unwrap();
    assert!(first.path().ends_with(LOCK_FILE));
    assert!(matches!(
        LibraryLock::acquire(dir.path()),
        Err(LockError::Held { .. })
    ));
    drop(first);
    acquire_soon(dir.path()).expect("free again after drop");
}

/// Another test may be spawning a process at this moment: until that child
/// execs, it briefly shares our (close-on-exec) descriptors, lock included.
fn acquire_soon(dir: &std::path::Path) -> Result<LibraryLock, LockError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match LibraryLock::acquire(dir) {
            Err(LockError::Held { .. }) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20))
            }
            r => return r,
        }
    }
}

#[test]
fn child_processes_do_not_inherit_the_lock() {
    // yt-dlp and FFmpeg run while the app holds the lock. If they inherited
    // it, an orphaned child would keep the library locked after the app died.
    let dir = tempfile::tempdir().unwrap();
    let lock = LibraryLock::acquire(dir.path()).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lock_holder_child", "--nocapture"])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    drop(lock);
    acquire_soon(dir.path()).expect("the running child does not hold the lock");
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn another_process_holding_the_lock_blocks_us_until_it_dies() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "lock_holder_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ENV, dir.path())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    while !line.contains("VV-LOCKED") {
        line.clear();
        assert!(out.read_line(&mut line).unwrap() > 0, "child exited early");
    }
    match LibraryLock::acquire(dir.path()) {
        Err(LockError::Held { pid }) => {
            // Linux can read the PID; Windows may refuse to read a locked file.
            if let Some(p) = pid {
                assert_eq!(p, child.id());
            }
        }
        other => panic!("expected Held, got {other:?}"),
    }
    // A crash: the OS drops the lock with the process. The file stays behind
    // and must not block the next start.
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(dir.path().join(LOCK_FILE).exists());
    acquire_soon(dir.path()).expect("lock is free after the holder died");
}
