//! VideoVault desktop shell (Tauri 2).
//!
//! Only typed commands cross into the webview. Which commands a window may
//! call is decided by `capabilities/main.json`; `build.rs` turns that check on
//! for app commands.

pub mod capture_commands;
pub mod commands;
pub mod data_commands;
pub mod download_commands;
pub mod engine;
pub mod instance;
pub mod library;
pub mod library_commands;
pub mod media_protocol;
pub mod organize_commands;
pub mod player_commands;
pub mod thumb_protocol;

use tauri::{Builder, DragDropEvent, Manager, Runtime, WindowEvent};

/// Registers every command on a builder. Shared by `run()` and the IPC tests so
/// both exercise exactly the same handler.
pub fn register<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    builder
        .manage(instance::InstanceState::default())
        .manage(library::LibraryState::default())
        .manage(engine::EngineState::default())
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::get_status,
            commands::get_diagnostics,
            library_commands::list_items,
            library_commands::get_item,
            library_commands::import_media,
            capture_commands::check_url,
            capture_commands::save_url,
            capture_commands::restore_item,
            organize_commands::get_sidebar,
            organize_commands::undo,
            organize_commands::tag_items,
            organize_commands::untag_items,
            organize_commands::add_to_collection,
            organize_commands::remove_from_collection,
            organize_commands::set_favorite,
            organize_commands::set_archived,
            organize_commands::remove_items,
            organize_commands::restore_items,
            organize_commands::delete_items_permanently,
            organize_commands::rename_tag,
            organize_commands::merge_tags,
            organize_commands::delete_tag,
            organize_commands::create_collection,
            organize_commands::rename_collection,
            organize_commands::move_collection,
            organize_commands::delete_collection,
            organize_commands::save_smart_collection,
            organize_commands::delete_smart_collection,
            organize_commands::set_notes,
            organize_commands::list_duplicates,
            organize_commands::preview_merge,
            organize_commands::merge_items,
            organize_commands::dismiss_duplicate,
            organize_commands::list_orphans,
            organize_commands::reimport_orphan,
            organize_commands::delete_orphan,
            data_commands::get_settings,
            data_commands::update_settings,
            data_commands::acknowledge_legal,
            data_commands::finish_onboarding,
            data_commands::get_storage_overview,
            data_commands::choose_media_folder,
            data_commands::export_library,
            data_commands::preview_library_import,
            data_commands::map_import_root,
            data_commands::apply_library_import,
            data_commands::cancel_pending,
            data_commands::backup_library,
            data_commands::list_backups,
            data_commands::inspect_backup,
            data_commands::restore_backup,
            player_commands::get_playback,
            player_commands::save_playback_position,
            player_commands::open_externally,
            player_commands::open_source,
            player_commands::relink_asset,
            download_commands::get_tool_health,
            download_commands::list_jobs,
            download_commands::cancel_job,
            download_commands::retry_job,
            download_commands::remove_partial,
            download_commands::get_download_options,
            download_commands::start_download,
            download_commands::fetch_metadata,
            download_commands::get_storage_usage,
            download_commands::set_storage_quota,
            download_commands::move_media_folder
        ])
        .register_uri_scheme_protocol(library_commands::THUMB_SCHEME, |ctx, request| {
            thumb_protocol::handle(&ctx.app_handle().state::<library::LibraryState>(), &request)
        })
        // Library media by asset id (F2.4). The E2E build also serves the
        // playback matrix folder by file name (ADR-0004).
        .register_uri_scheme_protocol(media_protocol::SCHEME, {
            let e2e_root = e2e_media_root();
            move |ctx, request| {
                media_protocol::dispatch(
                    &ctx.app_handle().state::<library::LibraryState>(),
                    e2e_root.as_deref(),
                    &request,
                )
            }
        })
}

/// `VV_E2E_PICK`, only in the E2E test build: the paths the native file
/// dialog stands for, so a real-window test can import without clicking a
/// system dialog. The shipped build always shows the dialog.
pub(crate) fn e2e_picked() -> Option<Vec<std::path::PathBuf>> {
    if !cfg!(feature = "e2e-cdp") {
        return None;
    }
    std::env::var_os("VV_E2E_PICK").map(|v| std::env::split_paths(&v).collect())
}

/// `VV_E2E_MEDIA_DIR`, only in the E2E test build.
fn e2e_media_root() -> Option<std::path::PathBuf> {
    if !cfg!(feature = "e2e-cdp") {
        return None;
    }
    std::env::var_os("VV_E2E_MEDIA_DIR")
        .and_then(|d| std::path::PathBuf::from(d).canonicalize().ok())
}

pub fn run() {
    #[allow(unused_mut)]
    let mut context = tauri::generate_context!();
    #[cfg(feature = "e2e-cdp")]
    e2e::enable_cdp(context.config_mut());
    // The plugin goes first: a second launch must exit before anything else
    // starts (ADR-0008).
    let mut builder = Builder::default();
    if !single_instance_disabled() {
        builder = builder.plugin(tauri_plugin_single_instance::init(
            instance::on_second_launch,
        ));
    }
    #[allow(unused_mut)]
    let mut builder = register(builder)
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            instance::take_library_lock(app.handle())?;
            library::open_library(app.handle())
        })
        // Files dropped on the window go to import from here; the webview
        // never handles their paths (ADR-0013).
        .on_window_event(|window, event| {
            if let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event {
                library::import_dropped(window.app_handle(), paths.clone());
            }
        });
    builder
        .run(context)
        .expect("error while running VideoVault");
}

/// E2E only: `VV_E2E_NO_SINGLE_INSTANCE=1` leaves out the plugin so the
/// library lock, the second layer, can be tested in the real app.
fn single_instance_disabled() -> bool {
    cfg!(feature = "e2e-cdp") && std::env::var_os("VV_E2E_NO_SINGLE_INSTANCE").is_some()
}

/// Test-only hook (feature `e2e-cdp`, ADR-0001). WebView2 ignores
/// WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS when the app passes its own browser
/// arguments, which Tauri always does, so the port has to be set here.
#[cfg(feature = "e2e-cdp")]
mod e2e {
    /// Tauri/wry's default WebView2 arguments; setting our own replaces them.
    const WRY_DEFAULT_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";

    pub fn enable_cdp(config: &mut tauri::Config) {
        let Ok(port) = std::env::var("VV_E2E_CDP_PORT") else {
            return;
        };
        let Ok(port) = port.parse::<u16>() else {
            return;
        };
        for window in &mut config.app.windows {
            window.additional_browser_args =
                Some(format!("{WRY_DEFAULT_ARGS} --remote-debugging-port={port}"));
        }
    }
}
