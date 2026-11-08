//! App settings (SPEC §11.6 `AppSettings`; STATE.md F2.7, F2.8; ADR-0016).
//!
//! One JSON value per key in `app_setting`. Each key has a fixed shape here;
//! a value that does not parse (an older or newer app wrote it, or it was
//! edited by hand) reads as the default instead of failing the app.

use crate::library::{LibraryError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// SPEC §9.7: dark is the default.
    #[default]
    Dark,
    Light,
    /// Follows the operating system.
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UpdateCheck {
    /// SPEC §9.7: checking by hand is the default.
    #[default]
    Manual,
    Automatic,
}

/// Every setting with its value (defaults filled in).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// DATA-05: VideoVault and the tools it runs do not use the network.
    pub offline_mode: bool,
    pub theme: Theme,
    pub update_check: UpdateCheck,
    /// UI language. Only `en` ships today (§9.6).
    pub locale: String,
    /// First run finished or skipped (§9.7).
    pub onboarding_completed_at: Option<i64>,
    /// The legal notice was acknowledged (§9.7 step 2).
    pub legal_ack_at: Option<i64>,
    /// `StorageRoot(kind = managed_media)` new downloads go to (§11.6).
    pub active_download_root_id: Option<String>,
}

impl Settings {
    pub fn defaults() -> Self {
        Settings {
            locale: "en".into(),
            ..Default::default()
        }
    }
}

/// A change to one setting. Unknown keys are refused by deserialization.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    pub offline_mode: Option<bool>,
    pub theme: Option<Theme>,
    pub update_check: Option<UpdateCheck>,
    pub locale: Option<String>,
}

/// Locales the UI has messages for.
pub const LOCALES: &[&str] = &["en"];

const OFFLINE: &str = "offline_mode";
const THEME: &str = "theme";
const UPDATE: &str = "update_check";
const LOCALE: &str = "locale";
const ONBOARDING: &str = "onboarding_completed_at";
const LEGAL: &str = "legal_ack_at";
const DOWNLOAD_ROOT: &str = "active_download_root_id";

fn read_raw(c: &Connection, key: &str) -> Result<Option<String>> {
    Ok(
        c.query_row("SELECT value FROM app_setting WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?,
    )
}

fn read<T: serde::de::DeserializeOwned>(c: &Connection, key: &str) -> Result<Option<T>> {
    Ok(read_raw(c, key)?.and_then(|v| serde_json::from_str(&v).ok()))
}

fn write<T: Serialize>(c: &Connection, key: &str, value: &T, now_ms: i64) -> Result<()> {
    let json = serde_json::to_string(value).map_err(|_| LibraryError::Invalid("setting"))?;
    c.execute(
        "INSERT INTO app_setting (key, value, updated_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, json, now_ms],
    )?;
    Ok(())
}

pub fn get(c: &Connection) -> Result<Settings> {
    let d = Settings::defaults();
    Ok(Settings {
        offline_mode: read(c, OFFLINE)?.unwrap_or(d.offline_mode),
        theme: read(c, THEME)?.unwrap_or(d.theme),
        update_check: read(c, UPDATE)?.unwrap_or(d.update_check),
        locale: read::<String>(c, LOCALE)?
            .filter(|l| LOCALES.contains(&l.as_str()))
            .unwrap_or(d.locale),
        onboarding_completed_at: read(c, ONBOARDING)?.flatten(),
        legal_ack_at: read(c, LEGAL)?.flatten(),
        active_download_root_id: read(c, DOWNLOAD_ROOT)?.flatten(),
    })
}

/// Applies `patch` and returns the settings after it.
pub fn update(c: &Connection, patch: &SettingsPatch, now_ms: i64) -> Result<Settings> {
    if let Some(v) = patch.offline_mode {
        write(c, OFFLINE, &v, now_ms)?;
    }
    if let Some(v) = patch.theme {
        write(c, THEME, &v, now_ms)?;
    }
    if let Some(v) = patch.update_check {
        write(c, UPDATE, &v, now_ms)?;
    }
    if let Some(v) = &patch.locale {
        if !LOCALES.contains(&v.as_str()) {
            return Err(LibraryError::Invalid("unknown locale"));
        }
        write(c, LOCALE, v, now_ms)?;
    }
    get(c)
}

pub fn offline_mode(c: &Connection) -> Result<bool> {
    Ok(read(c, OFFLINE)?.unwrap_or(false))
}

pub fn set_legal_ack(c: &Connection, now_ms: i64) -> Result<()> {
    write(c, LEGAL, &Some(now_ms), now_ms)
}

pub fn set_onboarding_done(c: &Connection, now_ms: i64) -> Result<()> {
    write(c, ONBOARDING, &Some(now_ms), now_ms)
}

pub fn set_download_root(c: &Connection, root_id: &str, now_ms: i64) -> Result<()> {
    write(c, DOWNLOAD_ROOT, &Some(root_id), now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Database, OpenOptions};

    fn db() -> (tempfile::TempDir, Database) {
        let d = tempfile::tempdir().unwrap();
        let db = Database::open(&d.path().join("s.sqlite"), &OpenOptions::default()).unwrap();
        (d, db)
    }

    #[test]
    fn defaults_then_patch_then_read_back() {
        let (_d, mut db) = db();
        let c = db.writer();
        let s = get(c).unwrap();
        assert_eq!(s, Settings::defaults());
        assert_eq!(s.theme, Theme::Dark);
        assert_eq!(s.update_check, UpdateCheck::Manual);
        assert!(!s.offline_mode);

        let patch: SettingsPatch =
            serde_json::from_value(serde_json::json!({ "offlineMode": true, "theme": "light" }))
                .unwrap();
        let s = update(c, &patch, 5).unwrap();
        assert!(s.offline_mode);
        assert_eq!(s.theme, Theme::Light);
        assert!(offline_mode(c).unwrap());

        set_legal_ack(c, 7).unwrap();
        set_onboarding_done(c, 8).unwrap();
        let s = get(c).unwrap();
        assert_eq!(s.legal_ack_at, Some(7));
        assert_eq!(s.onboarding_completed_at, Some(8));
    }

    #[test]
    fn bad_values_read_as_defaults_and_unknown_input_is_refused() {
        let (_d, mut db) = db();
        let c = db.writer();
        c.execute(
            "INSERT INTO app_setting (key, value, updated_at) VALUES ('theme', '\"neon\"', 1)",
            [],
        )
        .unwrap();
        assert_eq!(get(c).unwrap().theme, Theme::Dark);
        assert!(serde_json::from_value::<SettingsPatch>(serde_json::json!({ "x": 1 })).is_err());
        let p = SettingsPatch {
            offline_mode: None,
            theme: None,
            update_check: None,
            locale: Some("xx".into()),
        };
        assert!(update(c, &p, 1).is_err());
    }
}
