//! `storage_move` jobs: moving managed media to another root, one asset at
//! a time, without ever losing the only verified copy (STATE.md F3.9; SPEC
//! §13.1 `storage_move`, DATA-08; AC-43; ADR-0021).
//!
//! Each asset goes `pending → copied → verified → switched → cleaned`, and
//! the step is written to `storage_move_step` only after the file work it
//! names is done:
//!
//! 1. copy to `<target>/.videovault-moving/<asset-id>` (`copied`);
//! 2. size and SHA-256 equal the source's (`verified`; a source without a
//!    hash is hashed first);
//! 3. rename into its final place, then in one transaction point the asset
//!    at the new root (`switched`). Before this commit the database names
//!    the old copy, after it the new one;
//! 4. delete the old copy (`cleaned`; `cleanup_pending` when that fails,
//!    retried at the next start).
//!
//! A restart picks up where the rows say: `copied` is verified again,
//! `verified` finishes the switch (the renamed file is found and checked),
//! `switched` is cleaned. A cancel stops between assets: switched ones stay
//! in the new root, the rest in the old one, and both roots stay
//! registered. New downloads keep going to the old root until the move
//! succeeds; then the target becomes the active root.

use crate::context::{Context, Store};
use crate::download::place;
use crate::storage;
use crate::worker::{Stop, Work};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use vv_domain::error::ErrorCode;
use vv_persistence::jobs::{self, JobKind, Lease, NewJob, Progress};
use vv_persistence::library::repo;
use vv_persistence::rusqlite::{params, Connection, OptionalExtension};
use vv_persistence::settings;

pub const MOVING_DIR: &str = ".videovault-moving";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MovePayload {
    pub v: u32,
    pub source_root_id: String,
    pub target_root_id: String,
    /// Taken when the job was created (§13.1).
    pub asset_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Pending,
    Copied,
    Verified,
    Switched,
    Cleaned,
    CleanupPending,
    Skipped,
}

impl Step {
    pub fn as_str(self) -> &'static str {
        match self {
            Step::Pending => "pending",
            Step::Copied => "copied",
            Step::Verified => "verified",
            Step::Switched => "switched",
            Step::Cleaned => "cleaned",
            Step::CleanupPending => "cleanup_pending",
            Step::Skipped => "skipped",
        }
    }
    pub fn parse(s: &str) -> Option<Step> {
        [
            Step::Pending,
            Step::Copied,
            Step::Verified,
            Step::Switched,
            Step::Cleaned,
            Step::CleanupPending,
            Step::Skipped,
        ]
        .into_iter()
        .find(|x| x.as_str() == s)
    }
    fn done(self) -> bool {
        matches!(self, Step::Cleaned | Step::CleanupPending | Step::Skipped)
    }
}

/// Queues a move of every managed original in `source` to `target`.
pub fn enqueue(
    c: &mut Connection,
    source_root_id: &str,
    target_root_id: &str,
    now: i64,
) -> jobs::Result<jobs::Enqueued> {
    if source_root_id == target_root_id {
        return Err(jobs::JobError::Invalid("move to the same root"));
    }
    let asset_ids: Vec<String> = {
        let mut st = c.prepare(
            "SELECT id FROM media_asset WHERE storage_root_id = ?1 AND storage_kind = 'managed' AND role = 'original' ORDER BY created_at, id",
        )?;
        let rows = st.query_map([source_root_id], |r| r.get(0))?;
        rows.collect::<vv_persistence::rusqlite::Result<_>>()?
    };
    let mut job = NewJob::new(
        JobKind::StorageMove,
        format!("storage_move:{source_root_id}:{target_root_id}"),
    );
    job.payload = serde_json::to_value(MovePayload {
        v: 1,
        source_root_id: source_root_id.into(),
        target_root_id: target_root_id.into(),
        asset_ids,
    })
    .unwrap_or_default();
    jobs::enqueue(c, &job, now)
}

#[derive(Debug, Clone)]
struct Row {
    asset_id: String,
    step: Step,
    source_ref: String,
    target_ref: String,
    hash: Option<String>,
    size: Option<i64>,
}

fn load_rows(c: &Connection, job_id: &str) -> Vec<Row> {
    let Ok(mut st) = c.prepare(
        "SELECT asset_id, step, source_ref, target_ref, content_hash, size_bytes FROM storage_move_step WHERE job_id = ?1 ORDER BY rowid",
    ) else {
        return Vec::new();
    };
    st.query_map([job_id], |r| {
        let step: String = r.get(1)?;
        Ok(Row {
            asset_id: r.get(0)?,
            step: Step::parse(&step).unwrap_or(Step::Pending),
            source_ref: r.get(2)?,
            target_ref: r.get(3)?,
            hash: r.get(4)?,
            size: r.get(5)?,
        })
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}

fn set_step(
    c: &Connection,
    job_id: &str,
    row: &Row,
    now: i64,
) -> vv_persistence::rusqlite::Result<usize> {
    c.execute(
        "UPDATE storage_move_step SET step = ?3, content_hash = ?4, size_bytes = ?5, updated_at = ?6 WHERE job_id = ?1 AND asset_id = ?2",
        params![job_id, row.asset_id, row.step.as_str(), row.hash, row.size, now],
    )
}

/// Writes the step rows once, at the first attempt.
fn init_rows(
    c: &Connection,
    job_id: &str,
    p: &MovePayload,
    now: i64,
) -> vv_persistence::rusqlite::Result<()> {
    let have: i64 = c.query_row(
        "SELECT count(*) FROM storage_move_step WHERE job_id = ?1",
        [job_id],
        |r| r.get(0),
    )?;
    if have > 0 {
        return Ok(());
    }
    for id in &p.asset_ids {
        let a: Option<(String, Option<String>, Option<i64>)> = c
            .query_row(
                "SELECT storage_ref, content_hash, size_bytes FROM media_asset WHERE id = ?1 AND storage_root_id = ?2",
                params![id, p.source_root_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let (step, sref, hash, size) = match a {
            Some((r, h, s)) => (Step::Pending, r, h, s),
            None => (Step::Skipped, String::new(), None, None),
        };
        c.execute(
            "INSERT INTO storage_move_step (job_id, asset_id, step, source_root_id, source_ref, target_ref, size_bytes, content_hash, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?7, ?8)",
            params![job_id, id, step.as_str(), p.source_root_id, sref, size, hash, now],
        )?;
    }
    Ok(())
}

fn sha(p: &Path) -> Option<String> {
    vv_import::import::sha256(p, &AtomicBool::new(false))
        .ok()
        .flatten()
}

fn file_matches(p: &Path, size: Option<i64>, hash: Option<&str>) -> bool {
    let Ok(md) = std::fs::metadata(p) else {
        return false;
    };
    if size.is_some_and(|s| s != md.len() as i64) {
        return false;
    }
    match hash {
        Some(h) => sha(p).as_deref() == Some(h),
        None => false,
    }
}

pub fn run<S: Store>(cx: &Context<S>, lease: Lease) {
    let w = Work::new(cx, lease);
    let Ok(p) = serde_json::from_value::<MovePayload>(w.lease.job.payload.clone()) else {
        w.end_error(ErrorCode::InputRejected, None);
        return;
    };
    let roots = cx.store.read(|c| {
        (
            repo::get_storage_root(c, &p.source_root_id).ok().flatten(),
            repo::get_storage_root(c, &p.target_root_id).ok().flatten(),
        )
    });
    let (Some(src), Some(dst)) = roots else {
        w.end_error(ErrorCode::StorageRootUnavailable, None);
        return;
    };
    let (src_dir, dst_dir) = (PathBuf::from(&src.path), PathBuf::from(&dst.path));
    if !src_dir.is_dir() || !dst_dir.is_dir() {
        w.end_error(ErrorCode::StorageRootUnavailable, None);
        return;
    }
    if let Err(code) = storage::writable(&dst_dir) {
        w.end_error(code, None);
        return;
    }
    let now = cx.now();
    if cx.store.write(|c| init_rows(c, w.id(), &p, now)).is_err() {
        w.end_error(ErrorCode::InternalError, None);
        return;
    }
    let rows = cx.store.read(|c| load_rows(c, w.id()));
    let total = rows.len() as i64;
    let moving = dst_dir.join(MOVING_DIR);
    let mut done = rows.iter().filter(|r| r.step.done()).count() as i64;
    let report = |done: i64, force: bool| {
        w.progress(
            &Progress {
                phase: Some("move".into()),
                bytes_done: None,
                bytes_total: None,
                units_done: Some(done),
                units_total: Some(total),
            },
            force,
        );
    };
    report(done, true);
    for mut row in rows {
        if row.step.done() {
            continue;
        }
        if w.should_stop() {
            w.end_stopped(w.stopped().unwrap_or(Stop::Cancel));
            return;
        }
        if let Err(code) = move_one(cx, &w, &mut row, &src_dir, &dst_dir, &moving, &dst.id) {
            w.end_error(code, None);
            return;
        }
        done += 1;
        report(done, false);
    }
    let _ = std::fs::remove_dir(&moving);
    let (id, g, now) = (w.id().to_owned(), w.generation(), cx.now());
    let target = p.target_root_id.clone();
    let _ = cx.store.write(|c| {
        jobs::succeed_with(c, &id, g, now, |tx| {
            settings::set_download_root(tx, &target, now)
                .map_err(|_| jobs::JobError::Invalid("settings"))?;
            Ok(())
        })
    });
    cx.events.job_changed(&id, None);
}

fn move_one<S: Store>(
    cx: &Context<S>,
    w: &Work<'_, S>,
    row: &mut Row,
    src_dir: &Path,
    dst_dir: &Path,
    moving: &Path,
    dst_root_id: &str,
) -> Result<(), ErrorCode> {
    let src = vv_import::fsutil::resolve_ref(src_dir, &row.source_ref);
    let staged = moving.join(&row.asset_id);
    let fin = vv_import::fsutil::resolve_ref(dst_dir, &row.target_ref);
    let save = |row: &Row| {
        let now = cx.now();
        cx.store
            .write(|c| set_step(c, w.id(), row, now))
            .map_err(|_| ErrorCode::InternalError)
    };
    // The asset may have been deleted while the job waited.
    let exists = cx
        .store
        .read(|c| repo::get_media_asset(c, &row.asset_id).ok().flatten())
        .is_some();
    if !exists {
        row.step = Step::Skipped;
        return save(row).map(|_| ());
    }
    loop {
        match row.step {
            Step::Pending => {
                if row.hash.is_none() {
                    row.hash = Some(sha(&src).ok_or(ErrorCode::FileMissing)?);
                }
                if row.size.is_none() {
                    row.size = std::fs::metadata(&src).ok().map(|m| m.len() as i64);
                }
                std::fs::create_dir_all(moving).map_err(|e| storage::io_code(&e))?;
                std::fs::copy(&src, &staged).map_err(|e| {
                    let _ = std::fs::remove_file(&staged);
                    if src.exists() {
                        storage::io_code(&e)
                    } else {
                        ErrorCode::FileMissing
                    }
                })?;
                row.step = Step::Copied;
                save(row)?;
            }
            Step::Copied => {
                if file_matches(&staged, row.size, row.hash.as_deref()) {
                    row.step = Step::Verified;
                } else {
                    let _ = std::fs::remove_file(&staged);
                    row.step = Step::Pending;
                }
                save(row)?;
            }
            Step::Verified => {
                if !fin.is_file() {
                    if !staged.is_file() {
                        row.step = Step::Pending;
                        save(row)?;
                        continue;
                    }
                    place(&staged, &fin, row.hash.as_deref())?;
                }
                if !file_matches(&fin, row.size, row.hash.as_deref()) {
                    let _ = std::fs::remove_file(&fin);
                    row.step = Step::Pending;
                    save(row)?;
                    continue;
                }
                let now = cx.now();
                let r = cx.store.write(|c| {
                    let tx = c.transaction()?;
                    repo::relocate_asset(
                        &tx,
                        &row.asset_id,
                        dst_root_id,
                        &row.target_ref,
                        row.size,
                    )
                    .map_err(|_| vv_persistence::rusqlite::Error::InvalidQuery)?;
                    set_step(
                        &tx,
                        w.id(),
                        &Row {
                            step: Step::Switched,
                            ..row.clone()
                        },
                        now,
                    )?;
                    tx.commit()
                });
                r.map_err(|_| ErrorCode::InternalError)?;
                row.step = Step::Switched;
            }
            Step::Switched => {
                row.step = match std::fs::remove_file(&src) {
                    Ok(()) => Step::Cleaned,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Step::Cleaned,
                    Err(_) => Step::CleanupPending,
                };
                if let Some(d) = src.parent() {
                    let _ = std::fs::remove_dir(d); // only when empty
                }
                save(row)?;
                return Ok(());
            }
            Step::Cleaned | Step::CleanupPending | Step::Skipped => return Ok(()),
        }
    }
}

/// Start-up: old copies whose delete failed are tried again (§13.1).
pub fn retry_cleanups(c: &Connection, now: i64) -> usize {
    let Ok(mut st) = c.prepare(
        "SELECT s.job_id, s.asset_id, r.path, s.source_ref FROM storage_move_step s JOIN storage_root r ON r.id = s.source_root_id WHERE s.step = 'cleanup_pending'",
    ) else {
        return 0;
    };
    let rows: Vec<(String, String, String, String)> = st
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .map(|r| r.flatten().collect())
        .unwrap_or_default();
    let mut n = 0;
    for (job, asset, root, sref) in rows {
        let f = vv_import::fsutil::resolve_ref(Path::new(&root), &sref);
        let gone = match std::fs::remove_file(&f) {
            Ok(()) => true,
            Err(e) => e.kind() == std::io::ErrorKind::NotFound,
        };
        if gone {
            let _ = c.execute(
                "UPDATE storage_move_step SET step = 'cleaned', updated_at = ?3 WHERE job_id = ?1 AND asset_id = ?2",
                params![job, asset, now],
            );
            n += 1;
        }
    }
    n
}
