//! Storage roots and asset presence (SPEC DATA-09, IMP-07; STATE.md F1.5).
//!
//! A root that cannot be reached (unplugged drive, unmounted share) is
//! `disconnected` (or `inaccessible` when the OS refuses access) and its
//! assets keep their state: nothing becomes `missing` one by one. Only when
//! the root is `available` is each asset checked, and a file that is gone
//! becomes `missing` (AC-35, AC-47). The next check after the drive returns
//! verifies the root and its assets again on its own.

use crate::fsutil;
use std::path::Path;
use std::result::Result;
use vv_persistence::library::repo::{self, StorageRoot};
use vv_persistence::library::*;
use vv_persistence::rusqlite::Connection;

/// Reachability of a root directory right now.
pub fn root_status(path: &Path) -> StorageRootStatus {
    match std::fs::metadata(path) {
        Ok(m) if m.is_dir() => match std::fs::read_dir(path) {
            Ok(_) => StorageRootStatus::Available,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                StorageRootStatus::Inaccessible
            }
            Err(_) => StorageRootStatus::Disconnected,
        },
        Ok(_) => StorageRootStatus::Inaccessible,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            StorageRootStatus::Inaccessible
        }
        Err(_) => StorageRootStatus::Disconnected,
    }
}

/// The `external_import` root for a file: the most specific existing root
/// that contains it, else a new root for `dir` (§11.6: an existing folder or
/// one that covers it is reused). `dir` and `file` are canonical.
pub fn root_for(
    c: &Connection,
    now_ms: i64,
    dir: &Path,
    file: &Path,
) -> Result<StorageRoot, LibraryError> {
    let best = repo::list_storage_roots(c)?
        .into_iter()
        .filter(|r| {
            r.kind == StorageRootKind::ExternalImport
                && file.starts_with(Path::new(&r.path))
                && file != Path::new(&r.path)
        })
        .max_by_key(|r| Path::new(&r.path).components().count());
    if let Some(r) = best {
        return Ok(r);
    }
    let path = dir
        .to_str()
        .ok_or(LibraryError::Invalid("non-Unicode root path"))?;
    let label = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_owned();
    let r = StorageRoot {
        id: new_id(),
        kind: StorageRootKind::ExternalImport,
        label,
        path: path.to_owned(),
        volume_id: None,
        status: StorageRootStatus::Available,
        created_at: now_ms,
        last_verified_at: Some(now_ms),
    };
    repo::insert_storage_root(c, &r)?;
    Ok(r)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootCheck {
    pub root_id: String,
    pub before: StorageRootStatus,
    pub after: StorageRootStatus,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StorageCheck {
    pub roots: Vec<RootCheck>,
    /// Assets found where expected (incl. restored).
    pub verified: usize,
    /// Assets that were `missing` and are back.
    pub restored: usize,
    /// Newly or still `missing` on an available root.
    pub missing: usize,
    /// Present but a different size than recorded.
    pub modified: usize,
    /// Assets not checked because their root is not available.
    pub unavailable: usize,
}

/// Checks every root, then the assets of the roots that are available.
/// Run at start-up and periodically; it is also what notices a drive that
/// came back (DATA-09).
pub fn check_storage(c: &mut Connection, clock: &dyn Clock) -> Result<StorageCheck, LibraryError> {
    let now = clock.now_ms();
    let tx = c.transaction()?;
    let mut report = StorageCheck::default();
    for root in repo::list_storage_roots(&tx)? {
        let after = root_status(Path::new(&root.path));
        let verified_at = (after == StorageRootStatus::Available).then_some(now);
        repo::set_storage_root_status(&tx, &root.id, after, verified_at)?;
        report.roots.push(RootCheck {
            root_id: root.id.clone(),
            before: root.status,
            after,
        });
        let assets = repo::assets_in_root(&tx, &root.id)?;
        if after != StorageRootStatus::Available {
            report.unavailable += assets.len();
            continue;
        }
        for a in assets {
            let path = fsutil::resolve_ref(Path::new(&root.path), &a.storage_ref);
            let state = match std::fs::metadata(&path) {
                Ok(m) if m.is_file() => match a.size_bytes {
                    Some(s) if s != m.len() as i64 => IntegrityState::Modified,
                    _ => IntegrityState::Verified,
                },
                _ => IntegrityState::Missing,
            };
            match state {
                IntegrityState::Verified => {
                    report.verified += 1;
                    if a.integrity_state == IntegrityState::Missing {
                        report.restored += 1;
                    }
                }
                IntegrityState::Missing => report.missing += 1,
                _ => report.modified += 1,
            }
            if state != a.integrity_state {
                repo::set_asset_integrity(&tx, &a.id, state)?;
            }
        }
    }
    tx.commit()?;
    Ok(report)
}

/// How many of a root's files "Locate drive" looks for in the new folder.
pub const LOCATE_SAMPLE: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Located {
    /// Sampled files found in the new folder with their recorded size.
    pub found: usize,
    pub checked: usize,
}

#[derive(Debug)]
pub enum LocateError {
    NotFound,
    /// Too few of the root's files are in that folder: it is not the same
    /// folder (the root is left as it was).
    NotThere(Located),
    Unreadable,
    Db(LibraryError),
}

impl From<LibraryError> for LocateError {
    fn from(e: LibraryError) -> Self {
        LocateError::Db(e)
    }
}

impl From<vv_persistence::rusqlite::Error> for LocateError {
    fn from(e: vv_persistence::rusqlite::Error) -> Self {
        LocateError::Db(e.into())
    }
}

/// "Locate drive…" (DATA-09): the drive of a root came back under another
/// letter or mount point, so its files are in `dir` now. Files are stored
/// relative to their root, so pointing the root at `dir` brings them all
/// back; nothing is copied or moved. `dir` must hold at least half of a
/// sample of the root's files with their recorded sizes. When another root
/// of the same kind already uses `dir` (the folder was picked again in
/// Settings), the files are moved over to that root and this one is
/// removed. Assets are checked again afterwards.
pub fn locate(
    c: &mut Connection,
    clock: &dyn Clock,
    root_id: &str,
    dir: &Path,
) -> Result<Located, LocateError> {
    let dir = fsutil::canonical(dir).map_err(|_| LocateError::Unreadable)?;
    if root_status(&dir) != StorageRootStatus::Available {
        return Err(LocateError::Unreadable);
    }
    let path = dir.to_str().ok_or(LocateError::Unreadable)?.to_owned();
    let root = repo::get_storage_root(c, root_id)?.ok_or(LocateError::NotFound)?;
    let mut sample: Vec<(String, Option<i64>)> = repo::assets_in_root(c, root_id)?
        .into_iter()
        .map(|a| (a.storage_ref, a.size_bytes))
        .collect();
    sample.sort();
    sample.dedup();
    let step = sample.len().div_ceil(LOCATE_SAMPLE).max(1);
    let sample: Vec<_> = sample.into_iter().step_by(step).collect();
    let found = sample
        .iter()
        .filter(
            |(r, size)| match std::fs::metadata(fsutil::resolve_ref(&dir, r)) {
                Ok(m) if m.is_file() => size.is_none_or(|s| s == m.len() as i64),
                _ => false,
            },
        )
        .count();
    let res = Located {
        found,
        checked: sample.len(),
    };
    if !sample.is_empty() && (found == 0 || found * 2 < sample.len()) {
        return Err(LocateError::NotThere(res));
    }
    let now = clock.now_ms();
    let tx = c.transaction()?;
    let other = repo::list_storage_roots(&tx)?
        .into_iter()
        .find(|r| r.id != root.id && r.kind == root.kind && r.path == path);
    if let Some(o) = other {
        tx.execute(
            "UPDATE media_asset SET storage_root_id = ?2 WHERE storage_root_id = ?1",
            [&root.id, &o.id],
        )?;
        tx.execute(
            "UPDATE OR IGNORE orphan_media SET storage_root_id = ?2 WHERE storage_root_id = ?1",
            [&root.id, &o.id],
        )?;
        tx.execute(
            "DELETE FROM orphan_media WHERE storage_root_id = ?1",
            [&root.id],
        )?;
        if vv_persistence::settings::get(&tx)
            .ok()
            .and_then(|s| s.active_download_root_id)
            .as_deref()
            == Some(root.id.as_str())
        {
            vv_persistence::settings::set_download_root(&tx, &o.id, now)
                .map_err(|_| LocateError::Unreadable)?;
        }
        tx.execute("DELETE FROM storage_root WHERE id = ?1", [&root.id])?;
    } else {
        tx.execute(
            "UPDATE storage_root SET path = ?2, status = 'available', last_verified_at = ?3 WHERE id = ?1",
            vv_persistence::rusqlite::params![root.id, path, now],
        )?;
    }
    tx.commit()?;
    check_storage(c, clock)?;
    Ok(res)
}
