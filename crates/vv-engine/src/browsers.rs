//! Finding the user's browser profiles for `--cookies-from-browser`
//! (STATE.md F5.2; SPEC §20.3, §21.1; ADR-0023).
//!
//! Only Firefox for now: on Windows, Chrome 127+ encrypts cookies with
//! app-bound encryption that yt-dlp cannot read (DEC-08). The app finds the
//! profiles itself and gives yt-dlp the profile's folder, so yt-dlp needs
//! no `APPDATA` in its (cleaned) environment.

use std::path::{Path, PathBuf};

/// One Firefox profile. `path` is private (it holds the user name): it is
/// stored on the auth profile and passed to yt-dlp, never logged or sent
/// to the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserProfile {
    /// The profile's name in Firefox (`default-release`).
    pub name: String,
    pub path: PathBuf,
    /// The profile Firefox opens by default.
    pub is_default: bool,
    /// `cookies.sqlite` is there (Firefox has been used with it).
    pub has_cookies: bool,
}

/// Firefox's folder for this user, from the app's own environment.
pub fn firefox_root() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Mozilla").join("Firefox"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Library/Application Support/Firefox"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".mozilla/firefox"))
    }
}

/// Profiles listed in `<root>/profiles.ini`, default first. Missing or
/// unreadable file: none.
pub fn firefox_profiles(root: &Path) -> Vec<BrowserProfile> {
    let Ok(text) = std::fs::read_to_string(root.join("profiles.ini")) else {
        return Vec::new();
    };
    let mut sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            sections.push((line[1..line.len() - 1].to_string(), Vec::new()));
        } else if let (Some((k, v)), Some(last)) = (line.split_once('='), sections.last_mut()) {
            last.1.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let get = |kv: &Vec<(String, String)>, key: &str| {
        kv.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.clone())
    };
    // The profile an installation actually opens (`[Install…] Default=`)
    // wins over the legacy `Default=1` flag.
    let install_default: Option<String> = sections
        .iter()
        .filter(|(n, _)| n.starts_with("Install"))
        .find_map(|(_, kv)| get(kv, "Default"));
    let mut out: Vec<BrowserProfile> = Vec::new();
    for (name, kv) in &sections {
        if !name.starts_with("Profile") {
            continue;
        }
        let Some(rel) = get(kv, "Path") else { continue };
        if rel.is_empty() || rel.contains('\0') {
            continue;
        }
        let relative = get(kv, "IsRelative").as_deref() != Some("0");
        let path = if relative {
            // profiles.ini uses `/` on every platform.
            rel.split('/').fold(root.to_path_buf(), |p, s| p.join(s))
        } else {
            PathBuf::from(&rel)
        };
        // A relative path must stay inside Firefox's folder.
        if relative && rel.split('/').any(|s| s == ".." || s.is_empty()) {
            continue;
        }
        let is_default = match &install_default {
            Some(d) => d == &rel,
            None => get(kv, "Default").as_deref() == Some("1"),
        };
        out.push(BrowserProfile {
            name: get(kv, "Name").unwrap_or_else(|| rel.clone()),
            has_cookies: path.join("cookies.sqlite").is_file(),
            path,
            is_default,
        });
    }
    out.sort_by_key(|p| (!p.is_default, !p.has_cookies, p.name.to_lowercase()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_profiles_ini_with_install_default_and_skips_escapes() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        std::fs::create_dir_all(root.join("Profiles/abc.default-release")).unwrap();
        std::fs::write(
            root.join("Profiles/abc.default-release/cookies.sqlite"),
            b"x",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("Profiles/old.default")).unwrap();
        std::fs::write(
            root.join("profiles.ini"),
            "[Install308046B0AF4A39CB]\nDefault=Profiles/abc.default-release\nLocked=1\n\n\
             [Profile1]\nName=default\nIsRelative=1\nPath=Profiles/old.default\nDefault=1\n\n\
             [Profile0]\nName=default-release\nIsRelative=1\nPath=Profiles/abc.default-release\n\n\
             [Profile2]\nName=evil\nIsRelative=1\nPath=../../outside\n\n\
             [General]\nStartWithLastProfile=1\nVersion=2\n",
        )
        .unwrap();
        let p = firefox_profiles(root);
        assert_eq!(p.len(), 2, "{p:?}");
        assert_eq!(p[0].name, "default-release");
        assert!(p[0].is_default && p[0].has_cookies);
        assert_eq!(p[0].path, root.join("Profiles").join("abc.default-release"));
        assert_eq!(p[1].name, "default");
        assert!(!p[1].is_default && !p[1].has_cookies);
    }

    #[test]
    fn no_firefox_means_no_profiles() {
        let d = tempfile::tempdir().unwrap();
        assert!(firefox_profiles(d.path()).is_empty());
    }
}
