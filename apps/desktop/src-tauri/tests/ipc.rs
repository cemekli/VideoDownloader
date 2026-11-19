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
fn get_diagnostics_matches_contract_and_reports_fts5() {
    let app = app();
    let w = window(&app, "main");
    let c = contract();
    let res =
        invoke(&w, "get_diagnostics", json!({}), app_origin()).expect("get_diagnostics allowed");
    assert_same_shape(
        &res,
        &c["commands"]["get_diagnostics"]["response"],
        "get_diagnostics",
    );
    assert_eq!(res["sqliteFts5"], true);
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
    // Listening is needed for the import event (ADR-0013); emitting from the
    // webview, dialogs and file access are not granted.
    let expected: BTreeSet<String> = [
        "allow-ping",
        "allow-get-status",
        "allow-get-diagnostics",
        "allow-list-items",
        "allow-get-item",
        "allow-import-media",
        "allow-check-url",
        "allow-save-url",
        "allow-restore-item",
        "allow-get-sidebar",
        "allow-undo",
        "allow-tag-items",
        "allow-untag-items",
        "allow-add-to-collection",
        "allow-remove-from-collection",
        "allow-set-favorite",
        "allow-set-archived",
        "allow-remove-items",
        "allow-restore-items",
        "allow-delete-items-permanently",
        "allow-rename-tag",
        "allow-merge-tags",
        "allow-delete-tag",
        "allow-create-collection",
        "allow-rename-collection",
        "allow-move-collection",
        "allow-delete-collection",
        "allow-save-smart-collection",
        "allow-delete-smart-collection",
        "allow-set-notes",
        "allow-list-duplicates",
        "allow-preview-merge",
        "allow-merge-items",
        "allow-dismiss-duplicate",
        "allow-list-orphans",
        "allow-reimport-orphan",
        "allow-delete-orphan",
        "allow-get-settings",
        "allow-update-settings",
        "allow-acknowledge-legal",
        "allow-finish-onboarding",
        "allow-get-storage-overview",
        "allow-choose-media-folder",
        "allow-export-library",
        "allow-preview-library-import",
        "allow-map-import-root",
        "allow-apply-library-import",
        "allow-cancel-pending",
        "allow-backup-library",
        "allow-list-backups",
        "allow-inspect-backup",
        "allow-restore-backup",
        "allow-get-playback",
        "allow-save-playback-position",
        "allow-open-externally",
        "allow-open-source",
        "allow-relink-asset",
        "allow-get-tool-health",
        "allow-list-jobs",
        "allow-cancel-job",
        "allow-retry-job",
        "allow-remove-partial",
        "allow-clear-finished-downloads",
        "allow-clear-finished-ai-jobs",
        "allow-get-download-options",
        "allow-start-download",
        "allow-fetch-metadata",
        "allow-get-storage-usage",
        "allow-set-storage-quota",
        "allow-move-media-folder",
        "allow-list-storage-roots",
        "allow-locate-storage-root",
        "allow-list-sessions",
        "allow-connect-session",
        "allow-set-session-consent",
        "allow-clear-session",
        "allow-delete-session",
        "allow-connect-session-file",
        "allow-reconnect-session",
        "allow-verify-session",
        "allow-open-sign-in",
        "allow-start-app-sign-in",
        "allow-finish-app-sign-in",
        "allow-cancel-app-sign-in",
        "allow-list-models",
        "allow-download-model",
        "allow-verify-model",
        "allow-remove-model",
        "allow-get-ai-processing",
        "allow-get-search-index",
        "allow-set-embedding-api",
        "allow-clear-embedding-api",
        "allow-build-search-index",
        "allow-cancel-search-index",
        "allow-get-tag-suggest",
        "allow-set-suggest-api",
        "allow-clear-suggest-api",
        "allow-suggest-all",
        "allow-cancel-suggest-all",
        "allow-list-ai-proposals",
        "allow-suggest-tags",
        "allow-decide-ai-proposal",
        "allow-list-ai-proposal-groups",
        "allow-decide-ai-proposals",
        "allow-get-screen-text",
        "allow-read-screen-text",
        "allow-cancel-screen-text",
        "allow-list-transcripts",
        "allow-get-transcript",
        "allow-fetch-captions",
        "allow-transcribe-locally",
        "allow-delete-transcript",
        "allow-get-moment",
        "allow-preview-note",
        "allow-export-note",
        "core:event:allow-listen",
        "core:event:allow-unlisten",
    ]
    .map(String::from)
    .into();
    assert_eq!(perms, expected);
}

#[test]
fn csp_allows_no_remote_sources() {
    // SPEC §18.2: the webview loads no remote images, media, fonts or scripts.
    let conf: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    let security = &conf["app"]["security"];
    // Tauri's freezePrototype would also be injected into the in-app sign-in
    // window and break the sites there (ADR-0025); the app's own page freezes
    // Object.prototype itself, in its first script.
    assert_eq!(security["freezePrototype"], false);
    let html = include_str!("../../index.html");
    let first_script = html.find("<script").expect("index.html has scripts");
    assert!(
        html.get(first_script..)
            .unwrap_or_default()
            .starts_with(r#"<script src="/freeze-prototype.js"></script>"#),
        "freeze-prototype.js must be the first script"
    );
    assert!(include_str!("../../public/freeze-prototype.js")
        .contains("Object.freeze(Object.prototype);"));
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
        // Thumbnails come only from the local thumbnail protocol (ADR-0013).
        assert_eq!(
            csp["img-src"], "'self' vvthumb: http://vvthumb.localhost",
            "{key} img-src"
        );
        assert_eq!(csp["object-src"], "'none'", "{key} object-src");
    }
}

// ---- Library commands (STATE.md F1.7) ----

use vv_desktop::library::LibraryState;
use vv_persistence::library::repo::{self, LibraryItem};
use vv_persistence::library::{ItemKind, SourcePlatform};
use vv_persistence::{Database, OpenOptions};

/// A mock app whose library is open in a temp dir with one bookmark titled
/// "Deniz kenarında sabah". Returns the item id.
fn app_with_library(dir: &std::path::Path) -> (App<MockRuntime>, String) {
    let app = app();
    app.state::<LibraryState>().open(dir, None).unwrap();
    let mut db = Database::open(&dir.join("db/library.sqlite"), &OpenOptions::default()).unwrap();
    let mut it = LibraryItem::new(
        ItemKind::Bookmark,
        SourcePlatform::Youtube,
        1_794_000_000_000,
    );
    it.canonical_url = Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ".into());
    it.title = Some("Deniz kenarında sabah".into());
    it.duration_ms = Some(372_000);
    repo::insert_item(db.writer(), &it).unwrap();
    (app, it.id)
}

use tauri::Manager;
use vv_persistence::vectors;

#[test]
fn list_items_matches_contract_shape() {
    let d = tempfile::tempdir().unwrap();
    let (app, id) = app_with_library(d.path());
    let w = window(&app, "main");
    let c = contract();
    let spec = &c["commands"]["list_items"];
    let res =
        invoke(&w, "list_items", spec["args"].clone(), app_origin()).expect("list_items allowed");
    assert_same_shape(&res, &spec["response"], "list_items");
    assert_eq!(res["total"], 1);
    let item = &res["items"][0];
    assert_same_shape(item, &spec["response"]["items"][0], "list_items.items[0]");
    assert_same_shape(
        &item["snippet"],
        &spec["response"]["items"][0]["snippet"],
        "list_items.items[0].snippet",
    );
    assert_eq!(item["id"], json!(id));
    assert_eq!(item["localState"], "bookmark_only");
    assert_eq!(
        item["snippet"]["parts"][0],
        json!({"text": "Deniz", "highlight": true})
    );
    // No embedding API: the word search, and why meaning is off.
    assert_eq!(res["meaning"], "no_api");
    assert!(item["meaning"].is_null() && item["foundBy"].is_null());
}

/// A one-topic embedding API on 127.0.0.1: texts about the sea or water
/// get the same vector, anything else another.
fn tiny_embedding_api() -> String {
    use std::io::{BufRead, BufReader, Read, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}/v1", l.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut len = 0usize;
            let mut line = String::new();
            loop {
                line.clear();
                if r.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; len];
            let _ = r.read_exact(&mut body);
            let v: Value = serde_json::from_slice(&body).unwrap_or_default();
            let data: Vec<Value> = v["input"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|t| {
                    let t = t.as_str().unwrap_or("").to_lowercase();
                    let mut e = vec![0.0f32; 8];
                    let sea = ["deniz", "sea", "dalga", "kıyı", "water"]
                        .iter()
                        .any(|w| t.contains(w));
                    e[if sea { 0 } else { 1 }] = 1.0;
                    json!({ "embedding": e })
                })
                .collect();
            let out = json!({ "data": data }).to_string();
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{out}",
                out.len()
            );
        }
    });
    url
}

#[test]
fn list_items_adds_matches_by_meaning() {
    // F7.2, SEA-07, AC-40: the first answer is the word search and says
    // meaning is available; with `meaning: true` the result is hybrid and
    // each item says how it was found.
    use vv_persistence::transcripts::{self, CaptionKind, NewSegment, Origin, Provenance, Status};
    let d = tempfile::tempdir().unwrap();
    let (app, deniz) = app_with_library(d.path());
    let w = window(&app, "main");
    let url = tiny_embedding_api();
    let set = ok(
        &w,
        "set_embedding_api",
        json!({ "url": format!("{url}/"), "model": "tiny", "key": null, "allowRemote": false }),
    );
    assert_eq!(set["api"]["url"], json!(url), "normalized");
    assert_eq!(set["api"]["dims"], 8);
    assert_eq!(set["api"]["local"], true);
    assert_eq!(set["api"]["hasKey"], false);

    // A second video that never says "deniz" but is about the sea.
    let mut db =
        Database::open(&d.path().join("db/library.sqlite"), &OpenOptions::default()).unwrap();
    let mut other = LibraryItem::new(
        ItemKind::Bookmark,
        SourcePlatform::Youtube,
        1_794_000_000_001,
    );
    other.canonical_url = Some("https://www.youtube.com/watch?v=aaaaaaaaaaa".into());
    other.title = Some("Hafta sonu".into());
    repo::insert_item(db.writer(), &other).unwrap();
    let tx = db.writer().transaction().unwrap();
    let origin = Origin {
        item_id: other.id.clone(),
        asset_id: None,
        language: "tr".into(),
        provenance: Provenance::SourceCaptions,
        provider: "source_captions".into(),
        model_id: None,
        model_version: None,
        source_fingerprint: "tr:manual:sha256:01".into(),
        caption_kind: Some(CaptionKind::Manual),
    };
    transcripts::save(
        &tx,
        &origin,
        Status::Ready,
        None,
        &[NewSegment::new(5_000, 8_000, "Dalgalar kıyıya vuruyor.")],
        None,
        1,
    )
    .unwrap();
    tx.commit().unwrap();
    // The index, as a finished build would leave it.
    {
        let c = db.writer();
        let g = vectors::begin_build(
            c,
            &vectors::Producer {
                provider: "openai_compatible".into(),
                model_id: "tiny".into(),
                model_version: url.clone(),
                dims: 8,
            },
            "g1",
            1,
        )
        .unwrap();
        let mut sea = vec![0.0f32; 8];
        sea[0] = 1.0;
        for ch in vectors::pending(c, g.seq, 10).unwrap() {
            vectors::put(c, &g, ch.chunk_seq, &ch.text, &sea).unwrap();
        }
        vectors::activate(c, "g1", 2).unwrap();
    }
    let q = |meaning: bool| {
        json!({ "query": { "text": "deniz", "view": "all", "platforms": [], "kinds": [],
            "sort": "relevance", "descending": true, "offset": 0, "limit": 100, "meaning": meaning } })
    };
    let words = ok(&w, "list_items", q(false));
    assert_eq!(words["meaning"], "available");
    assert_eq!(words["total"], 1);
    let both = ok(&w, "list_items", q(true));
    assert_eq!(both["meaning"], "used");
    assert_eq!(both["total"], 2);
    assert_eq!(both["items"][0]["id"], json!(deniz));
    assert_eq!(
        both["items"][0]["foundBy"],
        json!({ "words": 1, "meaning": null })
    );
    let m = &both["items"][1];
    assert_eq!(m["id"], json!(other.id));
    assert_eq!(m["foundBy"], json!({ "words": null, "meaning": 1 }));
    assert_eq!(m["meaning"]["moments"][0]["startMs"], 5_000);
    assert_eq!(m["meaning"]["moments"][0]["snippet"]["field"], "meaning");
    let c = contract();
    let shape = &c["commands"]["list_items"]["meaningShape"];
    assert_same_shape(
        &m["meaning"],
        &with_nulls_of(&m["meaning"], &shape["meaning"]),
        "list_items meaning",
    );
    // Another sort: meaning does not apply.
    let mut added = q(true);
    added["query"]["sort"] = json!("added");
    assert!(ok(&w, "list_items", added)["meaning"].is_null());
    // The API forgotten: words only, lexical search unchanged (AC-40).
    ok(&w, "clear_embedding_api", json!({}));
    let after = ok(&w, "list_items", q(true));
    assert_eq!(after["meaning"], "no_api");
    assert_eq!(after["total"], 1);
}

#[test]
fn get_item_matches_contract_shape() {
    let d = tempfile::tempdir().unwrap();
    let (app, id) = app_with_library(d.path());
    let w = window(&app, "main");
    let c = contract();
    let spec = &c["commands"]["get_item"];
    let res = invoke(&w, "get_item", json!({ "id": id }), app_origin()).expect("get_item allowed");
    let mut expected = spec["response"].clone();
    // The test item has no media; the contract's asset row is checked by
    // the unit tests of `item_detail`.
    expected["assets"] = json!([]);
    assert_same_shape(&res, &expected, "get_item");
    assert_same_shape(&res["item"], &spec["response"]["item"], "get_item.item");
    assert_eq!(res["assets"], json!([]));
    let missing = invoke(
        &w,
        "get_item",
        json!({ "id": "00000000-0000-4000-8000-00000000ffff" }),
        app_origin(),
    )
    .unwrap_err();
    assert_eq!(missing["code"], "INTERNAL_ERROR");
}

#[test]
fn list_items_refuses_unknown_values() {
    let d = tempfile::tempdir().unwrap();
    let (app, _) = app_with_library(d.path());
    let w = window(&app, "main");
    for query in [
        json!({ "view": "everything" }),
        json!({ "sort": "random()" }),
        json!({ "platforms": ["myspace"] }),
        json!({ "extra": 1 }),
    ] {
        assert!(
            invoke(&w, "list_items", json!({ "query": query }), app_origin()).is_err(),
            "{query}"
        );
    }
}

#[test]
fn library_commands_fail_cleanly_before_the_library_opens() {
    let app = app();
    let w = window(&app, "main");
    let err = invoke(&w, "list_items", json!({ "query": {} }), app_origin()).unwrap_err();
    assert_eq!(err["code"], "INTERNAL_ERROR");
    assert_eq!(err["diagnosticId"], "library-not-open");
}

#[test]
fn import_without_tools_reports_tool_missing() {
    // Checked before any dialog opens.
    let d = tempfile::tempdir().unwrap();
    let (app, _) = app_with_library(d.path());
    let w = window(&app, "main");
    let err = invoke(
        &w,
        "import_media",
        json!({ "source": "files" }),
        app_origin(),
    )
    .unwrap_err();
    assert_eq!(err["code"], "TOOL_MISSING");
    assert!(invoke(&w, "import_media", json!({ "source": "C:/" }), app_origin()).is_err());
}

// ---- Add URL (STATE.md F1.8) ----

#[test]
fn add_url_commands_match_contract_shape() {
    let d = tempfile::tempdir().unwrap();
    let (app, _) = app_with_library(d.path());
    let w = window(&app, "main");
    let c = contract();
    let url = c["commands"]["save_url"]["args"].clone();

    let fresh = invoke(&w, "check_url", url.clone(), app_origin()).expect("check_url allowed");
    assert_eq!(fresh["existing"], Value::Null);
    assert_eq!(fresh["title"], "YouTube · vvTestVid01");

    let saved = invoke(&w, "save_url", url.clone(), app_origin()).expect("save_url allowed");
    assert_same_shape(&saved, &c["commands"]["save_url"]["response"], "save_url");
    assert_same_shape(
        &saved["item"],
        &c["commands"]["save_url"]["response"]["item"],
        "save_url.item",
    );
    assert_eq!(saved["outcome"], "created");

    let check = invoke(&w, "check_url", url.clone(), app_origin()).unwrap();
    assert_same_shape(&check, &c["commands"]["check_url"]["response"], "check_url");
    assert_same_shape(
        &check["existing"],
        &c["commands"]["check_url"]["response"]["existing"],
        "check_url.existing",
    );
    assert_eq!(check["existing"]["item"]["id"], saved["item"]["id"]);

    let again = invoke(&w, "save_url", url, app_origin()).unwrap();
    assert_eq!(again["outcome"], "existing");
    assert_eq!(again["matchedBy"], "canonical_url");

    let restored = invoke(
        &w,
        "restore_item",
        json!({ "id": saved["item"]["id"] }),
        app_origin(),
    )
    .expect("restore_item allowed");
    assert_same_shape(
        &restored,
        &c["commands"]["restore_item"]["response"],
        "restore_item",
    );
}

#[test]
fn add_url_refusals_are_domain_errors_without_the_input() {
    let d = tempfile::tempdir().unwrap();
    let (app, _) = app_with_library(d.path());
    let w = window(&app, "main");
    for cmd in ["check_url", "save_url"] {
        let err = invoke(
            &w,
            cmd,
            json!({ "url": "https://user:hunter2@example.com/v" }),
            app_origin(),
        )
        .unwrap_err();
        assert_eq!(err["code"], "URL_HAS_CREDENTIALS", "{cmd}");
        assert!(!err.to_string().contains("hunter2"), "{cmd}: {err}");
        assert!(invoke(&w, cmd, json!({ "url": 7 }), app_origin()).is_err());
    }
}

// ---- Organize (STATE.md F2.1–F2.3) ----

fn ok(w: &WebviewWindow<MockRuntime>, cmd: &str, args: Value) -> Value {
    invoke(w, cmd, args, app_origin()).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}

#[test]
fn organize_commands_match_contract_shape() {
    let d = tempfile::tempdir().unwrap();
    let (app, id) = app_with_library(d.path());
    let w = window(&app, "main");
    let c = contract();
    let spec = |cmd: &str| c["commands"][cmd]["response"].clone();
    let other = ok(&w, "save_url", c["commands"]["save_url"]["args"].clone())["item"]["id"].clone();
    let both = json!([id, other]);

    let col = ok(
        &w,
        "create_collection",
        json!({ "name": "Gezi", "parentId": null }),
    );
    assert_same_shape(&col, &spec("create_collection"), "create_collection");
    let sub = ok(
        &w,
        "create_collection",
        json!({ "name": "Deniz", "parentId": col["id"] }),
    );

    let tagged = ok(&w, "tag_items", json!({ "itemIds": both, "name": "deniz" }));
    assert_same_shape(&tagged, &spec("tag_items"), "tag_items");
    assert_same_shape(
        &tagged["undo"],
        &spec("tag_items")["undo"],
        "tag_items.undo",
    );
    assert_eq!(tagged["changed"], 2);
    let added = ok(
        &w,
        "add_to_collection",
        json!({ "itemIds": [id], "collectionId": col["id"] }),
    );
    assert_same_shape(&added, &spec("add_to_collection"), "add_to_collection");

    let sb = ok(&w, "get_sidebar", json!({}));
    let tag = sb["tags"][0]["id"].clone();
    let mut rules = c["commands"]["save_smart_collection"]["args"]["rules"].clone();
    rules["tagIds"] = json!([tag]);
    let smart = ok(
        &w,
        "save_smart_collection",
        json!({ "id": null, "name": "Yaz", "rules": rules }),
    );
    assert_same_shape(
        &smart,
        &spec("save_smart_collection"),
        "save_smart_collection",
    );
    let mut bad = rules.clone();
    bad["sql"] = json!("DROP TABLE tag");
    assert!(invoke(
        &w,
        "save_smart_collection",
        json!({ "id": null, "name": "x", "rules": bad }),
        app_origin()
    )
    .is_err());

    let sb = ok(&w, "get_sidebar", json!({}));
    let s = spec("get_sidebar");
    assert_same_shape(&sb, &s, "get_sidebar");
    assert_same_shape(
        &sb["collections"][0],
        &s["collections"][0],
        "get_sidebar.collections[0]",
    );
    assert_same_shape(
        &sb["collections"][1],
        &s["collections"][1],
        "get_sidebar.collections[1]",
    );
    assert_eq!(sb["collections"][1]["depth"], 1);
    assert_same_shape(
        &sb["smartCollections"][0],
        &s["smartCollections"][0],
        "get_sidebar.smartCollections[0]",
    );
    assert_same_shape(
        &sb["smartCollections"][0]["rules"],
        &s["smartCollections"][0]["rules"],
        "rules",
    );
    assert_same_shape(&sb["tags"][0], &s["tags"][0], "get_sidebar.tags[0]");
    assert_same_shape(&sb["counts"], &s["counts"], "get_sidebar.counts");
    assert_eq!(sb["counts"]["inbox"], 1);

    let list = |q: Value| ok(&w, "list_items", json!({ "query": q }))["total"].clone();
    assert_eq!(list(json!({ "smartCollectionId": smart["id"] })), 2);
    assert_eq!(list(json!({ "view": "inbox" })), 1);
    assert_eq!(list(json!({ "tagIds": [tag] })), 2);
    assert_eq!(list(json!({ "collectionId": col["id"] })), 1);
    assert_eq!(list(json!({ "text": "deniz" })), 2, "tags are searchable");

    let det = ok(&w, "get_item", json!({ "id": id }));
    let mut expected = spec("get_item");
    expected["assets"] = json!([]);
    assert_same_shape(&det, &expected, "get_item");
    assert_same_shape(
        &det["tagRefs"][0],
        &spec("get_item")["tagRefs"][0],
        "get_item.tagRefs[0]",
    );
    let noted = ok(
        &w,
        "set_notes",
        json!({ "id": id, "revision": det["revision"], "notes": "izlenecek" }),
    );
    let mut expected = spec("set_notes");
    expected["assets"] = json!([]);
    assert_same_shape(&noted, &expected, "set_notes");
    assert_eq!(noted["notes"], "izlenecek");
    let stale = invoke(
        &w,
        "set_notes",
        json!({ "id": id, "revision": det["revision"], "notes": "x" }),
        app_origin(),
    )
    .unwrap_err();
    assert_eq!(stale["code"], "REVISION_CONFLICT");

    for (cmd, args) in [
        ("set_favorite", json!({ "itemIds": both, "value": true })),
        ("set_archived", json!({ "itemIds": both, "value": true })),
    ] {
        assert_same_shape(&ok(&w, cmd, args), &spec(cmd), cmd);
    }
    let undone = ok(&w, "undo", json!({}));
    assert_same_shape(&undone, &spec("undo"), "undo");
    assert_eq!(undone["kind"], "set_archived");
    assert_eq!(undone["applied"], 2);

    let renamed = ok(
        &w,
        "rename_tag",
        json!({ "id": tag, "name": "deniz kenarı" }),
    );
    assert_same_shape(&renamed, &spec("rename_tag"), "rename_tag");
    ok(&w, "tag_items", json!({ "itemIds": [id], "name": "sahil" }));
    let sahil = ok(&w, "get_sidebar", json!({}))["tags"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "sahil")
        .unwrap()["id"]
        .clone();
    let merged = ok(
        &w,
        "merge_tags",
        json!({ "sourceId": sahil, "targetId": tag }),
    );
    assert_same_shape(&merged, &spec("merge_tags"), "merge_tags");
    let untag = ok(&w, "untag_items", json!({ "itemIds": [id], "tagId": tag }));
    assert_same_shape(&untag, &spec("untag_items"), "untag_items");
    let out = ok(
        &w,
        "remove_from_collection",
        json!({ "itemIds": [id], "collectionId": col["id"] }),
    );
    assert_same_shape(
        &out,
        &spec("remove_from_collection"),
        "remove_from_collection",
    );
    let gone = ok(&w, "delete_tag", json!({ "id": tag }));
    assert_same_shape(&gone, &spec("delete_tag"), "delete_tag");

    assert_eq!(
        ok(
            &w,
            "rename_collection",
            json!({ "id": col["id"], "name": "Geziler" })
        ),
        Value::Null
    );
    assert_eq!(
        ok(
            &w,
            "move_collection",
            json!({ "id": sub["id"], "parentId": null })
        ),
        Value::Null
    );
    let cycle = invoke(
        &w,
        "move_collection",
        json!({ "id": col["id"], "parentId": col["id"] }),
        app_origin(),
    )
    .unwrap_err();
    assert_eq!(cycle["diagnosticId"], "collection-cycle");
    assert!(invoke(
        &w,
        "delete_collection",
        json!({ "id": col["id"], "policy": "rm -rf" }),
        app_origin()
    )
    .is_err());
    assert_eq!(
        ok(
            &w,
            "delete_collection",
            json!({ "id": col["id"], "policy": "delete_all" })
        ),
        1
    );
    assert_eq!(
        ok(&w, "delete_smart_collection", json!({ "id": smart["id"] })),
        Value::Null
    );

    // Duplicates: merge, then Undo brings both back.
    let cand = {
        let mut db =
            Database::open(&d.path().join("db/library.sqlite"), &OpenOptions::default()).unwrap();
        repo::open_duplicate_candidate(
            db.writer(),
            &id,
            other.as_str().unwrap(),
            vv_persistence::library::DuplicateReason::SimilarUrl,
            1,
        )
        .unwrap()
        .id
    };
    let dups = ok(&w, "list_duplicates", json!({}));
    assert_same_shape(&dups[0], &spec("list_duplicates")[0], "list_duplicates[0]");
    // The contract's item is the saved bookmark (no duration yet).
    let saved = if dups[0]["a"]["id"] == other {
        &dups[0]["a"]
    } else {
        &dups[0]["b"]
    };
    assert_same_shape(
        saved,
        &spec("list_duplicates")[0]["a"],
        "list_duplicates[0].a",
    );
    let pv = ok(
        &w,
        "preview_merge",
        json!({ "candidateId": cand, "targetId": other }),
    );
    let mut expected = spec("preview_merge");
    expected["aliasUrl"] = pv["aliasUrl"].clone();
    assert_same_shape(&pv, &expected, "preview_merge");
    let m = ok(
        &w,
        "merge_items",
        json!({ "candidateId": cand, "targetId": other }),
    );
    assert_same_shape(&m, &spec("merge_items"), "merge_items");
    assert_eq!(list(json!({ "view": "removed" })), 1);
    assert_eq!(ok(&w, "undo", json!({}))["kind"], "merge_items");
    assert_eq!(list(json!({ "view": "removed" })), 0);
    assert_eq!(
        ok(&w, "dismiss_duplicate", json!({ "candidateId": cand })),
        Value::Null
    );
    assert_eq!(ok(&w, "list_duplicates", json!({})), json!([]));

    // Remove, restore, remove again and delete for good.
    let removed = ok(&w, "remove_items", json!({ "itemIds": [id] }));
    assert_same_shape(&removed, &spec("remove_items"), "remove_items");
    let restored = ok(&w, "restore_items", json!({ "itemIds": [id] }));
    assert_same_shape(&restored, &spec("restore_items"), "restore_items");
    ok(&w, "remove_items", json!({ "itemIds": [id] }));
    let del = ok(
        &w,
        "delete_items_permanently",
        json!({ "itemIds": [id], "deleteMedia": false }),
    );
    assert_same_shape(
        &del,
        &spec("delete_items_permanently"),
        "delete_items_permanently",
    );
    assert_same_shape(
        &del["files"],
        &spec("delete_items_permanently")["files"],
        "delete.files",
    );
    assert_eq!(del["items"], 1);
    let live = invoke(
        &w,
        "delete_items_permanently",
        json!({ "itemIds": [other], "deleteMedia": true }),
        app_origin(),
    )
    .unwrap_err();
    assert_eq!(
        live["code"], "INTERNAL_ERROR",
        "live items are never deleted for good"
    );

    assert_eq!(ok(&w, "list_orphans", json!({})), json!([]));
    for cmd in ["reimport_orphan", "delete_orphan"] {
        assert!(
            invoke(&w, cmd, json!({ "id": "nope" }), app_origin()).is_err(),
            "{cmd}"
        );
    }
}

#[test]
fn orphaned_media_round_trips_through_ipc() {
    use vv_persistence::library::repo::{MediaAsset, StorageRoot};
    use vv_persistence::library::*;
    let d = tempfile::tempdir().unwrap();
    let (app, id) = app_with_library(d.path());
    let w = window(&app, "main");
    let c = contract();
    {
        let mut db =
            Database::open(&d.path().join("db/library.sqlite"), &OpenOptions::default()).unwrap();
        let root = StorageRoot {
            id: new_id(),
            kind: StorageRootKind::ManagedMedia,
            label: "VideoVault".into(),
            path: d.path().join("media").to_string_lossy().into_owned(),
            volume_id: None,
            status: StorageRootStatus::Available,
            created_at: 1,
            last_verified_at: None,
        };
        repo::insert_storage_root(db.writer(), &root).unwrap();
        let mut a = MediaAsset::new(
            &id,
            AssetRole::Original,
            MediaKind::Video,
            StorageKind::Managed,
            &root.id,
            "a/video.mp4",
            1,
        );
        a.size_bytes = Some(10);
        repo::insert_media_asset(db.writer(), &a).unwrap();
    }
    ok(&w, "remove_items", json!({ "itemIds": [id] }));
    let del = ok(
        &w,
        "delete_items_permanently",
        json!({ "itemIds": [id], "deleteMedia": false }),
    );
    assert_eq!(del["orphaned"], 1);
    let orphans = ok(&w, "list_orphans", json!({}));
    let spec = &c["commands"]["list_orphans"]["response"][0];
    assert_same_shape(&orphans[0], spec, "list_orphans[0]");
    assert_eq!(orphans[0]["fileName"], "video.mp4");
    assert!(
        !orphans
            .to_string()
            .contains(&d.path().to_string_lossy().into_owned()),
        "no paths"
    );
    let item = ok(&w, "reimport_orphan", json!({ "id": orphans[0]["id"] }));
    assert_same_shape(
        &item,
        &c["commands"]["reimport_orphan"]["response"],
        "reimport_orphan",
    );
    assert_eq!(item["kind"], "local_video");
    assert_eq!(ok(&w, "list_orphans", json!({})), json!([]));
}

// ---- Player, settings and backups (STATE.md F2.4–F2.8) ----

#[test]
fn settings_playback_and_backup_commands_match_contract_shape() {
    use vv_persistence::library::repo::{MediaAsset, StorageRoot};
    use vv_persistence::library::*;
    let d = tempfile::tempdir().unwrap();
    let (app, id) = app_with_library(d.path());
    let w = window(&app, "main");
    let c = contract();
    let spec = |cmd: &str| c["commands"][cmd]["response"].clone();

    let s = ok(&w, "get_settings", json!({}));
    assert_same_shape(&s, &spec("get_settings"), "get_settings");
    assert_eq!(s["theme"], "dark");
    let s = ok(
        &w,
        "update_settings",
        c["commands"]["update_settings"]["args"].clone(),
    );
    assert_eq!(
        (s["theme"].clone(), s["offlineMode"].clone()),
        (json!("light"), json!(true))
    );
    assert!(
        invoke(
            &w,
            "update_settings",
            json!({ "patch": { "telemetry": true } }),
            app_origin()
        )
        .is_err(),
        "unknown settings are refused"
    );
    let err = invoke(&w, "open_source", json!({ "itemId": id }), app_origin()).unwrap_err();
    assert_eq!(
        err["code"], "OFFLINE_MODE",
        "no browser opens in Offline Mode"
    );
    ok(
        &w,
        "update_settings",
        json!({ "patch": { "offlineMode": false } }),
    );
    ok(&w, "acknowledge_legal", json!({}));
    let s = ok(&w, "finish_onboarding", json!({}));
    assert!(s["legalAckAt"].is_number() && s["onboardingCompletedAt"].is_number());
    let o = ok(&w, "get_storage_overview", json!({}));
    assert_same_shape(&o, &spec("get_storage_overview"), "get_storage_overview");
    assert_eq!(o["isDefault"], true);

    // A local file for the item: playback lists it with its vvmedia URL.
    let asset = {
        let mut db =
            Database::open(&d.path().join("db/library.sqlite"), &OpenOptions::default()).unwrap();
        let c = db.writer();
        let root = StorageRoot {
            id: new_id(),
            kind: StorageRootKind::ExternalImport,
            label: "Videos".into(),
            path: d.path().to_string_lossy().into_owned(),
            volume_id: None,
            status: StorageRootStatus::Available,
            created_at: 1,
            last_verified_at: None,
        };
        repo::insert_storage_root(c, &root).unwrap();
        let mut a = MediaAsset::new(
            &id,
            AssetRole::Original,
            MediaKind::Video,
            StorageKind::ExternalReference,
            &root.id,
            "clip.mp4",
            1,
        );
        a.container = Some("mp4".into());
        a.video_codec = Some("h264".into());
        a.duration_ms = Some(372_000);
        repo::insert_media_asset(c, &a).unwrap();
        a.id
    };
    let p = ok(&w, "get_playback", json!({ "itemId": id }));
    let mut expected = spec("get_playback");
    expected["state"] = Value::Null;
    assert_same_shape(&p, &expected, "get_playback");
    assert_same_shape(
        &p["sources"][0],
        &spec("get_playback")["sources"][0],
        "get_playback.sources[0]",
    );
    assert_eq!(p["sources"][0]["support"], "supported");
    let st = ok(
        &w,
        "save_playback_position",
        json!({ "itemId": id, "assetId": asset, "positionMs": 186_000, "durationMs": 372_000 }),
    );
    assert_same_shape(
        &st,
        &spec("save_playback_position"),
        "save_playback_position",
    );
    assert_eq!(st["resumable"], true);
    // AC-49: a new reader (as after a restart) sees the same position.
    assert_eq!(
        ok(&w, "get_playback", json!({ "itemId": id }))["state"]["positionMs"],
        186_000
    );
    assert!(invoke(
        &w,
        "save_playback_position",
        json!({ "itemId": id, "assetId": new_id(), "positionMs": 1 }),
        app_origin()
    )
    .is_err());

    let b = ok(&w, "backup_library", json!({}));
    assert_same_shape(&b, &spec("backup_library"), "backup_library");
    assert_eq!(b["kind"], "manual");
    assert_eq!(
        ok(&w, "list_backups", json!({})).as_array().unwrap().len(),
        1
    );
    let pv = ok(&w, "inspect_backup", json!({ "name": b["name"] }));
    let mut expected = spec("inspect_backup");
    expected["token"] = Value::Null;
    assert_same_shape(&pv, &expected, "inspect_backup");
    assert_eq!(pv["items"], 1);
    for bad in ["../db/library.sqlite", "library.sqlite"] {
        let e = invoke(&w, "inspect_backup", json!({ "name": bad }), app_origin()).unwrap_err();
        assert_eq!(e["code"], "FILE_MISSING", "{bad}");
    }
    ok(&w, "save_url", c["commands"]["save_url"]["args"].clone());
    let r = ok(
        &w,
        "restore_backup",
        json!({ "request": { "name": b["name"] } }),
    );
    assert_same_shape(&r, &spec("restore_backup"), "restore_backup");
    assert_eq!(r["items"], 1);
    assert_eq!(
        ok(&w, "list_items", json!({ "query": {} }))["total"],
        1,
        "the item saved after the backup is gone"
    );
    assert_eq!(
        ok(&w, "list_backups", json!({})).as_array().unwrap().len(),
        2,
        "pre-restore backup kept"
    );
    assert!(invoke(
        &w,
        "restore_backup",
        json!({ "request": { "token": "nope" } }),
        app_origin()
    )
    .is_err());
    ok(&w, "cancel_pending", json!({ "token": "nope" }));
}

// ---- Downloads, jobs and storage (STATE.md F3.2–F3.10) ----

/// `expected` with every key that is null in `actual` set to null, so the
/// shape check compares the rest (optional fields).
fn with_nulls_of(actual: &Value, expected: &Value) -> Value {
    let mut e = expected.clone();
    for (k, v) in actual.as_object().unwrap() {
        if v.is_null() {
            e[k] = Value::Null;
        }
    }
    e
}

#[test]
fn download_commands_match_contract_shape() {
    let d = tempfile::tempdir().unwrap();
    let (app, id) = app_with_library(d.path());
    let w = window(&app, "main");
    let c = contract();
    let spec = |cmd: &str| c["commands"][cmd]["response"].clone();

    // No engine in the mock app: the tools count as not ready.
    let t = ok(&w, "get_tool_health", json!({}));
    assert_same_shape(
        &t,
        &with_nulls_of(&t, &spec("get_tool_health")),
        "get_tool_health",
    );
    assert_eq!(t["ready"], false);

    let o = ok(&w, "get_download_options", json!({ "itemId": id }));
    assert_same_shape(
        &o,
        &with_nulls_of(&o, &spec("get_download_options")),
        "get_download_options",
    );
    assert_same_shape(
        &o["storage"],
        &with_nulls_of(&o["storage"], &spec("get_download_options")["storage"]),
        "get_download_options.storage",
    );
    assert_eq!(o["presets"].as_array().unwrap().len(), 4);
    assert_same_shape(
        &o["presets"][0],
        &with_nulls_of(
            &o["presets"][0],
            &spec("get_download_options")["presets"][0],
        ),
        "get_download_options.presets[0]",
    );

    // The UI sends a preset name; anything else is refused.
    for bad in [json!("bestvideo+bestaudio"), json!(1080)] {
        assert!(invoke(
            &w,
            "start_download",
            json!({ "itemId": id, "preset": bad }),
            app_origin()
        )
        .is_err());
    }
    let j = ok(
        &w,
        "start_download",
        json!({ "itemId": id, "preset": "720" }),
    );
    assert_same_shape(
        &j,
        &with_nulls_of(&j, &spec("start_download")),
        "start_download",
    );
    assert_eq!(
        (j["status"].clone(), j["preset"].clone()),
        (json!("queued"), json!("720"))
    );
    let l = ok(&w, "list_jobs", json!({}));
    assert_eq!(l.as_array().unwrap().len(), 1);
    assert_eq!(l[0]["itemTitle"], "Deniz kenarında sabah");

    let x = ok(&w, "cancel_job", json!({ "jobId": j["id"] }));
    assert_eq!(x["status"], "canceled");
    assert_eq!(x["canRetry"], true);
    let r = ok(&w, "retry_job", json!({ "jobId": j["id"] }));
    assert_eq!(r["status"], "queued");
    let e = invoke(
        &w,
        "remove_partial",
        json!({ "jobId": j["id"] }),
        app_origin(),
    )
    .unwrap_err();
    assert_eq!(
        e["code"], "INPUT_REJECTED",
        "a queued job keeps its partial file"
    );
    ok(&w, "cancel_job", json!({ "jobId": j["id"] }));
    let p = ok(&w, "remove_partial", json!({ "jobId": j["id"] }));
    assert_same_shape(&p, &spec("remove_partial"), "remove_partial");

    // "Clear finished": the canceled job leaves the list; a new download of
    // the same item is listed again.
    let cl = ok(&w, "clear_finished_downloads", json!({}));
    assert_same_shape(
        &cl,
        &spec("clear_finished_downloads"),
        "clear_finished_downloads",
    );
    assert_eq!(cl["cleared"], 1);
    assert_eq!(ok(&w, "list_jobs", json!({})).as_array().unwrap().len(), 0);
    let again = ok(
        &w,
        "start_download",
        json!({ "itemId": id, "preset": "720" }),
    );
    let l = ok(&w, "list_jobs", json!({}));
    assert_eq!(l.as_array().unwrap().len(), 1);
    assert_eq!(l[0]["id"], again["id"]);
    // An open job is never cleared.
    assert_eq!(ok(&w, "clear_finished_downloads", json!({}))["cleared"], 0);
    ok(&w, "cancel_job", json!({ "jobId": again["id"] }));
    let ai = ok(&w, "clear_finished_ai_jobs", json!({}));
    assert_same_shape(
        &ai,
        &spec("clear_finished_ai_jobs"),
        "clear_finished_ai_jobs",
    );

    // Settings → Storage lists folders that hold library files; a bookmark
    // has none yet.
    assert_eq!(ok(&w, "list_storage_roots", json!({})), json!([]));
    assert!(
        invoke(
            &w,
            "locate_storage_root",
            json!({ "rootId": 7 }),
            app_origin()
        )
        .is_err(),
        "a root id that is not a string is refused"
    );

    let m = ok(&w, "fetch_metadata", json!({ "itemId": id }));
    assert!(m.is_string());

    let u = ok(
        &w,
        "set_storage_quota",
        c["commands"]["set_storage_quota"]["args"].clone(),
    );
    assert_same_shape(
        &u,
        &with_nulls_of(&u, &spec("set_storage_quota")),
        "set_storage_quota",
    );
    assert_eq!(u["quotaBytes"], 10_737_418_240u64);
    let u = ok(&w, "set_storage_quota", json!({ "bytes": null }));
    assert!(u["quotaBytes"].is_null());
    assert!(invoke(&w, "set_storage_quota", json!({ "bytes": 0 }), app_origin()).is_err());
    let u = ok(&w, "get_storage_usage", json!({}));
    assert_same_shape(
        &u,
        &with_nulls_of(&u, &spec("get_storage_usage")),
        "get_storage_usage",
    );

    // Offline Mode: nothing new starts.
    ok(
        &w,
        "update_settings",
        json!({ "patch": { "offlineMode": true } }),
    );
    let e = invoke(
        &w,
        "start_download",
        json!({ "itemId": id, "preset": "best" }),
        app_origin(),
    )
    .unwrap_err();
    assert_eq!(e["code"], "OFFLINE_MODE");
    let e = invoke(&w, "fetch_metadata", json!({ "itemId": id }), app_origin()).unwrap_err();
    assert_eq!(e["code"], "OFFLINE_MODE");

    // No paths in any answer.
    for v in [&t, &o, &j, &l, &u] {
        let s = v.to_string();
        let dir = d.path().to_string_lossy();
        assert!(!s.contains(dir.as_ref()), "no paths: {s}");
    }
}

#[test]
fn session_commands_match_contract_and_never_show_the_folder() {
    // STATE.md F5.2, ADR-0023: the UI sees the browser profile's name,
    // never its folder or a secret reference.
    let d = tempfile::tempdir().unwrap();
    let (app, _) = app_with_library(d.path());
    let w = window(&app, "main");
    let c = contract();
    let spec = |cmd: &str| c["commands"][cmd]["response"].clone();
    let folder = d
        .path()
        .join("ffuser-SECRETNAME")
        .join("Profiles")
        .join("x1y2.default-release");
    let id = {
        let mut db =
            Database::open(&d.path().join("db/library.sqlite"), &OpenOptions::default()).unwrap();
        vv_persistence::auth::create(
            db.writer(),
            &vv_persistence::auth::NewProfile {
                provider: vv_domain::normalize::Platform::Reddit,
                label: "Reddit · Firefox (default-release)",
                method: vv_persistence::auth::Method::BrowserCookies,
                browser_kind: Some(vv_persistence::auth::BrowserKind::Firefox),
                profile_locator: Some(folder.to_str().unwrap()),
                consent_scope: &[
                    vv_persistence::auth::Operation::Metadata,
                    vv_persistence::auth::Operation::Download,
                ],
            },
            1,
        )
        .unwrap()
        .id
    };

    let l = ok(&w, "list_sessions", json!({}));
    assert_same_shape(&l, &spec("list_sessions"), "list_sessions");
    let s0 = &l["sessions"][0];
    assert_same_shape(
        s0,
        &with_nulls_of(s0, &spec("list_sessions")["sessions"][0]),
        "list_sessions.sessions[0]",
    );
    assert_eq!(s0["browserProfile"], "default-release");
    assert_eq!(s0["state"], "needs_verification");
    let text = l.to_string();
    assert!(
        !text.contains("SECRETNAME") && !text.contains("vvsec:"),
        "{text}"
    );
    assert!(!l["providers"]
        .as_array()
        .unwrap()
        .contains(&json!("generic")));

    let n = ok(
        &w,
        "set_session_consent",
        json!({ "id": id, "scope": ["download"] }),
    );
    assert_same_shape(
        &n,
        &with_nulls_of(&n, &spec("set_session_consent")),
        "set_session_consent",
    );
    assert_eq!(n["consent"], json!(["download"]));
    assert!(invoke(
        &w,
        "set_session_consent",
        json!({ "id": id, "scope": ["upload"] }),
        app_origin()
    )
    .is_err());

    let cl = ok(&w, "clear_session", json!({ "id": id }));
    assert_same_shape(
        &cl,
        &with_nulls_of(&cl, &spec("clear_session")),
        "clear_session",
    );
    assert_eq!(cl["state"], "not_connected");
    assert_eq!(cl["consent"], json!([]));

    // F5.5 Reconnect: consent again, the session starts unproven.
    let rc = ok(
        &w,
        "reconnect_session",
        json!({ "id": id, "scope": ["metadata", "download", "verify"] }),
    );
    assert_same_shape(
        &rc,
        &with_nulls_of(&rc, &spec("reconnect_session")),
        "reconnect_session",
    );
    assert_eq!(rc["state"], "needs_verification");
    assert!(invoke(
        &w,
        "reconnect_session",
        json!({ "id": id, "scope": [] }),
        app_origin()
    )
    .is_err());
    // The sign-in page: only known platforms; never in Offline Mode.
    assert!(invoke(
        &w,
        "open_sign_in",
        json!({ "provider": "generic" }),
        app_origin()
    )
    .is_err());
    ok(
        &w,
        "update_settings",
        json!({ "patch": { "offlineMode": true } }),
    );
    let e = invoke(
        &w,
        "open_sign_in",
        json!({ "provider": "reddit" }),
        app_origin(),
    )
    .unwrap_err();
    assert!(e.to_string().contains("OFFLINE_MODE"), "{e}");
    ok(
        &w,
        "update_settings",
        json!({ "patch": { "offlineMode": false } }),
    );

    // The in-app sign-in window (F5.7): only known platforms, never in
    // Offline Mode; Done without the window says it was closed; Cancel
    // without a window does nothing.
    assert!(invoke(
        &w,
        "start_app_sign_in",
        json!({ "provider": "generic" }),
        app_origin()
    )
    .is_err());
    ok(
        &w,
        "update_settings",
        json!({ "patch": { "offlineMode": true } }),
    );
    let e = invoke(
        &w,
        "start_app_sign_in",
        json!({ "provider": "vk" }),
        app_origin(),
    )
    .unwrap_err();
    assert!(e.to_string().contains("OFFLINE_MODE"), "{e}");
    ok(
        &w,
        "update_settings",
        json!({ "patch": { "offlineMode": false } }),
    );
    let e = invoke(
        &w,
        "finish_app_sign_in",
        json!({ "provider": "vk", "scope": ["download"] }),
        app_origin(),
    )
    .unwrap_err();
    assert!(e.to_string().contains("session-app-closed"), "{e}");
    assert_eq!(ok(&w, "cancel_app_sign_in", json!({})), Value::Null);

    // Only Firefox, only known platforms, only a profile that exists.
    for args in [
        json!({ "provider": "reddit", "browser": "chrome", "browserProfile": "Default", "scope": ["download"] }),
        json!({ "provider": "generic", "browser": "firefox", "browserProfile": "default-release", "scope": ["download"] }),
        json!({ "provider": "reddit", "browser": "firefox", "browserProfile": "no-such-profile-vv", "scope": ["download"] }),
    ] {
        assert!(
            invoke(&w, "connect_session", args.clone(), app_origin()).is_err(),
            "{args}"
        );
    }

    assert_eq!(ok(&w, "delete_session", json!({ "id": id })), Value::Null);
    assert_eq!(ok(&w, "list_sessions", json!({}))["sessions"], json!([]));
}

#[test]
fn model_commands_match_contract_and_need_consent() {
    // STATE.md F6.1, AC-20: no model and no download until the user agrees.
    let d = tempfile::tempdir().unwrap();
    let (app, _) = app_with_library(d.path());
    let w = window(&app, "main");
    let c = contract();
    let spec = |cmd: &str| c["commands"][cmd]["response"].clone();

    let l = ok(&w, "list_models", json!({}));
    assert_same_shape(&l, &spec("list_models"), "list_models");
    let m0 = &l["models"][0];
    assert_same_shape(
        m0,
        &with_nulls_of(m0, &spec("list_models")["models"][0]),
        "list_models.models[0]",
    );
    assert!(l["models"]
        .as_array()
        .unwrap()
        .iter()
        .all(|m| m["status"] == "not_installed" && m["job"].is_null()));
    let text = l.to_string();
    assert!(!text.contains(&*d.path().to_string_lossy()), "{text}");

    let e = invoke(
        &w,
        "download_model",
        json!({ "modelId": "whisper-tiny", "agreed": false }),
        app_origin(),
    )
    .unwrap_err();
    assert_eq!(e["code"], "INPUT_REJECTED");
    assert!(invoke(
        &w,
        "download_model",
        json!({ "modelId": "whisper-nope", "agreed": true }),
        app_origin()
    )
    .is_err());
    let l = ok(&w, "list_models", json!({}));
    assert!(l["models"]
        .as_array()
        .unwrap()
        .iter()
        .all(|m| m["status"] == "not_installed"));

    if vv_engine::models::catalog().revision.is_some() {
        let r = ok(
            &w,
            "download_model",
            json!({ "modelId": "whisper-tiny", "agreed": true }),
        );
        assert_same_shape(&r, &spec("download_model"), "download_model");
        let tiny = &r["models"][0];
        assert_eq!(tiny["status"], "pending");
        assert_eq!(tiny["job"]["kind"], "model_download");
        assert!(tiny["consentAt"].is_i64());
        let r = ok(&w, "remove_model", json!({ "modelId": "whisper-tiny" }));
        assert_eq!(r["models"][0]["status"], "not_installed");
        assert_eq!(r["models"][0]["job"]["status"], "canceled");
    }

    // F6.6: AI Processing shows this PC, the models and the queue; no path.
    let a = ok(&w, "get_ai_processing", json!({}));
    let s = spec("get_ai_processing");
    assert_same_shape(&a, &with_nulls_of(&a, &s), "get_ai_processing");
    assert_same_shape(
        &a["machine"],
        &with_nulls_of(&a["machine"], &s["machine"]),
        "get_ai_processing.machine",
    );
    assert!(a["machine"]["threads"].as_u64().unwrap() >= 1);
    assert_eq!(a["machine"]["cpuUse"], "full");
    assert!(a["jobs"].as_array().unwrap().is_empty());
    let text = a.to_string();
    assert!(!text.contains(&*d.path().to_string_lossy()), "{text}");
    let st = ok(
        &w,
        "update_settings",
        json!({ "patch": { "transcriptionCpu": "light" } }),
    );
    assert_eq!(st["transcriptionCpu"], "light");
    let a = ok(&w, "get_ai_processing", json!({}));
    assert_eq!(a["machine"]["cpuUse"], "light");
    assert!(a["machine"]["threads"].as_u64().unwrap() <= 2);

    // F7.1: search by meaning is off until an embedding API is set;
    // building without it is refused, cancel with nothing running is a no-op.
    let si = &a["searchIndex"];
    assert_same_shape(
        si,
        &with_nulls_of(si, &s["searchIndex"]),
        "get_ai_processing.searchIndex",
    );
    let g = ok(&w, "get_search_index", json!({}));
    let sg = spec("get_search_index");
    assert_same_shape(&g, &with_nulls_of(&g, &sg), "get_search_index");
    assert!(g["api"].is_null() && g["active"].is_null() && g["building"].is_null());
    assert!(g["job"].is_null());
    assert_eq!(g["searchable"], false);
    let e = invoke(
        &w,
        "build_search_index",
        json!({ "rebuild": false }),
        app_origin(),
    )
    .unwrap_err();
    assert_eq!(e["code"], "MODEL_MISSING");
    let c2 = ok(&w, "cancel_search_index", json!({}));
    assert!(c2["active"].is_null());
    // F7.4: reading text on screen needs Windows' OCR; elsewhere (and on a
    // Windows without an OCR language) it is refused, cancel is a no-op.
    let st = ok(&w, "get_screen_text", json!({}));
    assert_same_shape(
        &st,
        &with_nulls_of(&st, &spec("get_screen_text")),
        "get_screen_text",
    );
    assert_eq!(st["read"], 0);
    assert!(st["job"].is_null());
    if st["available"] == false {
        assert!(st["language"].is_null());
        let e = invoke(
            &w,
            "read_screen_text",
            json!({ "itemId": null }),
            app_origin(),
        )
        .unwrap_err();
        assert_eq!(e["code"], "MODEL_MISSING");
    } else {
        let r = ok(&w, "read_screen_text", json!({ "itemId": null }));
        assert_eq!(r["job"]["kind"], "ocr");
    }
    let cs = ok(&w, "cancel_screen_text", json!({}));
    assert_same_shape(
        &cs,
        &with_nulls_of(&cs, &spec("cancel_screen_text")),
        "cancel_screen_text",
    );

    let text = g.to_string();
    assert!(!text.contains(&*d.path().to_string_lossy()), "{text}");
    // A remote API is refused until the user agrees text goes there
    // (AI-C07); an address with credentials or not http(s) is refused.
    for (url, code) in [
        ("https://embed.example.com/v1", "URL_BLOCKED"),
        ("http://user:pw@localhost:11434/v1", "URL_HAS_CREDENTIALS"),
        ("file:///etc/passwd", "INVALID_URL"),
    ] {
        let e = invoke(
            &w,
            "set_embedding_api",
            json!({ "url": url, "model": "bge-m3", "key": null, "allowRemote": false }),
            app_origin(),
        )
        .unwrap_err();
        assert_eq!(e["code"], code, "{url}");
    }
    // Nothing listens on this port: the check fails and nothing is saved.
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let e = invoke(
        &w,
        "set_embedding_api",
        json!({ "url": format!("http://127.0.0.1:{port}/v1"), "model": "bge-m3", "key": null, "allowRemote": false }),
        app_origin(),
    )
    .unwrap_err();
    assert_eq!(e["code"], "NETWORK_UNAVAILABLE");
    assert!(ok(&w, "get_search_index", json!({}))["api"].is_null());
    let cl = ok(&w, "clear_embedding_api", json!({}));
    assert_same_shape(
        &cl,
        &with_nulls_of(&cl, &spec("clear_embedding_api")),
        "clear_embedding_api",
    );
}

/// A chat API on 127.0.0.1 that always suggests "deniz"; requests whose
/// bearer token is not `key` (when set) get 401.
fn tiny_chat_api(key: Option<&'static str>) -> String {
    use std::io::{BufRead, BufReader, Read, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}/v1", l.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            let mut r = BufReader::new(s.try_clone().unwrap());
            let (mut len, mut auth) = (0usize, String::new());
            let mut line = String::new();
            loop {
                line.clear();
                if r.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                    break;
                }
                let low = line.to_ascii_lowercase();
                if let Some(v) = low.strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
                if low.starts_with("authorization:") {
                    auth = line
                        .get("authorization:".len()..)
                        .unwrap_or("")
                        .trim()
                        .to_owned();
                }
            }
            let mut body = vec![0; len];
            let _ = r.read_exact(&mut body);
            let (status, out) = match key {
                Some(k) if auth != format!("Bearer {k}") => ("401 Unauthorized", "{}".to_owned()),
                _ => (
                    "200 OK",
                    json!({"choices": [{"message": {"content": "{\"tags\":[\"deniz\"]}"}}]})
                        .to_string(),
                ),
            };
            let _ = write!(
                s,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{out}",
                out.len()
            );
        }
    });
    url
}

#[test]
fn suggest_commands_match_contract_and_never_apply_by_themselves() {
    // STATE.md F7.3, AI-B01/B02, AI-C07: the chat API is checked before it
    // is saved, its key never comes back, a suggestion changes nothing
    // until it is accepted, and an accept is an undoable "add tag".
    let d = tempfile::tempdir().unwrap();
    let (app, id) = app_with_library(d.path());
    let w = window(&app, "main");
    let c = contract();
    let spec = |cmd: &str| c["commands"][cmd]["response"].clone();

    let g = ok(&w, "get_tag_suggest", json!({}));
    assert_same_shape(
        &g,
        &with_nulls_of(&g, &spec("get_tag_suggest")),
        "get_tag_suggest",
    );
    assert!(g["api"].is_null() && g["run"].is_null());
    assert_eq!(g["pending"], 0);
    // Nothing set: "Suggest for all" is refused.
    let e = invoke(&w, "suggest_all", json!({ "force": false }), app_origin()).unwrap_err();
    assert!(
        ["MODEL_MISSING", "TOOL_MISSING"].contains(&e["code"].as_str().unwrap()),
        "{e}"
    );

    // A remote API needs agreement; a wrong key fails the check.
    let e = invoke(
        &w,
        "set_suggest_api",
        json!({ "url": "https://llm.example.com/v1", "model": "m", "key": null, "allowRemote": false }),
        app_origin(),
    )
    .unwrap_err();
    assert_eq!(e["code"], "URL_BLOCKED");
    let keyed = tiny_chat_api(Some("sk-test-123"));
    let e = invoke(
        &w,
        "set_suggest_api",
        json!({ "url": keyed, "model": "qwen3.5:4b", "key": "wrong", "allowRemote": false }),
        app_origin(),
    )
    .unwrap_err();
    assert_eq!(e["code"], "ACCESS_DENIED");
    assert!(ok(&w, "get_tag_suggest", json!({}))["api"].is_null());
    assert!(!e.to_string().contains("wrong"));
    // (The OS credential store is not there in CI: a saved key is tested in vv-engine.)
    let url = tiny_chat_api(None);
    let s = ok(
        &w,
        "set_suggest_api",
        json!({ "url": url, "model": "qwen3.5:4b", "key": null, "allowRemote": false }),
    );
    assert_same_shape(
        &s,
        &with_nulls_of(&s, &spec("set_suggest_api")),
        "set_suggest_api",
    );
    assert_same_shape(
        &s["api"],
        &spec("set_suggest_api")["api"],
        "set_suggest_api.api",
    );
    assert_eq!(s["api"]["hasKey"], false);
    assert_eq!(s["api"]["local"], true);

    // A suggestion as a run stores it: listed, not applied.
    let p_id = {
        let mut db =
            Database::open(&d.path().join("db/library.sqlite"), &OpenOptions::default()).unwrap();
        let ps = vv_persistence::proposals::save_run(
            db.writer(),
            &id,
            "qwen3.5:4b",
            &url,
            &"0".repeat(32),
            &["deniz".into(), "sabah".into()],
            1_794_000_000_000,
        )
        .unwrap();
        ps[0].id.clone()
    };
    let l = ok(&w, "list_ai_proposals", json!({ "itemId": id }));
    assert_same_shape(&l[0], &spec("list_ai_proposals")[0], "list_ai_proposals[0]");
    assert_eq!(l.as_array().unwrap().len(), 2);
    assert_eq!(ok(&w, "get_tag_suggest", json!({}))["pending"], 2);
    let item = ok(&w, "get_item", json!({ "id": id }));
    assert!(item["tagRefs"].as_array().unwrap().is_empty());
    // The review screen sees the same two, grouped by value.
    let g = ok(&w, "list_ai_proposal_groups", json!({}));
    assert_same_shape(
        &g[0],
        &spec("list_ai_proposal_groups")[0],
        "list_ai_proposal_groups[0]",
    );
    assert_same_shape(
        &g[0]["items"][0],
        &spec("list_ai_proposal_groups")[0]["items"][0],
        "list_ai_proposal_groups[0].items[0]",
    );
    assert_eq!(g.as_array().unwrap().len(), 2);
    assert_eq!(ok(&w, "get_sidebar", json!({}))["counts"]["suggestions"], 2);

    let r = ok(
        &w,
        "decide_ai_proposal",
        json!({ "id": p_id, "accept": true, "name": null }),
    );
    assert_same_shape(&r, &spec("decide_ai_proposal"), "decide_ai_proposal");
    assert_same_shape(
        &r["op"],
        &spec("decide_ai_proposal")["op"],
        "decide_ai_proposal.op",
    );
    assert_eq!(r["proposals"].as_array().unwrap().len(), 1);
    let item = ok(&w, "get_item", json!({ "id": id }));
    assert_eq!(item["tagRefs"][0]["name"], "deniz");
    // Undo takes the tag off again.
    ok(&w, "undo", json!({}));
    let item = ok(&w, "get_item", json!({ "id": id }));
    assert!(item["tagRefs"].as_array().unwrap().is_empty());
    // Rejecting changes no tag; a decided suggestion is not decided again.
    let other = r["proposals"][0]["id"].clone();
    let r = ok(
        &w,
        "decide_ai_proposal",
        json!({ "id": other, "accept": false, "name": null }),
    );
    assert!(r["proposals"].as_array().unwrap().is_empty() && r["op"].is_null());
    assert!(invoke(
        &w,
        "decide_ai_proposal",
        json!({ "id": other, "accept": true, "name": null }),
        app_origin()
    )
    .is_err());

    // Many at once: one undoable "add tag", named as the user typed it.
    let more = {
        let mut db =
            Database::open(&d.path().join("db/library.sqlite"), &OpenOptions::default()).unwrap();
        vv_persistence::proposals::save_run(
            db.writer(),
            &id,
            "qwen3.5:4b",
            &url,
            &"1".repeat(32),
            &["gece".into()],
            1_794_000_000_001,
        )
        .unwrap()
    };
    let r = ok(
        &w,
        "decide_ai_proposals",
        json!({ "ids": [more[0].id], "accept": true, "name": "Gece çekimi" }),
    );
    assert_same_shape(&r, &spec("decide_ai_proposals"), "decide_ai_proposals");
    assert_same_shape(
        &r["op"],
        &spec("decide_ai_proposals")["op"],
        "decide_ai_proposals.op",
    );
    assert_eq!(r["decided"], 1);
    let item = ok(&w, "get_item", json!({ "id": id }));
    assert_eq!(item["tagRefs"][0]["name"], "Gece çekimi");
    assert!(ok(&w, "list_ai_proposal_groups", json!({}))
        .as_array()
        .unwrap()
        .is_empty());
    // Decided already: refused.
    assert!(invoke(
        &w,
        "decide_ai_proposals",
        json!({ "ids": [more[0].id], "accept": false, "name": null }),
        app_origin()
    )
    .is_err());
    ok(&w, "undo", json!({}));

    // Without the engine (no tools checked in this test) asking is refused cleanly.
    let e = invoke(
        &w,
        "suggest_tags",
        json!({ "itemId": id, "force": false }),
        app_origin(),
    )
    .unwrap_err();
    assert_eq!(e["code"], "TOOL_MISSING");
    let cs = ok(&w, "cancel_suggest_all", json!({}));
    assert_same_shape(
        &cs,
        &with_nulls_of(&cs, &spec("cancel_suggest_all")),
        "cancel_suggest_all",
    );
    let cl = ok(&w, "clear_suggest_api", json!({}));
    assert_same_shape(
        &cl,
        &with_nulls_of(&cl, &spec("clear_suggest_api")),
        "clear_suggest_api",
    );
    assert!(cl["api"].is_null());
}

#[test]
fn transcript_commands_match_contract_and_queue_captions() {
    // STATE.md F6.2, AI-A01: site captions are a transcript of their own.
    use vv_persistence::transcripts::{self, CaptionKind, NewSegment, Origin, Provenance, Status};
    let d = tempfile::tempdir().unwrap();
    let (app, id) = app_with_library(d.path());
    let w = window(&app, "main");
    let c = contract();
    let spec = |cmd: &str| c["commands"][cmd]["response"].clone();

    let l = ok(&w, "list_transcripts", json!({ "itemId": id }));
    assert_eq!(l["transcripts"], json!([]));
    assert!(l["captionsJob"].is_null());
    assert_eq!(l["canFetchCaptions"], true);
    assert_same_shape(
        &l,
        &with_nulls_of(&l, &spec("list_transcripts")),
        "list_transcripts",
    );
    // A bookmark has no file to transcribe on this PC (F6.2, ADR-0028).
    assert!(l["transcribeJob"].is_null());
    assert!(l["transcribeModel"].is_null());
    assert!(l["transcribeElapsedMs"].is_null());
    assert_eq!(l["local"]["ready"], false);
    assert_eq!(l["local"]["reason"], "no-media");
    assert_eq!(l["local"]["models"], json!([]));
    assert_eq!(l["local"]["languages"], json!(["auto", "tr", "en"]));
    let e = invoke(
        &w,
        "transcribe_locally",
        json!({ "itemId": id, "modelId": "whisper-base", "language": "auto" }),
        app_origin(),
    )
    .unwrap_err();
    assert_eq!(e["code"], "FILE_MISSING");
    let e = invoke(
        &w,
        "transcribe_locally",
        json!({ "itemId": id, "modelId": "whisper-base", "language": "de; rm -rf" }),
        app_origin(),
    )
    .unwrap_err();
    assert_eq!(e["code"], "INPUT_REJECTED");

    let mut db =
        Database::open(&d.path().join("db/library.sqlite"), &OpenOptions::default()).unwrap();
    let tx = db.writer().transaction().unwrap();
    let origin = Origin {
        item_id: id.clone(),
        asset_id: None,
        language: "tr".into(),
        provenance: Provenance::SourceCaptions,
        provider: "source_captions".into(),
        model_id: None,
        model_version: None,
        source_fingerprint: "tr:manual:sha256:00".into(),
        caption_kind: Some(CaptionKind::Manual),
    };
    let segs = vec![
        NewSegment::new(0, 3_200, "Günaydın, deniz kenarındayız."),
        NewSegment::new(3_200, 6_100, "<b>Bugün</b> sabah yürüyüşü yapıyoruz."),
    ];
    let t = transcripts::save(
        &tx,
        &origin,
        Status::Ready,
        Some(6_100),
        &segs,
        Some(372_000),
        1,
    )
    .unwrap();
    tx.commit().unwrap();

    let r = ok(&w, "fetch_captions", json!({ "itemId": id }));
    assert_same_shape(&r, &spec("fetch_captions"), "fetch_captions");
    assert_same_shape(
        &r["transcripts"][0],
        &spec("fetch_captions")["transcripts"][0],
        "fetch_captions.transcripts[0]",
    );
    assert_eq!(r["captionsJob"]["kind"], "fetch_captions");
    assert_eq!(r["transcripts"][0]["captionKind"], "manual");
    // Asking again while it is open gives the same job.
    let again = ok(&w, "fetch_captions", json!({ "itemId": id }));
    assert_eq!(again["captionsJob"]["id"], r["captionsJob"]["id"]);

    let g = ok(&w, "get_transcript", json!({ "transcriptId": t.id }));
    assert_same_shape(&g, &spec("get_transcript"), "get_transcript");
    // Text comes back as stored text; the UI renders it as text.
    assert_eq!(
        g["segments"][1]["text"],
        "<b>Bugün</b> sabah yürüyüşü yapıyoruz."
    );
    assert_eq!(g["segments"][1]["startMs"], 3_200);

    let e = invoke(
        &w,
        "get_transcript",
        json!({ "transcriptId": "nope" }),
        app_origin(),
    )
    .unwrap_err();
    assert_eq!(e["code"], "INPUT_REJECTED");
    let e = invoke(
        &w,
        "list_transcripts",
        json!({ "itemId": "nope" }),
        app_origin(),
    )
    .unwrap_err();
    assert_eq!(e["code"], "INPUT_REJECTED");

    assert_same_shape(
        &g["segments"][0],
        &spec("get_transcript")["segments"][0],
        "get_transcript.segments[0]",
    );

    // F6.4, SEA-06: words said in the video find the item; the moment opens
    // at the start of the segment that holds them (PLAY-06).
    let found = ok(
        &w,
        "list_items",
        json!({ "query": { "text": "yürüyüşü", "sort": "relevance" } }),
    );
    assert_eq!(found["total"], 1);
    let tr = &found["items"][0]["transcript"];
    let shape = &c["commands"]["list_items"]["transcriptShape"];
    assert_same_shape(tr, shape, "list_items.items[0].transcript");
    assert_same_shape(
        &tr["moments"][0],
        &shape["moments"][0],
        "transcript.moments[0]",
    );
    assert_eq!(tr["moments"][0]["startMs"], 3_200);
    assert_eq!(tr["moments"][0]["snippet"]["field"], "transcript");
    let seg = tr["moments"][0]["segmentId"].as_str().unwrap().to_owned();
    let m = ok(&w, "get_moment", json!({ "segmentId": seg }));
    assert_same_shape(&m, &spec("get_moment"), "get_moment");
    assert_eq!(
        (m["itemId"].as_str(), m["startMs"].as_i64()),
        (Some(id.as_str()), Some(3_200))
    );
    let plain = ok(&w, "list_items", json!({ "query": {} }));
    assert!(plain["items"][0]["transcript"].is_null());

    // F6.5, DATA-11: the note's timecodes come from stored segments.
    let note = ok(
        &w,
        "preview_note",
        json!({ "itemId": id, "segmentIds": [seg.clone()] }),
    );
    assert_same_shape(&note, &spec("preview_note"), "preview_note");
    assert_eq!(note["moments"], 1);
    let md = note["markdown"].as_str().unwrap();
    assert!(md.contains("## Moments"), "{md}");
    assert!(md.contains("0:03"), "{md}");
    let e = invoke(
        &w,
        "preview_note",
        json!({ "itemId": id, "segmentIds": ["not-a-segment"] }),
        app_origin(),
    )
    .unwrap_err();
    assert_eq!(e["code"], "INPUT_REJECTED");

    let x = ok(&w, "delete_transcript", json!({ "transcriptId": t.id }));
    assert_same_shape(&x, &spec("delete_transcript"), "delete_transcript");
    assert_eq!(x["transcripts"], json!([]));

    // The transcript is gone: an old result says so instead of seeking.
    let e = invoke(&w, "get_moment", json!({ "segmentId": seg }), app_origin()).unwrap_err();
    assert_eq!(
        (e["code"].as_str(), e["diagnosticId"].as_str()),
        (Some("INPUT_REJECTED"), Some("moment-gone"))
    );

    // F7.4, AC-41: text read on screen finds the item, apart from speech,
    // and its moment opens at the time the text appeared.
    use vv_persistence::screen_text::{self, Line, Source};
    screen_text::save(
        db.writer(),
        &Source {
            item_id: &id,
            asset_id: None,
            provider: "test",
            language: "tr",
            config_version: 1,
            fingerprint: "size:1",
        },
        4,
        &[Line {
            start_ms: 12_000,
            end_ms: 18_000,
            text: "HARRIER JUMP JET".into(),
        }],
        1_794_000_000_000,
    )
    .unwrap();
    let on = ok(
        &w,
        "list_items",
        json!({ "query": { "text": "harrier", "sort": "relevance" } }),
    );
    assert_eq!(on["total"], 1);
    let it = &on["items"][0];
    assert!(it["transcript"].is_null());
    let sc = &it["onScreen"];
    assert_same_shape(
        sc,
        &with_nulls_of(sc, shape),
        "list_items.items[0].onScreen",
    );
    assert_eq!(
        (sc["provenance"].as_str(), sc["language"].as_str()),
        (Some("ocr"), Some("tr"))
    );
    assert_eq!(sc["moments"][0]["startMs"], 12_000);
    assert_eq!(sc["moments"][0]["snippet"]["field"], "on_screen");
    let line = sc["moments"][0]["segmentId"].as_str().unwrap().to_owned();
    let om = ok(&w, "get_moment", json!({ "segmentId": line }));
    assert_same_shape(&om, &spec("get_moment"), "get_moment (on screen)");
    assert_eq!(
        (om["startMs"].as_i64(), om["transcriptId"].as_str()),
        (Some(12_000), Some(""))
    );

    for v in [&l, &r, &g, &x, &found, &m, &note] {
        let s = v.to_string();
        assert!(!s.contains(&*d.path().to_string_lossy()), "no paths: {s}");
    }
}
