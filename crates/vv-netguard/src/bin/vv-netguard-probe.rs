//! Test helper for `scripts/netguard-probe.mjs` (ADR-0007): starts the guard,
//! prints `{"proxy": "<url>"}`, serves until stdin closes, then prints the
//! recorded events as JSON. `--test-target <ip:port>` registers a local test
//! server as reachable; the shipped app has no such switch.

use std::io::{BufRead, Write};
use vv_netguard::{Guard, Policy, SystemResolver};

fn main() {
    let mut policy = Policy::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--test-target" => policy
                .test_targets
                .push(args.next().expect("address").parse().expect("ip:port")),
            other => panic!("unknown argument {other}"),
        }
    }
    let guard = Guard::start(policy, Box::new(SystemResolver)).expect("start guard");
    let mut out = std::io::stdout();
    writeln!(out, "{}", serde_json::json!({ "proxy": guard.proxy_url() })).unwrap();
    out.flush().unwrap();
    for _ in std::io::stdin().lock().lines() {}
    writeln!(out, "{}", serde_json::json!({ "events": guard.events() })).unwrap();
}
