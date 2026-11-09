//! Media root, free space, quota and usage (STATE.md F3.9; SPEC DATA-07,
//! §14.2, §14.3 step 1; AC-12, AC-33; ADR-0021).

use crate::context::Dirs;
use serde::Serialize;
use std::path::Path;
use std::result::Result;
use vv_domain::error::ErrorCode;
use vv_persistence::library::repo::{self, StorageRoot};
use vv_persistence::library::*;
use vv_persistence::rusqlite::{params, Connection, OptionalExtension};
use vv_persistence::settings;

/// `app_setting` key of the managed-media quota (bytes, or null for none).
pub const QUOTA_KEY: &str = "storage_quota_bytes";
/// Below this much free space the Download dialog warns before starting.
pub const LOW_SPACE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Head room on top of a known size (container overhead, `.part` files).
const SIZE_MARGIN_PERCENT: u64 = 5;

/// §19 code for a file-system error while writing.
pub fn io_code(e: &std::io::Error) -> ErrorCode {
    use std::io::ErrorKind as K;
    match e.kind() {
        K::StorageFull | K::QuotaExceeded => ErrorCode::DiskFull,
        K::PermissionDenied | K::ReadOnlyFilesystem => ErrorCode::PathNotWritable,
        K::NotFound => ErrorCode::StorageRootUnavailable,
        _ => match e.raw_os_error() {
            // ENOSPC / ERROR_DISK_FULL, ERROR_HANDLE_DISK_FULL
            Some(28) if cfg!(unix) => ErrorCode::DiskFull,
            Some(112 | 39) if cfg!(windows) => ErrorCode::DiskFull,
            _ => ErrorCode::PathNotWritable,
        },
    }
}

/// Size of everything under `dir` (0 when it does not exist). Links are not
/// followed.
pub fn dir_size(dir: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    rd.flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

/// Free bytes of the volume holding `path` (or its nearest existing parent).
pub fn free_bytes(path: &Path) -> Option<u64> {
    let mut p = path;
    loop {
        if p.exists() {
            return fs4::available_space(p).ok();
        }
        p = p.parent()?;
    }
}

pub fn quota(c: &Connection) -> Option<u64> {
    let v: Option<String> = c
        .query_row(
            "SELECT value FROM app_setting WHERE key = ?1",
            [QUOTA_KEY],
            |r| r.get(0),
        )
        .optional()
        .ok()
        .flatten();
    v.and_then(|v| serde_json::from_str::<Option<u64>>(&v).ok().flatten())
        .filter(|q| *q > 0)
}

pub fn set_quota(c: &Connection, bytes: Option<u64>, now: i64) -> Result<(), ErrorCode> {
    let json =
        serde_json::to_string(&bytes.filter(|b| *b > 0)).map_err(|_| ErrorCode::InternalError)?;
    c.execute(
        "INSERT INTO app_setting (key, value, updated_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![QUOTA_KEY, json, now],
    )
    .map_err(|_| ErrorCode::InternalError)?;
    Ok(())
}

/// Bytes of managed originals (what the quota counts).
pub fn managed_media_bytes(c: &Connection) -> u64 {
    c.query_row(
        "SELECT coalesce(sum(size_bytes), 0) FROM media_asset WHERE storage_kind = 'managed' AND role = 'original'",
        [],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n.max(0) as u64)
    .unwrap_or(0)
}

/// Bytes of derived files (thumbnails and later previews, waveforms).
pub fn derived_bytes(c: &Connection) -> u64 {
    c.query_row(
        "SELECT coalesce(sum(size_bytes), 0) FROM media_asset WHERE role <> 'original'",
        [],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n.max(0) as u64)
    .unwrap_or(0)
}

/// DATA-07: what the storage screen shows. No paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageUsage {
    pub managed_media_bytes: u64,
    pub derived_bytes: u64,
    /// Partial downloads waiting for retry or clean-up.
    pub staging_bytes: u64,
    pub quota_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
    pub low_space: bool,
    pub quota_exceeded: bool,
}

pub fn usage(c: &Connection, dirs: &Dirs, media_dir: &Path) -> StorageUsage {
    let managed = managed_media_bytes(c);
    let quota = quota(c);
    let free = free_bytes(media_dir);
    StorageUsage {
        managed_media_bytes: managed,
        derived_bytes: derived_bytes(c),
        staging_bytes: dir_size(&dirs.staging),
        quota_bytes: quota,
        free_bytes: free,
        low_space: free.is_some_and(|f| f < LOW_SPACE_BYTES),
        quota_exceeded: quota.is_some_and(|q| managed >= q),
    }
}

/// §14.3 step 1. `estimate` is the preset's size (exact or estimate);
/// unknown sizes only check that the quota is not already used up.
pub fn check_space(
    c: &Connection,
    dirs: &Dirs,
    media_dir: &Path,
    estimate: Option<u64>,
    merge: bool,
) -> Result<(), ErrorCode> {
    let need = estimate.map(|e| {
        let e = e.saturating_add(e / 100 * SIZE_MARGIN_PERCENT);
        if merge {
            e.saturating_mul(2)
        } else {
            e
        }
    });
    if let Some(q) = quota(c) {
        let used = managed_media_bytes(c);
        if used >= q || estimate.is_some_and(|e| used.saturating_add(e) > q) {
            return Err(ErrorCode::QuotaExceeded);
        }
    }
    if let Some(need) = need {
        for dir in [dirs.staging.as_path(), media_dir] {
            if free_bytes(dir).is_some_and(|f| f < need) {
                return Err(ErrorCode::DiskFull);
            }
        }
    }
    Ok(())
}

/// The root new downloads go to (DATA-08: `active_download_root_id`, else
/// the default folder). Created when missing; refused when not writable.
pub fn active_media_root(
    c: &mut Connection,
    clock: &dyn Clock,
    dirs: &Dirs,
) -> Result<StorageRoot, ErrorCode> {
    let s = settings::get(c).map_err(|_| ErrorCode::InternalError)?;
    if let Some(id) = s.active_download_root_id {
        if let Some(r) = repo::get_storage_root(c, &id).map_err(|_| ErrorCode::InternalError)? {
            let p = Path::new(&r.path);
            if !p.is_dir() {
                if r.status == StorageRootStatus::Available {
                    let _ = repo::set_storage_root_status(
                        c,
                        &r.id,
                        StorageRootStatus::Disconnected,
                        None,
                    );
                }
                return Err(ErrorCode::StorageRootUnavailable);
            }
            writable(p)?;
            return reachable(c, clock, r);
        }
    }
    std::fs::create_dir_all(&dirs.default_media).map_err(|e| io_code(&e))?;
    writable(&dirs.default_media)?;
    ensure_root(
        c,
        clock,
        &dirs.default_media,
        StorageRootKind::ManagedMedia,
        "media",
    )
}

/// The root was just reached: a status left from a check while its drive
/// was locked or unplugged (DATA-09) is set back to `available`, so its
/// files stop showing "Drive disconnected" without a restart.
fn reachable(
    c: &Connection,
    clock: &dyn Clock,
    mut r: StorageRoot,
) -> Result<StorageRoot, ErrorCode> {
    if r.status != StorageRootStatus::Available {
        let now = clock.now_ms();
        repo::set_storage_root_status(c, &r.id, StorageRootStatus::Available, Some(now))
            .map_err(|_| ErrorCode::InternalError)?;
        r.status = StorageRootStatus::Available;
        r.last_verified_at = Some(now);
    }
    Ok(r)
}

/// A probe file proves the folder takes writes now (§14.3 step 1).
pub fn writable(dir: &Path) -> Result<(), ErrorCode> {
    let probe = dir.join(format!(".videovault-write-test-{}", new_id()));
    std::fs::write(&probe, b"").map_err(|e| io_code(&e))?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

/// The root of `kind` at `dir`, created when there is none.
pub fn ensure_root(
    c: &Connection,
    clock: &dyn Clock,
    dir: &Path,
    kind: StorageRootKind,
    label: &str,
) -> Result<StorageRoot, ErrorCode> {
    let canonical = vv_import::fsutil::canonical(dir).map_err(|_| ErrorCode::PathNotWritable)?;
    let path = canonical
        .to_str()
        .ok_or(ErrorCode::PathNotWritable)?
        .to_owned();
    let roots = repo::list_storage_roots(c).map_err(|_| ErrorCode::InternalError)?;
    if let Some(r) = roots.into_iter().find(|r| r.kind == kind && r.path == path) {
        return reachable(c, clock, r);
    }
    let now = clock.now_ms();
    let r = StorageRoot {
        id: new_id(),
        kind,
        label: label.to_owned(),
        path,
        volume_id: None,
        status: StorageRootStatus::Available,
        created_at: now,
        last_verified_at: Some(now),
    };
    repo::insert_storage_root(c, &r).map_err(|_| ErrorCode::InternalError)?;
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vv_persistence::{Database, OpenOptions};

    #[test]
    fn quota_blocks_before_the_download_and_unknown_sizes_only_when_full() {
        let d = tempfile::tempdir().unwrap();
        let mut db = Database::open(&d.path().join("q.sqlite"), &OpenOptions::default()).unwrap();
        let c = db.writer();
        let dirs = Dirs::under(d.path());
        std::fs::create_dir_all(&dirs.staging).unwrap();
        assert_eq!(check_space(c, &dirs, d.path(), Some(10), false), Ok(()));
        set_quota(c, Some(100), 1).unwrap();
        assert_eq!(quota(c), Some(100));
        assert_eq!(
            check_space(c, &dirs, d.path(), Some(101), false),
            Err(ErrorCode::QuotaExceeded)
        );
        assert_eq!(check_space(c, &dirs, d.path(), None, false), Ok(()));
        // Far more than any disk: DISK_FULL before anything is written.
        assert_eq!(
            check_space(c, &dirs, d.path(), Some(u64::MAX / 4), false),
            Err(ErrorCode::QuotaExceeded)
        );
        set_quota(c, None, 2).unwrap();
        assert_eq!(
            check_space(c, &dirs, d.path(), Some(u64::MAX / 4), false),
            Err(ErrorCode::DiskFull)
        );
        let u = usage(c, &dirs, d.path());
        assert_eq!(u.quota_bytes, None);
        assert!(!u.quota_exceeded);
    }

    #[test]
    fn a_root_that_is_reached_again_is_available_again() {
        let d = tempfile::tempdir().unwrap();
        let mut db = Database::open(&d.path().join("r.sqlite"), &OpenOptions::default()).unwrap();
        let c = db.writer();
        let dirs = Dirs::under(d.path());
        let r = active_media_root(c, &SystemClock, &dirs).unwrap();
        // A start-up check ran while the drive was locked (BitLocker).
        repo::set_storage_root_status(c, &r.id, StorageRootStatus::Disconnected, None).unwrap();
        let again = active_media_root(c, &SystemClock, &dirs).unwrap();
        assert_eq!(again.id, r.id);
        assert_eq!(again.status, StorageRootStatus::Available);
        let stored = repo::get_storage_root(c, &r.id).unwrap().unwrap();
        assert_eq!(stored.status, StorageRootStatus::Available);
    }
}
