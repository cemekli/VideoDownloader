//! Table-driven test over the shared fixture `tests/fixtures/url-policy.json`.
//! The same fixture file is the single source of truth for expected results.

use serde::Deserialize;
use vv_domain::url_policy::validate_capture_url;

#[derive(Deserialize)]
struct Fixture {
    version: u32,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    input: String,
    expect: String,
    #[serde(default)]
    note: Option<String>,
}

#[test]
fn url_policy_fixture() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/url-policy.json"
    );
    let raw = std::fs::read_to_string(path).expect("fixture readable");
    let fx: Fixture = serde_json::from_str(&raw).expect("fixture valid JSON");
    assert_eq!(fx.version, 1);
    assert!(!fx.cases.is_empty());

    let mut failures = Vec::new();
    for c in &fx.cases {
        let got = match validate_capture_url(&c.input) {
            Ok(_) => "ok".to_string(),
            Err(e) => e.code().to_string(),
        };
        if got != c.expect {
            failures.push(format!(
                "input={:?} expected={} got={} note={}",
                c.input,
                c.expect,
                got,
                c.note.as_deref().unwrap_or("-")
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} case(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
