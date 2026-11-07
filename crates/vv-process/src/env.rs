//! The environment a managed child starts with.

use std::ffi::OsString;

/// Variables copied from the app's environment when present. Everything
/// else is dropped. Windows needs `SystemRoot` for Winsock and crypto;
/// `TEMP`/`TMP` give tools a scratch directory.
#[cfg(windows)]
pub const BASE_ENV_KEYS: &[&str] = &[
    "SystemRoot",
    "SystemDrive",
    "windir",
    "TEMP",
    "TMP",
    "PATHEXT",
    "NUMBER_OF_PROCESSORS",
    "PROCESSOR_ARCHITECTURE",
];
#[cfg(unix)]
pub const BASE_ENV_KEYS: &[&str] = &["HOME", "TMPDIR", "LANG"];

/// The allowlisted variables plus a fixed `PATH`: managed tools are always
/// started by absolute path, so `PATH` only has to find system libraries.
pub fn base_env() -> Vec<(OsString, OsString)> {
    let mut out: Vec<(OsString, OsString)> = BASE_ENV_KEYS
        .iter()
        .filter_map(|k| std::env::var_os(k).map(|v| (OsString::from(k), v)))
        .collect();
    #[cfg(windows)]
    {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        let mut path = root.clone();
        path.push("\\System32;");
        path.push(&root);
        out.push(("PATH".into(), path));
    }
    #[cfg(unix)]
    out.push(("PATH".into(), "/usr/bin:/bin".into()));
    out
}
