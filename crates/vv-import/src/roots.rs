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
