//! Typed IPC commands (SPEC §18). Each response is a stable DTO, never a raw
//! DB row or subprocess output. Shapes are pinned by
//! `tests/fixtures/ipc-contract.json`, which the UI tests use too.

use serde::Serialize;
use tauri::{AppHandle, Runtime};

/// Bumped whenever a command, DTO or error field changes incompatibly.
pub const CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pong {
    pub nonce: u32,
    pub contract_version: u32,
}

/// Liveness check for the IPC bridge. Echoes the nonce so the UI can tell a
/// fresh answer from a stale one.
#[tauri::command]
pub fn ping(nonce: u32) -> Pong {
    Pong {
        nonce,
        contract_version: CONTRACT_VERSION,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub app_name: String,
    pub app_version: String,
    pub contract_version: u32,
    /// `std::env::consts::OS`, e.g. `windows`, `linux`.
    pub os: &'static str,
    /// `std::env::consts::ARCH`, e.g. `x86_64`, `aarch64`.
    pub arch: &'static str,
    pub build_profile: &'static str,
}

pub fn status(app_name: &str, app_version: &str) -> AppStatus {
    AppStatus {
        app_name: app_name.to_owned(),
        app_version: app_version.to_owned(),
        contract_version: CONTRACT_VERSION,
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
    }
}

/// Basic app facts for the status bar and diagnostics. Contains no paths,
/// user names or other personal data.
#[tauri::command]
pub fn get_status<R: Runtime>(app: AppHandle<R>) -> AppStatus {
    let info = app.package_info();
    status(&info.name, &info.version.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_echoes_nonce() {
        assert_eq!(
            ping(7),
            Pong {
                nonce: 7,
                contract_version: CONTRACT_VERSION
            }
        );
    }

    #[test]
    fn status_reports_build_facts_only() {
        let s = status("VideoVault", "0.0.1");
        assert_eq!(s.contract_version, CONTRACT_VERSION);
        assert_eq!(s.os, std::env::consts::OS);
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            !json.contains('\\') && !json.contains('/'),
            "status must not leak paths: {json}"
        );
    }
}
