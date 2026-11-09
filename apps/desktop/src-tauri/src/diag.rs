//! Startup timing log (`<data dir>/logs/timing.log`), rewritten at every
//! launch. Written to find out why the window stalls after launch on
//! Cuma's machine: it holds event names, counts and milliseconds since the
//! process started, never paths, titles or URLs.

use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::Instant;

static START: LazyLock<Instant> = LazyLock::new(Instant::now);
static FILE: OnceLock<Mutex<std::fs::File>> = OnceLock::new();
static LINES: AtomicU32 = AtomicU32::new(0);
/// The log stops here so a long session cannot grow it without bound.
const MAX_LINES: u32 = 5_000;

/// Starts the clock; call first thing in `run`.
pub fn start() {
    LazyLock::force(&START);
}

/// Milliseconds since [`start`].
pub fn now_ms() -> u128 {
    START.elapsed().as_millis()
}

/// Opens the log under `data_dir/logs`, replacing the previous launch's.
pub fn open(data_dir: &Path) {
    let dir = data_dir.join("logs");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(f) = std::fs::File::create(dir.join("timing.log")) {
        let _ = FILE.set(Mutex::new(f));
        log(&format!(
            "log opened; version {}",
            env!("CARGO_PKG_VERSION")
        ));
    }
}

/// One line: `<ms> [<thread>] <event>`.
pub fn log(event: &str) {
    let Some(f) = FILE.get() else { return };
    if LINES.fetch_add(1, Ordering::Relaxed) >= MAX_LINES {
        return;
    }
    let t = std::thread::current();
    let line = format!("{:>8} [{}] {}\n", now_ms(), t.name().unwrap_or("?"), event);
    if let Ok(mut f) = f.lock() {
        let _ = f.write_all(line.as_bytes());
        let _ = f.flush();
    }
}

/// Times `f` and logs it when it took at least `min_ms`.
pub fn timed<T>(what: &str, min_ms: u128, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let r = f();
    let ms = t.elapsed().as_millis();
    if ms >= min_ms {
        log(&format!("{what}: {ms} ms"));
    }
    r
}
