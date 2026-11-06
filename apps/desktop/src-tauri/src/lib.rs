//! VideoVault desktop shell (Tauri 2).
//!
//! Only typed commands cross into the webview. Which commands a window may
//! call is decided by `capabilities/main.json`; `build.rs` turns that check on
//! for app commands.

pub mod commands;

use tauri::{Builder, Runtime};

/// Registers every command on a builder. Shared by `run()` and the IPC tests so
/// both exercise exactly the same handler.
pub fn register<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        commands::ping,
        commands::get_status
    ])
}

pub fn run() {
    register(Builder::default())
        .run(tauri::generate_context!())
        .expect("error while running VideoVault");
}
