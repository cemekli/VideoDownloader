//! ADR-0008, SPEC §13.2: clean environment, timeout and cancel kill the
//! whole tree, and (Windows) a crashed parent leaves no orphan.

use std::io::{BufRead, BufReader, Read};
#[cfg(windows)]
use std::process::Command;
use std::process::Stdio;
use std::time::{Duration, Instant};
use vv_process::{is_alive, ManagedCommand, Outcome};

const HELPER: &str = env!("CARGO_BIN_EXE_vv-process-helper");

fn gone_within(pid: u32, d: Duration) -> bool {
    let end = Instant::now() + d;
    while Instant::now() < end {
        if !is_alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    !is_alive(pid)
}

fn read_pid(line: &str, tag: &str) -> u32 {
    line.strip_prefix(tag)
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or_else(|| panic!("expected `{tag}<pid>`, got {line:?}"))
}

#[test]
fn environment_is_an_allowlist_plus_explicit_variables() {
    // Set in this process; the child must not see them.
    for k in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "NO_PROXY",
        "DENO_DIR",
        "PYTHONPATH",
        "VV_SECRET",
    ] {
        std::env::set_var(k, "leak");
    }
    let mut cmd = ManagedCommand::new(HELPER);
    cmd.arg("env")
        .env("VV_EXPLICIT", "yes")
        .stdout(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let mut out = String::new();
    child
        .take_stdout()
        .unwrap()
        .read_to_string(&mut out)
        .unwrap();
    assert!(
        matches!(child.wait_timeout(Duration::from_secs(30)).unwrap(), Outcome::Exited(s) if s.success())
    );
    let keys: Vec<&str> = out
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, _)| k)
        .collect();
    assert!(keys.contains(&"VV_EXPLICIT"), "{out}");
    for k in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "NO_PROXY",
        "DENO_DIR",
        "PYTHONPATH",
        "VV_SECRET",
    ] {
        assert!(
            !keys.iter().any(|x| x.eq_ignore_ascii_case(k)),
            "{k} leaked:\n{out}"
        );
    }
    for k in &keys {
        let allowed = *k == "VV_EXPLICIT"
            || k.eq_ignore_ascii_case("PATH")
            || vv_process::BASE_ENV_KEYS
                .iter()
                .any(|b| b.eq_ignore_ascii_case(k));
        assert!(allowed, "unexpected variable {k}:\n{out}");
    }
}

fn spawn_tree() -> (vv_process::Managed, u32) {
    let mut cmd = ManagedCommand::new(HELPER);
    cmd.arg("tree").stdout(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let mut lines = BufReader::new(child.take_stdout().unwrap()).lines();
    let g = read_pid(&lines.next().unwrap().unwrap(), "GRANDCHILD ");
    assert!(is_alive(g));
    (child, g)
}

#[test]
fn cancel_kills_the_grandchild_too() {
    let (mut child, g) = spawn_tree();
    let c = child.id();
    child.kill_tree().unwrap();
    assert!(gone_within(c, Duration::from_secs(5)), "child {c} survived");
    assert!(
        gone_within(g, Duration::from_secs(5)),
        "grandchild {g} survived the cancel"
    );
}

#[test]
fn timeout_kills_the_tree() {
    let (mut child, g) = spawn_tree();
    let t = Instant::now();
    assert_eq!(
        child.wait_timeout(Duration::from_millis(500)).unwrap(),
        Outcome::TimedOut
    );
    assert!(t.elapsed() < Duration::from_secs(10));
    assert!(
        gone_within(g, Duration::from_secs(5)),
        "grandchild {g} survived the timeout"
    );
}

#[test]
fn dropping_the_handle_kills_the_tree() {
    let (child, g) = spawn_tree();
    drop(child);
    assert!(
        gone_within(g, Duration::from_secs(5)),
        "grandchild {g} survived the drop"
    );
}

#[test]
fn normal_exit_is_reported() {
    let mut cmd = ManagedCommand::new(HELPER);
    cmd.args(["sleep", "0"]);
    let mut child = cmd.spawn().unwrap();
    assert!(
        matches!(child.wait_timeout(Duration::from_secs(30)).unwrap(), Outcome::Exited(s) if s.success())
    );
}

/// AC-38's other half: the app is killed (crash, Task Manager) and the OS
/// closes its job handle, which kills yt-dlp and whatever yt-dlp started.
/// Only Windows guarantees this; on Unix the group outlives the parent.
#[cfg(windows)]
#[test]
fn a_crashed_parent_leaves_no_orphan() {
    let mut sup = Command::new(HELPER)
        .arg("supervise")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(sup.stdout.take().unwrap()).lines();
    let c = read_pid(&lines.next().unwrap().unwrap(), "CHILD ");
    let g = read_pid(&lines.next().unwrap().unwrap(), "GRANDCHILD ");
    assert!(is_alive(c) && is_alive(g));
    // TerminateProcess: no destructor, no cleanup code runs in the parent.
    sup.kill().unwrap();
    sup.wait().unwrap();
    assert!(gone_within(c, Duration::from_secs(5)), "child {c} orphaned");
    assert!(
        gone_within(g, Duration::from_secs(5)),
        "grandchild {g} orphaned"
    );
}
