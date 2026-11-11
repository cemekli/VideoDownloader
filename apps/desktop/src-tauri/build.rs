// Every app command must be listed here. Listing them turns on Tauri's ACL for
// app commands, so a command is callable only if a capability grants it
// (SPEC §20 IPC row: least privilege).
const COMMANDS: &[&str] = &[
    "ping",
    "get_status",
    "get_diagnostics",
    "list_items",
    "get_item",
    "import_media",
    "check_url",
    "save_url",
    "restore_item",
    "get_sidebar",
    "undo",
    "tag_items",
    "untag_items",
    "add_to_collection",
    "remove_from_collection",
    "set_favorite",
    "set_archived",
    "remove_items",
    "restore_items",
    "delete_items_permanently",
    "rename_tag",
    "merge_tags",
    "delete_tag",
    "create_collection",
    "rename_collection",
    "move_collection",
    "delete_collection",
    "save_smart_collection",
    "delete_smart_collection",
    "set_notes",
    "list_duplicates",
    "preview_merge",
    "merge_items",
    "dismiss_duplicate",
    "list_orphans",
    "reimport_orphan",
    "delete_orphan",
    "get_settings",
    "update_settings",
    "acknowledge_legal",
    "finish_onboarding",
    "get_storage_overview",
    "choose_media_folder",
    "export_library",
    "preview_library_import",
    "map_import_root",
    "apply_library_import",
    "cancel_pending",
    "backup_library",
    "list_backups",
    "inspect_backup",
    "restore_backup",
    "get_playback",
    "save_playback_position",
    "open_externally",
    "open_source",
    "relink_asset",
    "get_tool_health",
    "list_jobs",
    "cancel_job",
    "retry_job",
    "remove_partial",
    "get_download_options",
    "start_download",
    "fetch_metadata",
    "get_storage_usage",
    "set_storage_quota",
    "move_media_folder",
    "list_sessions",
    "connect_session",
    "set_session_consent",
    "clear_session",
    "delete_session",
    "connect_session_file",
    "reconnect_session",
    "verify_session",
    "open_sign_in",
    "start_app_sign_in",
    "finish_app_sign_in",
    "cancel_app_sign_in",
    "list_models",
    "download_model",
    "verify_model",
    "remove_model",
    "list_transcripts",
    "get_transcript",
    "fetch_captions",
    "transcribe_locally",
    "delete_transcript",
];

fn main() {
    // On Windows, Tauri links against Common Controls v6, which needs an
    // application manifest. tauri-build embeds it only into the app binary, so
    // test executables (tests/ipc.rs) die at startup with
    // STATUS_ENTRYPOINT_NOT_FOUND. Embedding the same manifest through the
    // linker covers every target: the app, unit tests and integration tests.
    let windows = if is_windows_msvc() {
        embed_windows_manifest();
        tauri_build::WindowsAttributes::new_without_app_manifest()
    } else {
        tauri_build::WindowsAttributes::new()
    };

    tauri_build::try_build(
        tauri_build::Attributes::new()
            .windows_attributes(windows)
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}

fn is_windows_msvc() -> bool {
    std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
}

fn embed_windows_manifest() {
    let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("windows-app-manifest.xml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}
