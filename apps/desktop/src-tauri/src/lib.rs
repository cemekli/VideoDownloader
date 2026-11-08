//! VideoVault desktop shell (Tauri 2).
//!
//! Only typed commands cross into the webview. Which commands a window may
//! call is decided by `capabilities/main.json`; `build.rs` turns that check on
//! for app commands.

pub mod commands;
pub mod instance;
pub mod library;
pub mod library_commands;
pub mod media_protocol;
pub mod thumb_protocol;

use tauri::{Builder, DragDropEvent, Manager, Runtime, WindowEvent};

/// Registers every command on a builder. Shared by `run()` and the IPC tests so
/// both exercise exactly the same handler.
pub fn register<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    builder
        .manage(instance::InstanceState::default())
        .manage(library::LibraryState::default())
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::get_status,
            commands::get_diagnostics,
            library_commands::list_items,
            library_commands::get_item,
            library_commands::import_media
        ])
        .register_uri_scheme_protocol(library_commands::THUMB_SCHEME, |ctx, request| {
            thumb_protocol::handle(&ctx.app_handle().state::<library::LibraryState>(), &request)
        })
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
    #[cfg(feature = "e2e-cdp")]
    {
        builder = e2e::register_media_protocol(builder);
    }
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

    /// Serves `VV_E2E_MEDIA_DIR` over the media protocol for the playback
    /// matrix (ADR-0004). Without the variable nothing is registered.
    pub fn register_media_protocol<R: tauri::Runtime>(
        builder: tauri::Builder<R>,
    ) -> tauri::Builder<R> {
        let Some(root) = std::env::var_os("VV_E2E_MEDIA_DIR")
            .and_then(|d| std::path::PathBuf::from(d).canonicalize().ok())
        else {
            return builder;
        };
        builder.register_uri_scheme_protocol(crate::media_protocol::SCHEME, move |_ctx, request| {
            crate::media_protocol::handle(&root, &request)
        })
    }
}
