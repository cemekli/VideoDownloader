//! Player commands (STATE.md F2.4; SPEC PLAY-01…04, §11.4; ADR-0004,
//! ADR-0016).
//!
//! `get_playback` lists what the player may load: original and preview
//! files of the item, each with its `vvmedia` URL, whether its folder is
//! reachable, and what ADR-0004's matrix says about its codec. The player
//! still decides by actually loading the file (`canPlayType` is not
//! trusted). Opening a file in another program or the source page happens
//! here in Rust with the path resolved from the database.

use crate::data_commands::ensure_online;
use crate::library::internal;
use crate::library_commands::{blocking, item_detail, ItemDetail};
use crate::media_protocol::{asset_file, media_url};
use serde::Serialize;
use std::path::Path;
use tauri::{AppHandle, Manager, Runtime};
use vv_domain::error::{DomainError, ErrorCode};
use vv_persistence::library::playback::{self, PlaybackState};
use vv_persistence::library::repo;
use vv_persistence::library::{
    AssetRole, Clock, IntegrityState, MediaKind, StorageRootStatus, SystemClock,
};
use vv_persistence::rusqlite::Connection;

/// ADR-0004 measured on WebView2: H.264, VP8, VP9 and AV1 play in MP4,
/// WebM, MKV and MOV; HEVC played sound only. Other codecs are unknown.
pub fn codec_support(media_kind: MediaKind, video_codec: Option<&str>) -> &'static str {
    match (
        media_kind,
        video_codec.map(str::to_ascii_lowercase).as_deref(),
    ) {
        (MediaKind::Audio, _) => "supported",
        (_, Some("h264" | "avc1" | "vp8" | "vp9" | "av1")) => "supported",
        (_, Some("hevc" | "h265" | "hvc1" | "hev1")) => "unsupported",
        (_, Some(_)) => "unknown",
        (_, None) => "unknown",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackSource {
    pub asset_id: String,
    pub url: String,
    pub role: &'static str,
    pub media_kind: &'static str,
    pub container: Option<String>,
    pub video_codec: Option<String>,
    pub duration_ms: Option<i64>,
    /// Its folder is connected and the file was not found missing.
    pub available: bool,
    /// `supported`, `unsupported` (HEVC) or `unknown` (ADR-0004).
    pub support: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackStateDto {
    pub asset_id: String,
    pub position_ms: i64,
    pub duration_ms: Option<i64>,
    pub last_played_at: i64,
    pub completed: bool,
    /// Offer "Resume from …" (PLAY-02).
    pub resumable: bool,
}

impl From<PlaybackState> for PlaybackStateDto {
    fn from(s: PlaybackState) -> Self {
        PlaybackStateDto {
            resumable: s.resumable(),
            asset_id: s.asset_id,
            position_ms: s.position_ms,
            duration_ms: s.duration_ms,
            last_played_at: s.last_played_at,
            completed: s.completed_at.is_some(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackInfo {
    pub item_id: String,
    pub title: String,
    /// Playable files, originals first.
    pub sources: Vec<PlaybackSource>,
    pub state: Option<PlaybackStateDto>,
    /// The item has a source page (PLAY-03: "Open source" for bookmarks).
    pub has_source_url: bool,
}

pub fn playback_info(c: &Connection, item_id: &str) -> Result<PlaybackInfo, DomainError> {
    let it = repo::get_item(c, item_id)
        .map_err(|_| internal("playback-item"))?
        .filter(|i| i.removed_at.is_none())
        .ok_or_else(|| internal("item-not-found"))?;
    let mut assets: Vec<_> = repo::assets_for_item(c, item_id)
        .map_err(|_| internal("playback-assets"))?
        .into_iter()
        .filter(|a| {
            matches!(a.role, AssetRole::Original | AssetRole::Preview)
                && matches!(a.media_kind, MediaKind::Video | MediaKind::Audio)
        })
        .collect();
    assets.sort_by_key(|a| (a.role != AssetRole::Original, a.created_at));
    let mut sources = Vec::new();
    for a in assets {
        let root =
            repo::get_storage_root(c, &a.storage_root_id).map_err(|_| internal("playback-root"))?;
        let available = root.is_some_and(|r| r.status == StorageRootStatus::Available)
            && a.integrity_state != IntegrityState::Missing;
        sources.push(PlaybackSource {
            url: media_url(&a.id),
            role: a.role.as_str(),
            media_kind: a.media_kind.as_str(),
            support: codec_support(a.media_kind, a.video_codec.as_deref()),
            container: a.container,
            video_codec: a.video_codec,
            duration_ms: a.duration_ms,
            available,
            asset_id: a.id,
        });
    }
    Ok(PlaybackInfo {
        title: it.display_title(),
        item_id: it.id.clone(),
        sources,
        state: playback::get(c, item_id)
            .map_err(|_| internal("playback-state"))?
            .map(Into::into),
        has_source_url: it.canonical_url.is_some(),
    })
}

#[tauri::command]
pub async fn get_playback<R: Runtime>(
    app: AppHandle<R>,
    item_id: String,
) -> Result<PlaybackInfo, DomainError> {
    blocking(app, move |st| playback_info(&st.get()?.reader()?, &item_id)).await
}

/// Records where the user is; called while playing and on pause/close.
#[tauri::command]
pub async fn save_playback_position<R: Runtime>(
    app: AppHandle<R>,
    item_id: String,
    asset_id: String,
    position_ms: i64,
    duration_ms: Option<i64>,
) -> Result<PlaybackStateDto, DomainError> {
    blocking(app, move |st| {
        st.get()?.write(|c| {
            playback::save(
                c,
                &item_id,
                &asset_id,
                position_ms,
                duration_ms,
                SystemClock.now_ms(),
            )
            .map(Into::into)
            .map_err(|e| match e {
                vv_persistence::library::LibraryError::Invalid(_)
                | vv_persistence::library::LibraryError::NotFound => {
                    DomainError::new(ErrorCode::InputRejected, "playback-save-invalid")
                }
                _ => internal("playback-save"),
            })
        })
    })
    .await
}

/// The file of a playable asset, or why there is none.
pub fn playable_file(c: &Connection, asset_id: &str) -> Result<std::path::PathBuf, DomainError> {
    let (root, storage_ref, status) = vv_persistence::library::view::media_location(c, asset_id)
        .map_err(|_| internal("media-location"))?
        .ok_or_else(|| DomainError::new(ErrorCode::FileMissing, "media-unknown-asset"))?;
    if status != StorageRootStatus::Available {
        return Err(DomainError::new(
            ErrorCode::StorageRootUnavailable,
            "media-root-unavailable",
        ));
    }
    asset_file(Path::new(&root), &storage_ref)
        .ok_or_else(|| DomainError::new(ErrorCode::FileMissing, "media-file-missing"))
}

/// Opens the file in the system's default player (PLAY-04, ADR-0004).
#[tauri::command]
pub async fn open_externally<R: Runtime>(
    app: AppHandle<R>,
    asset_id: String,
) -> Result<(), DomainError> {
    blocking(app, move |st| {
        let path = playable_file(&st.get()?.reader()?, &asset_id)?;
        tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|_| internal("open-externally"))
    })
    .await
}

/// Opens the item's source page in the browser (PLAY-03). Refused in
/// Offline Mode (DATA-05).
#[tauri::command]
pub async fn open_source<R: Runtime>(
    app: AppHandle<R>,
    item_id: String,
) -> Result<(), DomainError> {
    blocking(app, move |st| {
        let c = st.get()?.reader()?;
        ensure_online(&c)?;
        let url = repo::get_item(&c, &item_id)
            .map_err(|_| internal("open-source-item"))?
            .and_then(|i| i.canonical_url)
            .ok_or_else(|| internal("open-source-no-url"))?;
        // Stored URLs passed the policy at save; checked again before they leave the app.
        let url = vv_domain::url_policy::validate_capture_url(&url)
            .map_err(|e| DomainError::new(ErrorCode::from(&e), "open-source-url"))?;
        tauri_plugin_opener::open_url(url.as_str(), None::<&str>)
            .map_err(|_| internal("open-source"))
    })
    .await
}

/// Points a missing file at its new location (IMP-07). `None` when the
/// dialog was closed.
#[tauri::command]
pub async fn relink_asset<R: Runtime>(
    app: AppHandle<R>,
    asset_id: String,
) -> Result<Option<ItemDetail>, DomainError> {
    use tauri_plugin_dialog::DialogExt;
    use vv_import::relink::{relink, RelinkError};
    blocking(app.clone(), move |st| {
        let lib = st.get()?;
        let Some(tools) = lib.tools() else {
            return Err(DomainError::new(ErrorCode::ToolMissing, "relink-no-tools"));
        };
        let mut dialog = app
            .dialog()
            .file()
            .add_filter("Video and audio", vv_import::fsutil::MEDIA_EXTENSIONS);
        if let Some(w) = app.get_webview_window("main") {
            dialog = dialog.set_parent(&w);
        }
        let Some(file) = dialog.blocking_pick_file().and_then(|p| p.into_path().ok()) else {
            return Ok(None);
        };
        let item_id = lib.write(|c| {
            relink(c, &SystemClock, tools, &asset_id, &file, false).map_err(|e| match e {
                RelinkError::NotFound => {
                    DomainError::new(ErrorCode::FileMissing, "relink-unknown-asset")
                }
                RelinkError::Unreadable => {
                    DomainError::new(ErrorCode::FileMissing, "relink-unreadable")
                }
                RelinkError::Rejected(code) => DomainError::new(code, "relink-rejected"),
                RelinkError::ContentDiffers => {
                    DomainError::new(ErrorCode::InputRejected, "relink-content-differs")
                        .with_detail("reason", "content_differs")
                }
                RelinkError::AlreadyInLibrary { .. } => {
                    DomainError::new(ErrorCode::DuplicateItem, "relink-already-in-library")
                }
                RelinkError::Storage(_) => internal("relink-storage"),
            })?;
            repo::get_media_asset(c, &asset_id)
                .map_err(|_| internal("relink-read"))?
                .map(|a| a.library_item_id)
                .ok_or_else(|| internal("relink-read"))
        })?;
        Ok(Some(item_detail(&lib.reader()?, &item_id)?))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use vv_persistence::library::repo::{LibraryItem, MediaAsset, StorageRoot};
    use vv_persistence::library::*;
    use vv_persistence::{Database, OpenOptions};

    #[test]
    fn codec_support_follows_the_playback_matrix() {
        assert_eq!(codec_support(MediaKind::Video, Some("h264")), "supported");
        assert_eq!(codec_support(MediaKind::Video, Some("AV1")), "supported");
        assert_eq!(codec_support(MediaKind::Video, Some("hevc")), "unsupported");
        assert_eq!(
            codec_support(MediaKind::Video, Some("mpeg2video")),
            "unknown"
        );
        assert_eq!(codec_support(MediaKind::Audio, None), "supported");
    }

    #[test]
    fn playback_info_lists_playable_files_with_urls_and_no_paths() {
        let d = tempfile::tempdir().unwrap();
        let mut db = Database::open(&d.path().join("l.sqlite"), &OpenOptions::default()).unwrap();
        let c = db.writer();
        let mut it = LibraryItem::new(ItemKind::LocalVideo, SourcePlatform::Local, 1);
        it.title = Some("Clip".into());
        repo::insert_item(c, &it).unwrap();
        let root = StorageRoot {
            id: new_id(),
            kind: StorageRootKind::ExternalImport,
            label: "Videos".into(),
            path: d.path().to_string_lossy().into_owned(),
            volume_id: None,
            status: StorageRootStatus::Available,
            created_at: 1,
            last_verified_at: None,
        };
        repo::insert_storage_root(c, &root).unwrap();
        let mut a = MediaAsset::new(
            &it.id,
            AssetRole::Original,
            MediaKind::Video,
            StorageKind::ExternalReference,
            &root.id,
            "clip.mkv",
            1,
        );
        a.video_codec = Some("hevc".into());
        repo::insert_media_asset(c, &a).unwrap();
        std::fs::write(d.path().join("clip.mkv"), b"x").unwrap();

        let info = playback_info(c, &it.id).unwrap();
        assert_eq!(info.sources.len(), 1);
        assert_eq!(info.sources[0].support, "unsupported");
        assert!(info.sources[0].available);
        assert!(info.sources[0].url.ends_with(&a.id));
        assert!(!info.has_source_url);
        assert!(!serde_json::to_string(&info)
            .unwrap()
            .contains(&*d.path().to_string_lossy()));
        assert_eq!(
            playable_file(c, &a.id).unwrap(),
            std::fs::canonicalize(d.path().join("clip.mkv")).unwrap()
        );
    }
}
