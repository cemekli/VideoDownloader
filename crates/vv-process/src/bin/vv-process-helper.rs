//! Test helper for `tests/process.rs` (ADR-0008). Modes:
//! - `env`: prints its environment, one `KEY=value` per line;
//! - `sleep <secs>`;
//! - `tree`: starts `sleep 120` with a plain `Command` (as yt-dlp starts
//!   Deno or FFmpeg), prints `GRANDCHILD <pid>`, then sleeps;
//! - `supervise`: plays the app: starts `tree` through `ManagedCommand`,
//!   prints `CHILD <pid>` and `GRANDCHILD <pid>`, then sleeps until killed.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let me = std::env::current_exe().unwrap();
    let mut out = std::io::stdout();
    match args.first().map(String::as_str) {
        Some("env") => {
            for (k, v) in std::env::vars_os() {
                writeln!(out, "{}={}", k.to_string_lossy(), v.to_string_lossy()).unwrap();
            }
        }
        Some("sleep") => {
            let s = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(120);
            std::thread::sleep(Duration::from_secs(s));
        }
        Some("tree") => {
            // Left running on purpose: the test checks who kills it.
            #[allow(clippy::zombie_processes)]
            let g = Command::new(&me)
                .args(["sleep", "120"])
                .stdout(Stdio::null())
                .spawn()
                .unwrap();
            writeln!(out, "GRANDCHILD {}", g.id()).unwrap();
            out.flush().unwrap();
            std::thread::sleep(Duration::from_secs(120));
        }
        Some("supervise") => {
            let mut cmd = vv_process::ManagedCommand::new(&me);
            cmd.arg("tree").stdout(Stdio::piped());
            let mut child = cmd.spawn().unwrap();
            let mut lines = BufReader::new(child.take_stdout().unwrap()).lines();
            let g = lines.next().unwrap().unwrap();
            writeln!(out, "CHILD {}", child.id()).unwrap();
            writeln!(out, "{g}").unwrap();
            out.flush().unwrap();
            std::thread::sleep(Duration::from_secs(120));
            drop(child);
        }
        _ => panic!("unknown mode {args:?}"),
    }
}
