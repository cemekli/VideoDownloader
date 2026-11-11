//! Settings, first run, portable export/import and backups (STATE.md
//! F2.5–F2.8; SPEC §9.7, §11.6, §15; ADR-0016, ADR-0017).
//!
//! Every file is chosen in a native dialog opened here in Rust; the webview
//! never sends or receives a path. Backups are named by their file name in
//! `<data>/backups/`, which must be one this module listed.

use crate::library::internal;
use crate::library_commands::blocking;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};
use vv_domain::error::{DomainError, ErrorCode};
use vv_persistence::library::repo::{self, StorageRoot};
use vv_persistence::library::{
    new_id, Clock, LibraryError, StorageRootKind, StorageRootStatus, SystemClock,
};
use vv_persistence::portable::{
    self, ConflictPolicy, Document, ExportOptions, ImportOptions, ImportPreview, ImportReport,
    PortableError,
};
use vv_persistence::restore::{self, BackupSummary, RestoreError};
use vv_persistence::rusqlite::Connection;
use vv_persistence::settings::{self, Settings, SettingsPatch};

/// A checked file waiting for the user's next step, by token.
pub enum PendingImport {
    Json {
        doc: Box<Document>,
        root_map: HashMap<String, String>,
    },
    Backup {
        path: PathBuf,
    },
}

fn lib_err(id: &'static str) -> impl Fn(LibraryError) -> DomainError {
    move |_| internal(id)
}

fn now() -> i64 {
    SystemClock.now_ms()
}

fn parent_window<R: Runtime>(app: &AppHandle<R>) -> Option<tauri::WebviewWindow<R>> {
    app.get_webview_window("main")
}

// ---------------------------------------------------------------- settings

#[tauri::command]
pub async fn get_settings<R: Runtime>(app: AppHandle<R>) -> Result<Settings, DomainError> {
    blocking(app, |st| {
        settings::get(&st.get()?.reader()?).map_err(lib_err("settings-read"))
    })
    .await
}

#[tauri::command]
pub async fn update_settings<R: Runtime>(
    app: AppHandle<R>,
    patch: SettingsPatch,
) -> Result<Settings, DomainError> {
    let a = app.clone();
    let s = blocking(app, move |st| {
        st.get()?.write(|c| {
            settings::update(c, &patch, now()).map_err(|e| {
                if matches!(e, LibraryError::Invalid(_)) {
                    DomainError::new(ErrorCode::InputRejected, "settings-invalid")
                } else {
                    internal("settings-write")
                }
            })
        })
    })
    .await?;
    // Running downloads stop and waiting ones are held (DATA-05).
    if let Some(e) = a.try_state::<crate::engine::EngineState>() {
        e.set_offline(s.offline_mode);
    }
    Ok(s)
}

/// First run step 2: the legal notice was read (§9.7).
#[tauri::command]
pub async fn acknowledge_legal<R: Runtime>(app: AppHandle<R>) -> Result<Settings, DomainError> {
    blocking(app, |st| {
        st.get()?.write(|c| {
            settings::set_legal_ack(c, now()).map_err(lib_err("legal-ack"))?;
            settings::get(c).map_err(lib_err("settings-read"))
        })
    })
    .await
}

/// First run finished or skipped; it is not shown again (§9.7).
#[tauri::command]
pub async fn finish_onboarding<R: Runtime>(app: AppHandle<R>) -> Result<Settings, DomainError> {
    blocking(app, |st| {
        st.get()?.write(|c| {
            settings::set_onboarding_done(c, now()).map_err(lib_err("onboarding-done"))?;
            settings::get(c).map_err(lib_err("settings-read"))
        })
    })
    .await
}

/// Where new downloads go, as the first run and Settings show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageOverview {
    /// Folder name only; the full path stays in the backend.
    pub label: String,
    pub is_default: bool,
    pub free_bytes: Option<u64>,
    /// The folder is inside a cloud-synced folder (OneDrive): downloads
    /// would be uploaded and placeholders can go missing offline (§14.2).
    pub cloud_synced: bool,
}

/// OneDrive (and similar) folders: the `OneDrive*` env roots on Windows and
/// any path component named like them.
pub fn is_cloud_synced(path: &Path) -> bool {
    let env_roots = ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from);
    let mut roots = env_roots;
    if roots.any(|r| !r.as_os_str().is_empty() && path.starts_with(&r)) {
        return true;
    }
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy().to_ascii_lowercase();
        s == "onedrive"
            || s.starts_with("onedrive - ")
            || s == "icloud drive"
            || s == "dropbox"
            || s == "google drive"
    })
}

fn free_bytes(path: &Path) -> Option<u64> {
    let mut p = path;
    // The folder may not exist yet: ask the nearest parent that does.
    loop {
        if p.exists() {
            return fs4::available_space(p).ok();
        }
        p = p.parent()?;
    }
}

fn overview_of(path: &Path, is_default: bool) -> StorageOverview {
    StorageOverview {
        label: path.file_name().map_or_else(
            || path.to_string_lossy().into_owned(),
            |n| n.to_string_lossy().into_owned(),
        ),
        is_default,
        free_bytes: free_bytes(path),
        cloud_synced: is_cloud_synced(path),
    }
}

fn active_media_dir(c: &Connection, default: &Path) -> Result<(PathBuf, bool), DomainError> {
    let s = settings::get(c).map_err(lib_err("settings-read"))?;
    if let Some(id) = s.active_download_root_id {
        if let Some(r) = repo::get_storage_root(c, &id).map_err(lib_err("root-read"))? {
            return Ok((PathBuf::from(r.path), false));
        }
    }
    Ok((default.to_path_buf(), true))
}

#[tauri::command]
pub async fn get_storage_overview<R: Runtime>(
    app: AppHandle<R>,
) -> Result<StorageOverview, DomainError> {
    blocking(app, |st| {
        let lib = st.get()?;
        let (path, is_default) = active_media_dir(&lib.reader()?, &lib.default_media_dir())?;
        Ok(overview_of(&path, is_default))
    })
    .await
}

/// The root of `kind` at `path`, created when there is none.
fn ensure_root(
    c: &Connection,
    kind: StorageRootKind,
    path: &Path,
    label: &str,
) -> Result<StorageRoot, DomainError> {
    let canonical = vv_import::fsutil::canonical(path)
        .map_err(|_| DomainError::new(ErrorCode::PathNotWritable, "root-resolve"))?;
    let path = canonical
        .to_str()
        .ok_or_else(|| DomainError::new(ErrorCode::PathNotWritable, "root-non-unicode"))?
        .to_owned();
    if let Some(r) = repo::list_storage_roots(c)
        .map_err(lib_err("roots-read"))?
        .into_iter()
        .find(|r| r.kind == kind && r.path == path)
    {
        return Ok(r);
    }
    let t = now();
    let r = StorageRoot {
        id: new_id(),
        kind,
        label: label.to_owned(),
        path,
        volume_id: None,
        status: StorageRootStatus::Available,
        created_at: t,
        last_verified_at: Some(t),
    };
    repo::insert_storage_root(c, &r).map_err(lib_err("root-insert"))?;
    Ok(r)
}

/// Picks the folder new downloads go to (first run step 3, Settings).
/// `None` when the dialog was closed.
#[tauri::command]
pub async fn choose_media_folder<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<StorageOverview>, DomainError> {
    use tauri_plugin_dialog::DialogExt;
    blocking(app.clone(), move |st| {
        let mut dialog = app.dialog().file();
        if let Some(w) = parent_window(&app) {
            dialog = dialog.set_parent(&w);
        }
        let Some(dir) = dialog
            .blocking_pick_folder()
            .and_then(|p| p.into_path().ok())
        else {
            return Ok(None);
        };
        // A folder we cannot write to is refused now, not at the first download.
        let probe = dir.join(format!(".videovault-write-test-{}", new_id()));
        std::fs::write(&probe, b"").map_err(|_| {
            DomainError::new(ErrorCode::PathNotWritable, "media-folder-not-writable")
        })?;
        let _ = std::fs::remove_file(&probe);
        let label = dir
            .file_name()
            .map_or_else(|| "Media".to_owned(), |n| n.to_string_lossy().into_owned());
        st.get()?.write(|c| {
            let r = ensure_root(c, StorageRootKind::ManagedMedia, &dir, &label)?;
            settings::set_download_root(c, &r.id, now()).map_err(lib_err("settings-write"))?;
            Ok(Some(overview_of(Path::new(&r.path), false)))
        })
    })
    .await
}

// ---------------------------------------------------------------- export / import

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    /// `false` when the save dialog was closed.
    pub saved: bool,
    pub items: usize,
    pub media_references: usize,
    pub bytes: u64,
    pub file_name: Option<String>,
}

/// Writes the portable JSON export to a file the user picks (§15.1).
#[tauri::command]
pub async fn export_library<R: Runtime>(
    app: AppHandle<R>,
    include_absolute_paths: bool,
) -> Result<ExportResult, DomainError> {
    use tauri_plugin_dialog::DialogExt;
    let version = app.package_info().version.to_string();
    blocking(app.clone(), move |st| {
        let t = now();
        let mut dialog = app
            .dialog()
            .file()
            .add_filter("VideoVault library", &["json"])
            .set_file_name(format!(
                "videovault-library-{}.json",
                portable::iso_utc(t).chars().take(10).collect::<String>()
            ));
        if let Some(w) = parent_window(&app) {
            dialog = dialog.set_parent(&w);
        }
        let Some(dest) = dialog.blocking_save_file().and_then(|p| p.into_path().ok()) else {
            return Ok(ExportResult {
                saved: false,
                items: 0,
                media_references: 0,
                bytes: 0,
                file_name: None,
            });
        };
        let c = st.get()?.reader()?;
        let doc = portable::export(
            &c,
            ExportOptions {
                include_absolute_paths,
            },
            &version,
            t,
        )
        .map_err(|_| internal("export-build"))?;
        let bytes = portable::write_export(&doc, &dest)
            .map_err(|_| DomainError::new(ErrorCode::PathNotWritable, "export-write"))?;
        Ok(ExportResult {
            saved: true,
            items: doc.items.len(),
            media_references: doc.media_references.len(),
            bytes,
            file_name: dest.file_name().map(|n| n.to_string_lossy().into_owned()),
        })
    })
    .await
}

pub fn portable_error(e: &PortableError) -> DomainError {
    match e {
        PortableError::UnsupportedSchema { found, supported } => {
            DomainError::new(ErrorCode::ImportSchemaUnsupported, "import-schema")
                .with_detail("found", found.to_string())
                .with_detail("supported", supported.to_string())
        }
        PortableError::UnsafePath(_) => {
            DomainError::new(ErrorCode::InputRejected, "import-unsafe-path")
                .with_detail("reason", "unsafe_path")
        }
        PortableError::TooLarge => DomainError::new(ErrorCode::InputRejected, "import-too-large")
            .with_detail("reason", "too_large"),
        PortableError::Malformed(_) | PortableError::NotAnExport => {
            DomainError::new(ErrorCode::InputRejected, "import-not-an-export")
                .with_detail("reason", "not_an_export")
        }
        PortableError::Io(_) => DomainError::new(ErrorCode::FileMissing, "import-read"),
        PortableError::Library(LibraryError::Invalid(_)) => {
            DomainError::new(ErrorCode::InputRejected, "import-invalid")
                .with_detail("reason", "invalid")
        }
        PortableError::Library(_) => internal("import-apply"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingImportDto {
    pub token: String,
    pub file_name: String,
    pub preview: ImportPreview,
}

/// Opens a JSON export and checks it; nothing changes yet (§15.2 steps 1–3).
/// `None` when the dialog was closed.
#[tauri::command]
pub async fn preview_library_import<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<PendingImportDto>, DomainError> {
    use tauri_plugin_dialog::DialogExt;
    blocking(app.clone(), move |st| {
        let mut dialog = app
            .dialog()
            .file()
            .add_filter("VideoVault library", &["json"]);
        if let Some(w) = parent_window(&app) {
            dialog = dialog.set_parent(&w);
        }
        let Some(path) = dialog.blocking_pick_file().and_then(|p| p.into_path().ok()) else {
            return Ok(None);
        };
        let doc = portable::read_file(&path).map_err(|e| portable_error(&e))?;
        let preview =
            portable::preview(&st.get()?.reader()?, &doc).map_err(|e| portable_error(&e))?;
        let token = new_id();
        st.imports
            .lock()
            .map_err(|_| internal("imports-poisoned"))?
            .insert(
                token.clone(),
                PendingImport::Json {
                    doc: Box::new(doc),
                    root_map: HashMap::new(),
                },
            );
        Ok(Some(PendingImportDto {
            token,
            file_name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            preview,
        }))
    })
    .await
}

/// Points an exported root at a folder on this machine (§15.2 step 6).
/// Returns the folder's name, or `None` when the dialog was closed.
#[tauri::command]
pub async fn map_import_root<R: Runtime>(
    app: AppHandle<R>,
    token: String,
    root_id: String,
) -> Result<Option<String>, DomainError> {
    use tauri_plugin_dialog::DialogExt;
    blocking(app.clone(), move |st| {
        let kind = {
            let map = st
                .imports
                .lock()
                .map_err(|_| internal("imports-poisoned"))?;
            let Some(PendingImport::Json { doc, .. }) = map.get(&token) else {
                return Err(internal("import-token"));
            };
            let r = doc
                .storage_roots
                .iter()
                .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(root_id.as_str()))
                .ok_or_else(|| internal("import-root"))?;
            match r.get("kind").and_then(|v| v.as_str()) {
                Some("external_import") => StorageRootKind::ExternalImport,
                _ => StorageRootKind::ManagedMedia,
            }
        };
        let mut dialog = app.dialog().file();
        if let Some(w) = parent_window(&app) {
            dialog = dialog.set_parent(&w);
        }
        let Some(dir) = dialog
            .blocking_pick_folder()
            .and_then(|p| p.into_path().ok())
        else {
            return Ok(None);
        };
        let label = dir
            .file_name()
            .map_or_else(|| "Media".to_owned(), |n| n.to_string_lossy().into_owned());
        let local = st.get()?.write(|c| ensure_root(c, kind, &dir, &label))?;
        if let Some(PendingImport::Json { root_map, .. }) = st
            .imports
            .lock()
            .map_err(|_| internal("imports-poisoned"))?
            .get_mut(&token)
        {
            root_map.insert(root_id, local.id);
        }
        Ok(Some(label))
    })
    .await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyImport {
    pub token: String,
    #[serde(default)]
    pub policy: ConflictPolicy,
    /// The user approved absolute file paths from the export (AC-51).
    #[serde(default)]
    pub use_original_locations: bool,
}

/// Applies a checked import in one transaction (§15.2 steps 4–7).
#[tauri::command]
pub async fn apply_library_import<R: Runtime>(
    app: AppHandle<R>,
    request: ApplyImport,
) -> Result<ImportReport, DomainError> {
    blocking(app, move |st| {
        let pending = st
            .imports
            .lock()
            .map_err(|_| internal("imports-poisoned"))?
            .remove(&request.token);
        let Some(PendingImport::Json { doc, root_map }) = pending else {
            return Err(internal("import-token"));
        };
        let opts = ImportOptions {
            policy: request.policy,
            root_map,
            use_original_locations: request.use_original_locations,
        };
        let lib = st.get()?;
        let rep =
            lib.write(|c| portable::apply(c, &doc, &opts, now()).map_err(|e| portable_error(&e)))?;
        lib.refresh_storage();
        Ok(rep)
    })
    .await
}

#[tauri::command]
pub async fn cancel_pending<R: Runtime>(
    app: AppHandle<R>,
    token: String,
) -> Result<(), DomainError> {
    blocking(app, move |st| {
        st.imports
            .lock()
            .map_err(|_| internal("imports-poisoned"))?
            .remove(&token);
        Ok(())
    })
    .await
}

// ---------------------------------------------------------------- backups

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupDto {
    pub name: String,
    pub kind: &'static str,
    pub created_at: i64,
    pub size_bytes: u64,
}

fn backup_dto(b: &restore::BackupFile) -> BackupDto {
    BackupDto {
        name: b
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        kind: b.kind.as_str(),
        created_at: b.created_ms,
        size_bytes: b.size_bytes,
    }
}

pub fn restore_error(e: &RestoreError) -> DomainError {
    match e {
        RestoreError::TooNew { backup, app } => {
            DomainError::new(ErrorCode::ImportSchemaUnsupported, "restore-too-new")
                .with_detail("found", backup.to_string())
                .with_detail("supported", app.to_string())
        }
        RestoreError::Unreadable => DomainError::new(ErrorCode::MediaCorrupt, "restore-unreadable")
            .with_detail("reason", "unreadable"),
        RestoreError::NotALibrary | RestoreError::Mismatch => {
            DomainError::new(ErrorCode::InputRejected, "restore-not-a-library")
                .with_detail("reason", "not_a_library")
        }
        RestoreError::Migration(_) => {
            DomainError::new(ErrorCode::MigrationFailed, "restore-migration")
        }
        RestoreError::Io(_) => DomainError::new(ErrorCode::PathNotWritable, "restore-io"),
        RestoreError::Storage(_) => internal("restore-storage"),
    }
}

#[tauri::command]
pub async fn backup_library<R: Runtime>(app: AppHandle<R>) -> Result<BackupDto, DomainError> {
    blocking(app, |st| {
        let lib = st.get()?;
        let dir = lib.backup_dir();
        let path =
            lib.with_db(|db| restore::backup_now(db, &dir, now()).map_err(|e| restore_error(&e)))?;
        restore::list(&dir)
            .iter()
            .find(|b| b.path == path)
            .map(backup_dto)
            .ok_or_else(|| internal("backup-missing"))
    })
    .await
}

#[tauri::command]
pub async fn list_backups<R: Runtime>(app: AppHandle<R>) -> Result<Vec<BackupDto>, DomainError> {
    blocking(app, |st| {
        Ok(restore::list(&st.get()?.backup_dir())
            .iter()
            .map(backup_dto)
            .collect())
    })
    .await
}

/// A listed backup by its file name; any other name is refused.
fn listed_backup(dir: &Path, name: &str) -> Result<PathBuf, DomainError> {
    restore::list(dir)
        .into_iter()
        .find(|b| b.path.file_name().is_some_and(|n| n == name))
        .map(|b| b.path)
        .ok_or_else(|| DomainError::new(ErrorCode::FileMissing, "backup-not-listed"))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPreview {
    /// For `restore_backup` when the file came from the dialog.
    pub token: Option<String>,
    pub name: String,
    pub schema_version: u32,
    pub items: i64,
    /// Items in Recently removed; their 30 days start at restore (AC-57).
    pub removed: i64,
    pub collections: i64,
    pub tags: i64,
    /// Media files referenced; they are not inside the backup (§15.3).
    pub media_files: i64,
}

fn backup_preview(token: Option<String>, name: String, s: BackupSummary) -> BackupPreview {
    BackupPreview {
        token,
        name,
        schema_version: s.schema_version,
        items: s.items,
        removed: s.removed,
        collections: s.collections,
        tags: s.tags,
        media_files: s.media_files,
    }
}

/// What a listed backup holds, or (no name) a backup file the user picks.
#[tauri::command]
pub async fn inspect_backup<R: Runtime>(
    app: AppHandle<R>,
    name: Option<String>,
) -> Result<Option<BackupPreview>, DomainError> {
    use tauri_plugin_dialog::DialogExt;
    blocking(app.clone(), move |st| {
        let lib = st.get()?;
        if let Some(name) = name {
            let path = listed_backup(&lib.backup_dir(), &name)?;
            let s = restore::inspect(&path).map_err(|e| restore_error(&e))?;
            return Ok(Some(backup_preview(None, name, s)));
        }
        let mut dialog = app
            .dialog()
            .file()
            .add_filter("VideoVault backup", &["sqlite"]);
        if let Some(w) = parent_window(&app) {
            dialog = dialog.set_parent(&w);
        }
        let Some(path) = dialog.blocking_pick_file().and_then(|p| p.into_path().ok()) else {
            return Ok(None);
        };
        let s = restore::inspect(&path).map_err(|e| restore_error(&e))?;
        let token = new_id();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        st.imports
            .lock()
            .map_err(|_| internal("imports-poisoned"))?
            .insert(token.clone(), PendingImport::Backup { path });
        Ok(Some(backup_preview(Some(token), name, s)))
    })
    .await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreRequest {
    /// A listed backup...
    #[serde(default)]
    pub name: Option<String>,
    /// ...or the token of a file picked in `inspect_backup`.
    #[serde(default)]
    pub token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub items: i64,
    pub removed: i64,
    /// The library as it was, saved before it was replaced.
    pub pre_restore_backup: String,
    pub migrated: Vec<u32>,
    /// Media files the restored library references but cannot find now.
    pub media_missing: i64,
}

/// Replaces the library with a backup (§15.3, AC-50). The current state is
/// saved first; a bad file changes nothing.
#[tauri::command]
pub async fn restore_backup<R: Runtime>(
    app: AppHandle<R>,
    request: RestoreRequest,
) -> Result<RestoreResult, DomainError> {
    blocking(app, move |st| {
        let lib = st.get()?;
        let dir = lib.backup_dir();
        let path = match (&request.name, &request.token) {
            (Some(name), None) => listed_backup(&dir, name)?,
            (None, Some(token)) => match st.imports.lock().map_err(|_| internal("imports-poisoned"))?.remove(token) {
                Some(PendingImport::Backup { path }) => path,
                _ => return Err(internal("restore-token")),
            },
            _ => return Err(internal("restore-source")),
        };
        let rep = lib.with_db(|db| restore::restore(db, &path, &dir, now()).map_err(|e| restore_error(&e)))?;
        lib.refresh_storage();
        let media_missing = lib
            .reader()?
            .query_row(
                "SELECT count(*) FROM media_asset a JOIN storage_root r ON r.id = a.storage_root_id \
                 WHERE a.role = 'original' AND (a.integrity_state = 'missing' OR r.status <> 'available')",
                [],
                |r| r.get(0),
            )
            .map_err(|_| internal("restore-count"))?;
        Ok(RestoreResult {
            items: rep.summary.items,
            removed: rep.summary.removed,
            pre_restore_backup: rep.pre_restore_backup.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            migrated: rep.migrated,
            media_missing,
        })
    })
    .await
}

/// Offline Mode check for anything that would use the network (DATA-05).
pub fn ensure_online(c: &Connection) -> Result<(), DomainError> {
    if settings::offline_mode(c).map_err(lib_err("settings-read"))? {
        return Err(DomainError::new(ErrorCode::OfflineMode, "offline-mode"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_folders_are_recognised() {
        assert!(is_cloud_synced(Path::new("C:/Users/x/OneDrive/Videos")));
        assert!(is_cloud_synced(Path::new(
            "C:/Users/x/OneDrive - Contoso/Videos"
        )));
        assert!(!is_cloud_synced(Path::new("D:/Media/VideoVault")));
    }

    #[test]
    fn schema_errors_name_the_versions_and_paths_are_never_details() {
        let e = portable_error(&PortableError::UnsupportedSchema {
            found: 9,
            supported: 1,
        });
        assert_eq!(e.code, ErrorCode::ImportSchemaUnsupported);
        assert_eq!(e.safe_details.get("found").map(String::as_str), Some("9"));
        let e = portable_error(&PortableError::UnsafePath("../../secret".into()));
        assert!(!serde_json::to_string(&e).unwrap().contains("secret"));
        let e = restore_error(&RestoreError::TooNew { backup: 7, app: 4 });
        assert_eq!(e.code, ErrorCode::ImportSchemaUnsupported);
    }

    #[test]
    fn only_listed_backups_can_be_named() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("backup-5.sqlite"), b"x").unwrap();
        std::fs::write(d.path().join("other.sqlite"), b"x").unwrap();
        assert!(listed_backup(d.path(), "backup-5.sqlite").is_ok());
        for bad in ["other.sqlite", "../backup-5.sqlite", "backup-6.sqlite"] {
            assert!(listed_backup(d.path(), bad).is_err(), "{bad}");
        }
    }
}
