//! Read models for the Library screen (SPEC §9.2, §9.3; STATE.md F1.7).
//!
//! The card state comes from `media_asset` rows and their storage root, never
//! from `library_item.kind` (INV-02). A root that cannot be reached right now
//! is its own state, not "missing" (DATA-09).

use super::*;
use rusqlite::{types::Value, Connection};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LocalState {
    /// No original media on disk (a bookmark or a remote item not downloaded).
    BookmarkOnly,
    /// An original is on a reachable root and was found there.
    LocalReady,
    /// The original's root is not reachable (unplugged drive, offline share).
    Disconnected,
    /// The root is reachable but the file is gone or unreadable.
    Missing,
}

impl LocalState {
    pub fn as_str(self) -> &'static str {
        match self {
            LocalState::BookmarkOnly => "bookmark_only",
            LocalState::LocalReady => "local_ready",
            LocalState::Disconnected => "disconnected",
            LocalState::Missing => "missing",
        }
    }
}

/// Local state of each item in `ids`. Items without a ready original are
/// `BookmarkOnly`; a partial download is not media yet (§13.3).
pub fn local_states(c: &Connection, ids: &[&str]) -> Result<HashMap<String, LocalState>> {
    let mut out: HashMap<String, LocalState> = ids
        .iter()
        .map(|id| ((*id).to_owned(), LocalState::BookmarkOnly))
        .collect();
    if ids.is_empty() {
        return Ok(out);
    }
    let ph = ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT a.library_item_id, a.integrity_state, r.status FROM media_asset a \
         JOIN storage_root r ON r.id = a.storage_root_id \
         WHERE a.role = 'original' AND a.acquisition_status = 'ready' \
         AND a.library_item_id IN ({ph})"
    );
    let params: Vec<Value> = ids.iter().map(|s| Value::Text((*s).to_owned())).collect();
    let mut stmt = c.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params), |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, IntegrityState>(1)?,
            r.get::<_, StorageRootStatus>(2)?,
        ))
    })?;
    for row in rows {
        let (id, integrity, root) = row?;
        let this = if root != StorageRootStatus::Available {
            LocalState::Disconnected
        } else {
            match integrity {
                IntegrityState::Missing | IntegrityState::Corrupt => LocalState::Missing,
                IntegrityState::Unknown | IntegrityState::Verified | IntegrityState::Modified => {
                    LocalState::LocalReady
                }
            }
        };
        let slot = out.entry(id).or_insert(LocalState::BookmarkOnly);
        // One playable original is enough; otherwise "disconnected" says more
        // than "missing" because the file may well be on the absent drive.
        let rank = |s: LocalState| match s {
            LocalState::LocalReady => 3,
            LocalState::Disconnected => 2,
            LocalState::Missing => 1,
            LocalState::BookmarkOnly => 0,
        };
        if rank(this) > rank(*slot) {
            *slot = this;
        }
    }
    Ok(out)
}

/// What the inspector shows about one stored file. `file_name` is the last
/// path component only; the full path stays in the backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetView {
    pub id: String,
    pub role: AssetRole,
    pub file_name: String,
    pub root_label: String,
    pub root_status: StorageRootStatus,
    pub integrity_state: IntegrityState,
    pub size_bytes: Option<i64>,
    pub container: Option<String>,
    pub video_codec: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
}

pub fn asset_views(c: &Connection, item_id: &str) -> Result<Vec<AssetView>> {
    let mut stmt = c.prepare(
        "SELECT a.id, a.role, a.storage_ref, r.label, r.status, a.integrity_state, a.size_bytes, \
         a.container, a.video_codec, a.width, a.height, a.duration_ms \
         FROM media_asset a JOIN storage_root r ON r.id = a.storage_root_id \
         WHERE a.library_item_id = ?1 AND a.role = 'original' ORDER BY a.created_at, a.id",
    )?;
    let v = stmt
        .query_map([item_id], |r| {
            let storage_ref: String = r.get(2)?;
            let file_name = storage_ref
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or_default()
                .to_owned();
            Ok(AssetView {
                id: r.get(0)?,
                role: r.get(1)?,
                file_name,
                root_label: r.get(3)?,
                root_status: r.get(4)?,
                integrity_state: r.get(5)?,
                size_bytes: r.get(6)?,
                container: r.get(7)?,
                video_codec: r.get(8)?,
                width: r.get(9)?,
                height: r.get(10)?,
                duration_ms: r.get(11)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

/// Where a thumbnail asset lives, if `asset_id` is a thumbnail in a cache
/// root. Used by the thumbnail protocol, which serves nothing else.
/// Where a playable file is: `(root path, storage_ref, root status)` for an
/// original or preview video/audio asset (the media protocol, F2.4).
pub fn media_location(
    c: &Connection,
    asset_id: &str,
) -> Result<Option<(String, String, StorageRootStatus)>> {
    use rusqlite::OptionalExtension;
    Ok(c.query_row(
        "SELECT r.path, a.storage_ref, r.status FROM media_asset a \
         JOIN storage_root r ON r.id = a.storage_root_id \
         WHERE a.id = ?1 AND a.role IN ('original', 'preview') AND a.media_kind IN ('video', 'audio') \
         AND r.kind IN ('managed_media', 'external_import', 'cache')",
        [asset_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .optional()?)
}

pub fn thumbnail_location(c: &Connection, asset_id: &str) -> Result<Option<(String, String)>> {
    use rusqlite::OptionalExtension;
    Ok(c.query_row(
        "SELECT r.path, a.storage_ref FROM media_asset a \
         JOIN storage_root r ON r.id = a.storage_root_id \
         WHERE a.id = ?1 AND a.role = 'thumbnail' AND r.kind = 'cache'",
        [asset_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()?)
}
