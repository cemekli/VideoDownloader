//! Thumbnails (SPEC IMP-06, §11.3 derivation, §14.2 `cache/thumbnails/`).
//!
//! Runs after import, never inside it: a missing or failed thumbnail does
//! not block saving or search. The frame is written to a fresh path in the
//! cache root and recorded as a derived asset of the original.

use crate::fsutil;
use crate::tools::{run, MediaTools, RunError};
use std::path::{Path, PathBuf};
use std::result::Result;
use vv_domain::error::ErrorCode;
use vv_persistence::library::repo::{self, MediaAsset, StorageRoot};
use vv_persistence::library::*;
use vv_persistence::rusqlite::Connection;

/// Derivation config version for this generator's settings.
pub const THUMBNAIL_CONFIG_VERSION: u32 = 1;
pub const THUMBNAIL_WIDTH: u32 = 320;

/// The cache root at `dir` (created on disk and in the DB if needed).
pub fn ensure_cache_root(
    c: &Connection,
    clock: &dyn Clock,
    dir: &Path,
) -> Result<StorageRoot, LibraryError> {
    std::fs::create_dir_all(dir).map_err(|_| LibraryError::Invalid("cannot create cache root"))?;
    let dir =
        fsutil::canonical(dir).map_err(|_| LibraryError::Invalid("cannot resolve cache root"))?;
    let path = dir
        .to_str()
        .ok_or(LibraryError::Invalid("non-Unicode cache root"))?
        .to_owned();
    if let Some(r) = repo::list_storage_roots(c)?
        .into_iter()
        .find(|r| r.kind == StorageRootKind::Cache && r.path == path)
    {
        return Ok(r);
    }
    let now = clock.now_ms();
    let r = StorageRoot {
        id: new_id(),
        kind: StorageRootKind::Cache,
        label: "cache".into(),
        path,
        volume_id: None,
        status: StorageRootStatus::Available,
        created_at: now,
        last_verified_at: Some(now),
    };
    repo::insert_storage_root(c, &r)?;
    Ok(r)
}

#[derive(Debug)]
pub enum ThumbnailError {
    /// No original video asset, or it is not reachable now.
    NoSource(ErrorCode),
    Tool(ErrorCode),
    /// FFmpeg ran but wrote no image (e.g. undecodable frame).
    NoOutput,
    Storage(LibraryError),
}

impl From<LibraryError> for ThumbnailError {
    fn from(e: LibraryError) -> Self {
        ThumbnailError::Storage(e)
    }
}

/// 10 % into the video, at most 5 s, so short clips and title cards work.
pub fn frame_time_ms(duration_ms: Option<i64>) -> u64 {
    duration_ms.map_or(0, |d| (d.max(0) as u64 / 10).min(5_000))
}

pub fn generate_thumbnail(
    c: &Connection,
    clock: &dyn Clock,
    tools: &MediaTools,
    cache: &StorageRoot,
    item_id: &str,
) -> Result<String, ThumbnailError> {
    let original = repo::assets_for_item(c, item_id)?
        .into_iter()
        .find(|a| a.role == AssetRole::Original && a.media_kind == MediaKind::Video)
        .ok_or(ThumbnailError::NoSource(ErrorCode::MediaCorrupt))?;
    let root =
        repo::get_storage_root(c, &original.storage_root_id)?.ok_or(LibraryError::NotFound)?;
    if root.status != StorageRootStatus::Available {
        return Err(ThumbnailError::NoSource(ErrorCode::StorageRootUnavailable));
    }
    let input = fsutil::resolve_ref(Path::new(&root.path), &original.storage_ref);
    if !input.is_file() {
        return Err(ThumbnailError::NoSource(ErrorCode::FileMissing));
    }
    let now = clock.now_ms();
    let mut thumb = MediaAsset::new(
        item_id,
        AssetRole::Thumbnail,
        MediaKind::Image,
        StorageKind::Managed,
        &cache.id,
        "",
        now,
    );
    let storage_ref = format!("thumbnails/{item_id}/{}.jpg", thumb.id);
    let output: PathBuf = fsutil::resolve_ref(Path::new(&cache.path), &storage_ref);
    if let Some(dir) = output.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|_| ThumbnailError::Tool(ErrorCode::PathNotWritable))?;
    }
    let at = frame_time_ms(original.duration_ms);
    let args = vv_ffmpeg::args::thumbnail_args(&input, &output, at)
        .map_err(|_| ThumbnailError::Tool(ErrorCode::InternalError))?;
    let out = run(&tools.ffmpeg, args, tools.thumbnail_timeout).map_err(|e| {
        ThumbnailError::Tool(match e {
            RunError::Spawn(_) => ErrorCode::ToolMissing,
            RunError::TimedOut => ErrorCode::Timeout,
        })
    })?;
    let size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
    if !out.success || size == 0 {
        let _ = std::fs::remove_file(&output);
        return Err(ThumbnailError::NoOutput);
    }
    thumb.storage_ref = storage_ref;
    thumb.mime_type = Some("image/jpeg".into());
    thumb.size_bytes = Some(size as i64);
    thumb.acquired_at = Some(now);
    thumb.acquisition_status = AcquisitionStatus::Ready;
    thumb.integrity_state = IntegrityState::Verified;
    thumb.source_asset_id = Some(original.id.clone());
    thumb.source_fingerprint = original.content_hash.clone();
    thumb.derivation = Some(
        serde_json::json!({
            "generator": "ffmpeg",
            "generator_version": tools.build,
            "config": { "v": THUMBNAIL_CONFIG_VERSION, "width": THUMBNAIL_WIDTH, "at_ms": at },
            "created_at": now,
        })
        .to_string(),
    );
    repo::insert_media_asset(c, &thumb)?;
    repo::set_thumbnail(c, item_id, Some(&thumb.id), now)?;
    Ok(thumb.id)
}

/// An item id and its thumbnail asset id or why there is none.
pub type ThumbnailResult = (String, Result<String, ThumbnailError>);

/// Works through items without a thumbnail. One failure is recorded and the
/// rest continue (IMP-06).
pub fn generate_missing(
    c: &Connection,
    clock: &dyn Clock,
    tools: &MediaTools,
    cache: &StorageRoot,
    limit: u32,
) -> Result<Vec<ThumbnailResult>, LibraryError> {
    let ids = repo::items_missing_thumbnail(c, limit)?;
    Ok(ids
        .into_iter()
        .map(|id| {
            let r = generate_thumbnail(c, clock, tools, cache, &id);
            (id, r)
        })
        .collect())
}
