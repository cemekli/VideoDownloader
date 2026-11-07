//! STATE.md F1.3: table-driven test over `tests/fixtures/normalize.json`
//! (SPEC §6, §7.1 CAP-02/CAP-08, §11.7). The fixture is the single source of
//! truth for canonical forms; AC-36 and AC-52 cases live there too.

use serde::Deserialize;
use vv_domain::normalize::{normalize, NormalizeError, NORMALIZATION_VERSION};
use vv_domain::url_policy::validate_capture_url;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    version: u32,
    normalization_version: u32,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    input: String,
    #[serde(default)]
    expect: Option<Expect>,
    #[serde(default)]
    redacted: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Expect {
    platform: String,
    shape: String,
    source_id: Option<String>,
    canonical: String,
}

fn load() -> Fixture {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/normalize.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn shape_str(s: vv_domain::normalize::Shape) -> String {
    serde_json::to_value(s)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn normalization_fixture() {
    let fx = load();
    assert_eq!(fx.version, 1);
    assert_eq!(
        fx.normalization_version, NORMALIZATION_VERSION,
        "rules changed: bump NORMALIZATION_VERSION and the fixture together (§11.7)"
    );
    let mut failures = Vec::new();
    for c in &fx.cases {
        let url = validate_capture_url(&c.input).expect("fixture inputs pass the URL policy");
        match (normalize(&url), &c.expect, &c.error) {
            (Ok(n), Some(e), None) => {
                let got = (
                    n.platform.as_str().to_owned(),
                    shape_str(n.shape),
                    n.source_id.clone(),
                    n.canonical_url.clone(),
                );
                let want = (
                    e.platform.clone(),
                    e.shape.clone(),
                    e.source_id.clone(),
                    e.canonical.clone(),
                );
                if got != want {
                    failures.push(format!("{}: got {got:?}, want {want:?}", c.input));
                }
                let want_red = c
                    .redacted
                    .clone()
                    .unwrap_or_else(|| url.as_str().to_owned());
                if n.redacted_original != want_red {
                    failures.push(format!(
                        "{}: redacted {:?}, want {want_red:?}",
                        c.input, n.redacted_original
                    ));
                }
                if n.canonical_url.contains("CANARY") || n.redacted_original.contains("CANARY") {
                    failures.push(format!("{}: secret value leaked", c.input));
                }
                assert_eq!(n.version, NORMALIZATION_VERSION);
            }
            (Err(NormalizeError::SecretWithoutCanonical), None, Some(code)) => {
                assert_eq!(code, "URL_HAS_CREDENTIALS");
            }
            (got, _, _) => failures.push(format!("{}: unexpected {got:?}", c.input)),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Normalizing a canonical URL again gives the same canonical URL, so a
/// stored item never drifts on renormalization with unchanged rules.
#[test]
fn canonical_form_is_a_fixed_point() {
    for c in load().cases.iter().filter(|c| c.expect.is_some()) {
        let first = normalize(&validate_capture_url(&c.input).unwrap()).unwrap();
        let again = normalize(&validate_capture_url(&first.canonical_url).unwrap()).unwrap();
        assert_eq!(again.canonical_url, first.canonical_url, "{}", c.input);
        assert_eq!(again.source_id, first.source_id, "{}", c.input);
        assert_eq!(again.platform, first.platform, "{}", c.input);
    }
}
