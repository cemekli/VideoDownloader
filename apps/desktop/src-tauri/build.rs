// Every app command must be listed here. Listing them turns on Tauri's ACL for
// app commands, so a command is callable only if a capability grants it
// (SPEC §20 IPC row: least privilege).
const COMMANDS: &[&str] = &["ping", "get_status", "get_diagnostics"];

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
