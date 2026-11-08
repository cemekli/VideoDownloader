//! Permanent deletion, the retention clean-up and orphaned media (SPEC §7.3
//! "Kaldırma semantiği", "Sahipsiz medya"; STATE.md F2.2; ADR-0015).
//!
//! The database side only. Files are deleted by the caller after the
//! transaction commits, from the list these functions return, so a failed
//! commit never leaves a row pointing at a deleted file. Rules:
//!
//! - `external_reference` files are never touched.
//! - Managed original media is kept by default and becomes `OrphanMedia`;
//!   it is deleted only when the user asks for it explicitly.
//! - Derived files (thumbnails and the like) go with their item.

use super::repo::{self, LibraryItem};
use super::{new_id, AssetRole, LibraryError, Result, StorageKind};
use rusqlite::{params, Connection, OptionalExtension};

/// Default retention of Recently removed (§7.3: 30 days, adjustable).
pub const RETENTION_DAYS: i64 = 30;
pub const DAY_MS: i64 = 86_400_000;

/// A file to delete after commit: `(storage_root_id, storage_ref)`.
pub type FileRef = (String, String);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeleteReport {
    pub items: usize,
    /// Managed originals kept as orphaned media.
    pub orphaned: usize,
    pub files_to_delete: Vec<FileRef>,
}

impl DeleteReport {
    fn add(&mut self, o: DeleteReport) {
        self.items += o.items;
        self.orphaned += o.orphaned;
        self.files_to_delete.extend(o.files_to_delete);
    }
}

/// Deletes one item for good. Only items already in Recently removed can
/// be deleted (the UI flow goes through removal first).
pub fn delete_permanently(
    c: &Connection,
    id: &str,
    delete_media: bool,
    now_ms: i64,
) -> Result<DeleteReport> {
    let it: LibraryItem = repo::get_item(c, id)?.ok_or(LibraryError::NotFound)?;
    if it.removed_at.is_none() {
        return Err(LibraryError::Invalid("item is not in Recently removed"));
    }
    let mut r = DeleteReport {
        items: 1,
        ..Default::default()
    };
    for a in repo::assets_for_item(c, id)? {
        match (a.role, a.storage_kind) {
            (_, StorageKind::ExternalReference) => {}
            (AssetRole::Original, StorageKind::Managed) if delete_media => {
                r.files_to_delete.push((a.storage_root_id, a.storage_ref));
            }
            (AssetRole::Original, StorageKind::Managed) => {
                c.execute(
                    "INSERT OR IGNORE INTO orphan_media (id, storage_root_id, storage_ref, size_bytes, content_hash, former_item_title, orphaned_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![new_id(), a.storage_root_id, a.storage_ref, a.size_bytes, a.content_hash, it.display_title(), now_ms],
                )?;
                r.orphaned += 1;
            }
            (_, StorageKind::Managed) => r.files_to_delete.push((a.storage_root_id, a.storage_ref)),
        }
    }
    // Assets, relations and the search document go with the item.
    let seq: i64 = c.query_row("SELECT seq FROM library_item WHERE id = ?1", [id], |r| {
        r.get(0)
    })?;
    c.execute("DELETE FROM search_document WHERE rowid = ?1", [seq])?;
    c.execute(
        "UPDATE library_item SET thumbnail_asset_id = NULL WHERE id = ?1",
        [id],
    )?;
    c.execute("DELETE FROM url_alias WHERE item_id = ?1", [id])?;
    c.execute(
        "DELETE FROM duplicate_candidate WHERE item_a_id = ?1 OR item_b_id = ?1",
        [id],
    )?;
    c.execute("DELETE FROM library_item WHERE id = ?1", [id])?;
    Ok(r)
}

pub fn delete_many(
    c: &Connection,
    ids: &[String],
    delete_media: bool,
    now_ms: i64,
) -> Result<DeleteReport> {
    let mut r = DeleteReport::default();
    for id in ids {
        r.add(delete_permanently(c, id, delete_media, now_ms)?);
    }
    Ok(r)
}

/// Retention clean-up: items removed more than `retention_days` ago are
/// deleted for good; their managed media is always kept as orphaned media
/// (§7.3: automatic clean-up never deletes managed originals).
pub fn purge_expired(c: &Connection, now_ms: i64, retention_days: i64) -> Result<DeleteReport> {
    let cutoff = now_ms - retention_days * DAY_MS;
    let ids: Vec<String> = {
        let mut s = c.prepare(
            "SELECT id FROM library_item WHERE removed_at IS NOT NULL AND removed_at < ?1",
        )?;
        let v = s
            .query_map([cutoff], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        v
    };
    delete_many(c, &ids, false, now_ms)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Orphan {
    pub id: String,
    pub storage_root_id: String,
    pub storage_ref: String,
    pub root_label: String,
    pub root_status: String,
    pub size_bytes: Option<i64>,
    pub former_item_title: Option<String>,
    pub orphaned_at: i64,
}

fn orphan_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Orphan> {
    Ok(Orphan {
        id: r.get(0)?,
        storage_root_id: r.get(1)?,
        storage_ref: r.get(2)?,
        root_label: r.get(3)?,
        root_status: r.get(4)?,
        size_bytes: r.get(5)?,
        former_item_title: r.get(6)?,
        orphaned_at: r.get(7)?,
    })
}

const ORPHAN_SELECT: &str = "SELECT o.id, o.storage_root_id, o.storage_ref, r.label, r.status, o.size_bytes, \
     o.former_item_title, o.orphaned_at FROM orphan_media o JOIN storage_root r ON r.id = o.storage_root_id";

pub fn list_orphans(c: &Connection) -> Result<Vec<Orphan>> {
    let mut s = c.prepare(&format!(
        "{ORPHAN_SELECT} ORDER BY o.orphaned_at DESC, o.id"
    ))?;
    let v = s
        .query_map([], orphan_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(v)
}

pub fn get_orphan(c: &Connection, id: &str) -> Result<Option<Orphan>> {
    Ok(c.query_row(
        &format!("{ORPHAN_SELECT} WHERE o.id = ?1"),
        [id],
        orphan_row,
    )
    .optional()?)
}

/// "Re-import": a new `local_video` item that refers to the file where it
/// is, as managed media; nothing is moved (AC-53). Returns the new item.
/// The file's facts are filled in by the next storage check / probe.
pub fn reimport_orphan(c: &Connection, id: &str, now_ms: i64) -> Result<LibraryItem> {
    let o = get_orphan(c, id)?.ok_or(LibraryError::NotFound)?;
    let mut it = LibraryItem::new(
        super::ItemKind::LocalVideo,
        super::SourcePlatform::Local,
        now_ms,
    );
    let file_name = o
        .storage_ref
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&o.storage_ref)
        .to_owned();
    it.title = Some(o.former_item_title.clone().unwrap_or(file_name));
    it.metadata_status = super::MetadataStatus::Ready;
    repo::insert_item(c, &it)?;
    let mut a = repo::MediaAsset::new(
        &it.id,
        AssetRole::Original,
        super::MediaKind::Video,
        StorageKind::Managed,
        &o.storage_root_id,
        &o.storage_ref,
        now_ms,
    );
    a.size_bytes = o.size_bytes;
    a.acquisition_status = super::AcquisitionStatus::Ready;
    repo::insert_media_asset(c, &a)?;
    c.execute("DELETE FROM orphan_media WHERE id = ?1", [id])?;
    Ok(it)
}

/// Forgets an orphan and returns its file for deletion (separate consent in
/// the UI). A file on a disconnected root is not deleted: refused.
pub fn delete_orphan(c: &Connection, id: &str) -> Result<FileRef> {
    let o = get_orphan(c, id)?.ok_or(LibraryError::NotFound)?;
    if o.root_status == "disconnected" {
        return Err(LibraryError::Invalid("root-disconnected"));
    }
    c.execute("DELETE FROM orphan_media WHERE id = ?1", [id])?;
    Ok((o.storage_root_id, o.storage_ref))
}
