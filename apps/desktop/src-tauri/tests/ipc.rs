//! IPC integration tests (STATE.md F0.3, SPEC §18 and the §20 IPC row).
//!
//! These go through Tauri's real invoke path: command routing, argument
//! deserialization, the ACL built from `capabilities/main.json` and origin
//! checks. Only the window system is mocked (`MockRuntime`), so they run on
//! every OS without a display.

use serde_json::{json, Value};
use std::collections::BTreeSet;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{App, WebviewWindow, WebviewWindowBuilder};

const CONTRACT: &str = include_str!("../../../../tests/fixtures/ipc-contract.json");

fn contract() -> Value {
    serde_json::from_str(CONTRACT).expect("ipc-contract.json is valid JSON")
}

fn app() -> App<MockRuntime> {
    vv_desktop::register(mock_builder())
        .build(tauri::generate_context!())
        .expect("app builds")
}

fn window(app: &App<MockRuntime>, label: &str) -> WebviewWindow<MockRuntime> {
    WebviewWindowBuilder::new(app, label, Default::default())
        .build()
        .expect("window builds")
}

/// The origin the bundled frontend has in production builds.
fn app_origin() -> &'static str {
    if cfg!(windows) {
        "http://tauri.localhost"
    } else {
        "tauri://localhost"
    }
}

fn invoke(
    w: &WebviewWindow<MockRuntime>,
    cmd: &str,
    args: Value,
    origin: &str,
) -> Result<Value, Value> {
    get_ipc_response(
        w,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: origin.parse().unwrap(),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|b| b.deserialize::<Value>().unwrap())
}

/// Same keys, and for each key the same JSON type. Values like `os` differ per
/// machine, so only the shape is compared.
fn assert_same_shape(actual: &Value, expected: &Value, what: &str) {
    let (a, e) = (actual.as_object().unwrap(), expected.as_object().unwrap());
    let keys = |m: &serde_json::Map<String, Value>| m.keys().cloned().collect::<BTreeSet<_>>();
    assert_eq!(
        keys(a),
        keys(e),
        "{what}: keys differ from ipc-contract.json"
    );
    for (k, v) in e {
        assert_eq!(
            std::mem::discriminant(&a[k]),
            std::mem::discriminant(v),
            "{what}.{k}: JSON type differs from ipc-contract.json"
        );
    }
}

#[test]
fn ping_round_trips_through_ipc() {
    let app = app();
    let w = window(&app, "main");
    let c = contract();
    let res = invoke(
        &w,
        "ping",
        c["commands"]["ping"]["args"].clone(),
        app_origin(),
    )
    .expect("ping allowed");
    assert_eq!(res, c["commands"]["ping"]["response"]);
}

#[test]
fn get_status_matches_contract_shape() {
    let app = app();
    let w = window(&app, "main");
    let c = contract();
    let res = invoke(&w, "get_status", json!({}), app_origin()).expect("get_status allowed");
    assert_same_shape(&res, &c["commands"]["get_status"]["response"], "get_status");
    assert_eq!(res["appName"], "VideoVault");
    assert_eq!(res["contractVersion"], c["contractVersion"]);
    assert_eq!(res["os"], std::env::consts::OS);
}

#[test]
fn domain_error_matches_contract_shape() {
    use vv_domain::error::{DomainError, ErrorCode};
    let c = contract();
    let e =
        serde_json::to_value(DomainError::new(ErrorCode::InternalError, "diag-00000000")).unwrap();
    assert_eq!(e, c["domainError"]);
}

#[test]
fn wrong_argument_type_is_rejected() {
    let app = app();
    let w = window(&app, "main");
    assert!(invoke(&w, "ping", json!({ "nonce": "not a number" }), app_origin()).is_err());
    assert!(invoke(&w, "ping", json!({}), app_origin()).is_err());
}

#[test]
fn unknown_command_is_rejected() {
    let app = app();
    let w = window(&app, "main");
    assert!(invoke(
        &w,
        "read_file",
        json!({ "path": "C:/Windows/win.ini" }),
        app_origin()
    )
    .is_err());
}

#[test]
fn remote_origin_cannot_call_commands() {
    // SPEC §10.1: arbitrary remote pages must not reach privileged commands.
    let app = app();
    let w = window(&app, "main");
    let err = invoke(
        &w,
        "ping",
        json!({ "nonce": 1 }),
        "https://attacker.example",
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("not allowed"),
        "unexpected error: {err}"
    );
}

#[test]
fn other_windows_get_no_commands() {
    // Capabilities are bound to the "main" window label only.
    let app = app();
    let w = window(&app, "untrusted");
    assert!(invoke(&w, "ping", json!({ "nonce": 1 }), app_origin()).is_err());
}

#[test]
fn main_capability_grants_only_app_commands() {
    // A plugin or core permission (fs, shell, http, opener...) or a `remote`
    // URL list here would widen the attack surface; that needs a SPEC change.
    let cap: Value = serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
    assert_eq!(cap["windows"], json!(["main"]));
    assert!(
        cap.get("remote").is_none(),
        "remote origins must not get capabilities"
    );
    let perms: BTreeSet<String> = cap["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().expect("plain permission id").to_owned())
        .collect();
    let expected: BTreeSet<String> = ["allow-ping", "allow-get-status"].map(String::from).into();
    assert_eq!(perms, expected);
}

#[test]
fn csp_allows_no_remote_sources() {
    // SPEC §18.2: the webview loads no remote images, media, fonts or scripts.
    let conf: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    let security = &conf["app"]["security"];
    assert_eq!(security["freezePrototype"], true);
    for key in ["csp", "devCsp"] {
        let csp = security[key]
            .as_object()
            .unwrap_or_else(|| panic!("{key} must be set"));
        for (directive, value) in csp {
            let v = value.as_str().unwrap();
            assert!(
                !v.contains("https:") && !v.contains("http://*") && !v.contains('*'),
                "{key} {directive} allows remote: {v}"
            );
            assert!(!v.contains("unsafe-eval"), "{key} {directive} allows eval");
        }
        assert_eq!(csp["script-src"], "'self'", "{key} script-src");
        assert_eq!(csp["object-src"], "'none'", "{key} object-src");
    }
}
