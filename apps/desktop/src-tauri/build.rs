// Every app command must be listed here. Listing them turns on Tauri's ACL for
// app commands, so a command is callable only if a capability grants it
// (SPEC §20 IPC row: least privilege).
const COMMANDS: &[&str] = &["ping", "get_status"];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
