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
