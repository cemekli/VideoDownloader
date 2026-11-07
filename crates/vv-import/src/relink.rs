//! Relink a moved file (SPEC IMP-07, DATA-03; AC-35).
//!
//! Only the asset's location changes. The item, its notes, overrides and
//! relations stay as they are. When the asset has a content hash, the new
//! file must have the same one unless the user explicitly accepts a
//! different file.

use crate::fsutil;
use crate::import::sha256;
use crate::roots;
use crate::tools::MediaTools;
use std::path::Path;
use std::result::Result;
use std::sync::atomic::AtomicBool;
use vv_domain::error::ErrorCode;
use vv_persistence::library::repo;
use vv_persistence::library::*;
use vv_persistence::rusqlite::{Connection, TransactionBehavior};

#[derive(Debug)]
pub enum RelinkError {
    NotFound,
    Unreadable,
    /// The new file is not importable media (§19 code).
    Rejected(ErrorCode),
    /// The new file's content differs from the recorded hash.
    ContentDiffers,
    /// The new file already belongs to another item.
    AlreadyInLibrary {
        item_id: String,
    },
    Storage(LibraryError),
}

impl From<LibraryError> for RelinkError {
    fn from(e: LibraryError) -> Self {
        RelinkError::Storage(e)
    }
}

impl From<vv_persistence::rusqlite::Error> for RelinkError {
    fn from(e: vv_persistence::rusqlite::Error) -> Self {
        RelinkError::Storage(e.into())
    }
}

pub fn relink(
    c: &mut Connection,
    clock: &dyn Clock,
    tools: &MediaTools,
    asset_id: &str,
    new_file: &Path,
    accept_different_content: bool,
) -> Result<(), RelinkError> {
    let asset = repo::get_media_asset(c, asset_id)?.ok_or(RelinkError::NotFound)?;
    let path = fsutil::canonical(new_file).map_err(|_| RelinkError::Unreadable)?;
    if !path.is_file() || path.to_str().is_none() {
        return Err(RelinkError::Unreadable);
    }
    tools
        .probe(&path)
        .map_err(|e| RelinkError::Rejected(e.code()))?;
    if let (Some(want), false) = (&asset.content_hash, accept_different_content) {
        let got = sha256(&path, &AtomicBool::new(false)).map_err(|_| RelinkError::Unreadable)?;
        if got.as_deref() != Some(want.as_str()) {
            return Err(RelinkError::ContentDiffers);
        }
    }
    let size = std::fs::metadata(&path).ok().map(|m| m.len() as i64);
    let now = clock.now_ms();
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let dir = path.parent().ok_or(RelinkError::Unreadable)?;
    let root = roots::root_for(&tx, now, dir, &path)?;
    let storage_ref =
        fsutil::relative_ref(Path::new(&root.path), &path).ok_or(RelinkError::Unreadable)?;
    if let Some(other) = repo::asset_at(&tx, &root.id, &storage_ref)? {
        if other.id != asset.id {
            return Err(RelinkError::AlreadyInLibrary {
                item_id: other.library_item_id,
            });
        }
    }
    repo::relocate_asset(&tx, &asset.id, &root.id, &storage_ref, size)?;
    tx.commit()?;
    Ok(())
}
