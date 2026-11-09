//! `download` jobs: guarded yt-dlp download, merge, verification and the
//! move from staging to the media root (STATE.md F3.6; SPEC DL-03…DL-08,
//! §13.2–§13.4, §14.2, §14.3; AC-08, AC-09, AC-10, AC-12; ADR-0021).
//!
//! §14.3 in order:
//!
//! 1. Target root reachable and writable, enough free space for the known
//!    size (twice that when parts are merged), quota not exceeded
//!    (`DISK_FULL`, `QUOTA_EXCEEDED`, `PATH_NOT_WRITABLE` block the job;
//!    the bookmark is untouched and nothing is deleted).
//! 2. yt-dlp writes into `staging/<job-id>/` only. That folder is kept
//!    between attempts, so a retry or a resume continues the `.part` file.
//! 3. Merging (yt-dlp's FFmpeg merger, local files only) reports the
//!    `merge` phase; nothing is shown as done before verification.
//! 4. The file yt-dlp reported must be inside the staging folder; it is
//!    probed with the restricted ffprobe (a container with streams), sized
//!    and hashed (SHA-256).
//! 5. It is renamed to `<root>/media/<item-id>/<asset-id>.<ext>` (copy,
//!    verify, delete when the volume differs). The final reference is saved
//!    as the job's checkpoint first, so a crash between the move and the
//!    commit is finished on the next attempt instead of downloading again.
//! 6. The `MediaAsset` is published with the job's success in one
//!    transaction, only by the current generation (§13.2).

use crate::context::{Context, Store};
use crate::formats::{self, PresetId};
use crate::storage;
use crate::worker::{Stop, Work};
use crate::ytdlp::{self, Mode, RunResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::result::Result;
use std::sync::atomic::AtomicBool;
use vv_domain::error::ErrorCode;
use vv_persistence::auth::Operation;
use vv_persistence::jobs::{self, JobKind, Lease, NewJob, Progress};
use vv_persistence::library::repo::{self, MediaAsset};
use vv_persistence::library::*;
use vv_persistence::rusqlite::{params, Connection};
use vv_ytdlp::progress::{DownloadStatus, PostprocessStatus, ProgressEvent};

pub const DOWNLOAD_CONFIG_VERSION: u32 = 1;
/// Checkpoint prefix: the final file was placed but not yet published.
const PLACED: &str = "placed:";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadPayload {
    pub v: u32,
    pub preset: PresetId,
}

/// Queues a download of `item_id` with `preset`. One open download per item
/// and preset; a different preset is a different job.
pub fn enqueue(
    c: &mut Connection,
    item_id: &str,
    preset: PresetId,
    now: i64,
) -> jobs::Result<jobs::Enqueued> {
    let mut job = NewJob::new(
        JobKind::Download,
        format!(
            "download:{item_id}:{}:v{DOWNLOAD_CONFIG_VERSION}",
            preset.as_str()
        ),
    );
    job.item_id = Some(item_id.to_owned());
    job.payload = serde_json::to_value(DownloadPayload { v: 1, preset })
        .unwrap_or_else(|_| serde_json::json!({}));
    jobs::enqueue(c, &job, now)
}

/// The staging folder of a job. Ours alone: never shown, never imported.
pub fn staging_dir(staging: &Path, job_id: &str) -> PathBuf {
    staging.join(job_id)
}

/// Bytes left behind in a job's staging folder (shown in Downloads as
/// "partial file"; removed only by the user, §13.3).
pub fn partial_bytes(staging: &Path, job_id: &str) -> u64 {
    storage::dir_size(&staging_dir(staging, job_id))
}

/// Maps yt-dlp progress to §13.4 progress. Separate video and audio come as
/// two download runs: they are `download` part 1 and 2, never one invented
/// percentage.
struct Tracker {
    merge: bool,
    part: i64,
    finished_parts: i64,
}

impl Tracker {
    fn map(&mut self, ev: &ProgressEvent) -> (Progress, bool) {
        match ev {
            ProgressEvent::Download(d) => {
                let mut force = false;
                if d.status == DownloadStatus::Finished {
                    self.finished_parts += 1;
                    force = true;
                } else if self.part <= self.finished_parts {
                    self.part = self.finished_parts + 1;
                    force = true;
                }
                let phase = if self.merge {
                    if self.part <= 1 {
                        "download_video"
                    } else {
                        "download_audio"
                    }
                } else {
                    "download"
                };
                (
                    Progress {
                        phase: Some(phase.into()),
                        bytes_done: Some(d.bytes_done as i64),
                        bytes_total: d.bytes_total.map(|b| b as i64),
                        bytes_estimate: d
                            .bytes_total
                            .is_none()
                            .then_some(d.bytes_total_estimate)
                            .flatten()
                            .map(|b| b as i64),
                        units_done: d.units_done.map(|u| u as i64),
                        units_total: d.units_total.map(|u| u as i64),
                    },
                    force,
                )
            }
            ProgressEvent::Postprocess {
                postprocessor,
                status,
            } => {
                let phase = if postprocessor.starts_with("Merger") {
                    "merge"
                } else {
                    "postprocess"
                };
                (
                    Progress {
                        phase: Some(phase.into()),
                        ..Default::default()
                    },
                    *status != PostprocessStatus::Processing,
                )
            }
        }
    }
}

fn phase(w: &Work<'_, impl Store>, name: &str) {
    w.progress(
        &Progress {
            phase: Some(name.into()),
            ..Default::default()
        },
        true,
    );
}

pub fn run<S: Store>(cx: &Context<S>, lease: Lease) {
    let w = Work::new(cx, lease);
    let Some(item_id) = w.item_id().map(str::to_owned) else {
        w.end_error(ErrorCode::InternalError, Some("download job without item"));
        return;
    };
    let payload: Option<DownloadPayload> = serde_json::from_value(w.lease.job.payload.clone()).ok();
    let Some(payload) = payload else {
        w.end_error(ErrorCode::InputRejected, None);
        return;
    };
    let Some(item) = cx
        .store
        .read(|c| repo::get_item(c, &item_id).ok().flatten())
    else {
        return;
    };
    let tools = cx.tools();
    let (Some(yt), Some(media)) = (tools.ytdlp.clone(), tools.media.clone()) else {
        let code = tools
            .report
            .ready_for_downloads()
            .err()
            .unwrap_or(ErrorCode::ToolMissing);
        w.end_error(code, None);
        return;
    };
    // A file placed by an attempt that crashed before its commit (§13.2).
    if let Some(cp) = w.lease.job.checkpoint_ref.clone() {
        if let Some(rest) = cp.strip_prefix(PLACED) {
            if let Some((root_id, storage_ref)) = rest.split_once('/') {
                if let Some(done) = adopt_placed(&w, &media, &item.id, root_id, storage_ref) {
                    if done {
                        return;
                    }
                }
            }
        }
    }
    let Some(url) = item.canonical_url.clone() else {
        w.end_error(ErrorCode::InvalidUrl, None);
        return;
    };
    if let Err(code) = ytdlp::precheck(&cx.policy, &url) {
        w.end_error(code, None);
        return;
    }
    if cx.store.read(|c| crate::metadata::is_live(c, &item.id)) {
        w.end_error(ErrorCode::InputRejected, None);
        return;
    }
    let stored = cx
        .store
        .read(|c| crate::metadata::stored_formats(c, &item.id));
    let (preset, plan) = formats::preset(stored.as_deref().unwrap_or(&[]), payload.preset);
    let Some(plan) = plan else {
        w.end_error(ErrorCode::InputRejected, None);
        return;
    };

    phase(&w, "prepare");
    let root = match cx
        .store
        .write(|c| storage::active_media_root(c, cx.clock.as_ref(), &cx.dirs))
    {
        Ok(r) => r,
        Err(code) => {
            w.end_error(code, None);
            return;
        }
    };
    let estimate = preset.size_bytes.or(preset.size_estimate_bytes);
    let check = cx.store.read(|c| {
        storage::check_space(
            c,
            &cx.dirs,
            Path::new(&root.path),
            estimate,
            preset.needs_merge,
        )
    });
    if let Err(code) = check {
        w.end_error(code, None);
        return;
    }

    let dir = staging_dir(&cx.dirs.staging, w.id());
    if std::fs::create_dir_all(&dir).is_err() {
        w.end_error(ErrorCode::PathNotWritable, None);
        return;
    }
    let mut tracker = Tracker {
        merge: preset.needs_merge,
        part: 0,
        finished_parts: 0,
    };
    let exec = |attempt: &crate::auth::Attempt, tracker: &mut Tracker| {
        let cookies = attempt.cookies();
        ytdlp::run_as(
            &yt,
            cx.run_policy(),
            &url,
            &Mode::Download {
                dir: dir.clone(),
                selector: plan.selector.clone(),
                merge_format: plan.merge_format.clone(),
            },
            cookies.as_deref(),
            None,
            |ev| {
                let (p, force) = tracker.map(ev);
                w.progress(&p, force);
            },
            || w.should_stop(),
        )
    };
    let op = Operation::Download;
    let mut attempt = crate::auth::first(cx, &item, op);
    let mut run = exec(&attempt, &mut tracker);
    if attempt.profile.is_none()
        && matches!(
            run.result,
            RunResult::Failed {
                code: ErrorCode::LoginRequired,
                ..
            }
        )
    {
        if let Some(a) = crate::auth::after_login_required(cx, &item, op) {
            attempt = a;
            run = exec(&attempt, &mut tracker);
        }
    }
    match run.result {
        RunResult::Succeeded => {}
        RunResult::Canceled => {
            w.end_stopped(w.stopped().unwrap_or(Stop::Cancel));
            return;
        }
        RunResult::Failed { code, message } => {
            let now = cx.now();
            cx.store
                .write(|c| crate::auth::after_failure(c, &item.id, &attempt, code, now));
            w.end_error(code, message.as_deref());
            return;
        }
    }
    let Some(file) = run.files.last().cloned() else {
        w.end_error(ErrorCode::ExtractorFailed, Some("no output file reported"));
        return;
    };
    if !ytdlp::inside(&dir, &file) {
        w.end_error(ErrorCode::InternalError, Some("output outside staging"));
        return;
    }
    if w.should_stop() {
        w.end_stopped(w.stopped().unwrap_or(Stop::Cancel));
        return;
    }
    phase(&w, "verify");
    let asset = match verify(&media, &item.id, &root.id, &file) {
        Ok(a) => a,
        Err(code) => {
            w.end_error(code, None);
            return;
        }
    };
    phase(&w, "finalize");
    let target = vv_import::fsutil::resolve_ref(Path::new(&root.path), &asset.storage_ref);
    let cp = format!("{PLACED}{}/{}", root.id, asset.storage_ref);
    let now = cx.now();
    if cx
        .store
        .write(|c| jobs::set_checkpoint(c, w.id(), w.generation(), Some(&cp), now))
        .is_err()
    {
        return; // Lost the lease: the new holder decides.
    }
    if let Err(code) = place(&file, &target, asset.content_hash.as_deref()) {
        w.end_error(code, None);
        return;
    }
    if publish(&w, &asset, &attempt) {
        let _ = std::fs::remove_dir_all(&dir);
        thumbnail_after(cx, &media);
    } else {
        // Not published (cancelled at the last moment, or the lease is
        // gone): the placed file belongs to no asset, so it goes.
        let _ = std::fs::remove_file(&target);
    }
}

/// Probes, sizes and hashes the staged file and describes the asset that
/// will be published. The file is not moved yet.
fn verify(
    media: &vv_import::MediaTools,
    item_id: &str,
    root_id: &str,
    file: &Path,
) -> Result<MediaAsset, ErrorCode> {
    let probed = media.probe(file).map_err(|e| e.code())?;
    if probed.video.is_none() && probed.audio.is_none() {
        return Err(ErrorCode::MediaCorrupt);
    }
    let size = std::fs::metadata(file)
        .map_err(|_| ErrorCode::FileMissing)?
        .len();
    if size == 0 {
        return Err(ErrorCode::MediaCorrupt);
    }
    let hash = vv_import::import::sha256(file, &AtomicBool::new(false))
        .map_err(|_| ErrorCode::FileMissing)?;
    let ext = file
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| e.len() <= 8 && e.bytes().all(|b| b.is_ascii_alphanumeric()))
        .unwrap_or("bin")
        .to_ascii_lowercase();
    let kind = if probed.video.is_some() {
        MediaKind::Video
    } else {
        MediaKind::Audio
    };
    let mut a = MediaAsset::new(
        item_id,
        AssetRole::Original,
        kind,
        StorageKind::Managed,
        root_id,
        "",
        0,
    );
    a.storage_ref = format!("media/{item_id}/{}.{ext}", a.id);
    vv_import::import::describe(&mut a, &probed, file);
    a.size_bytes = Some(size as i64);
    a.hash_algorithm = hash.as_ref().map(|_| "sha256".to_owned());
    a.content_hash = hash;
    a.acquisition_status = AcquisitionStatus::Ready;
    a.integrity_state = IntegrityState::Verified;
    Ok(a)
}

/// Moves the verified file into place: rename on the same volume, else copy,
/// check size and hash, then remove the staged copy.
pub fn place(from: &Path, to: &Path, hash: Option<&str>) -> Result<(), ErrorCode> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| storage::io_code(&e))?;
    }
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to).map_err(|e| {
        let _ = std::fs::remove_file(to);
        storage::io_code(&e)
    })?;
    let ok_size = std::fs::metadata(from).map(|m| m.len()).ok()
        == std::fs::metadata(to).map(|m| m.len()).ok();
    let ok_hash = match hash {
        None => true,
        Some(h) => {
            vv_import::import::sha256(to, &AtomicBool::new(false))
                .ok()
                .flatten()
                .as_deref()
                == Some(h)
        }
    };
    if !(ok_size && ok_hash) {
        let _ = std::fs::remove_file(to);
        return Err(ErrorCode::MediaCorrupt);
    }
    let _ = std::fs::remove_file(from);
    Ok(())
}

fn publish<S: Store>(w: &Work<'_, S>, asset: &MediaAsset, attempt: &crate::auth::Attempt) -> bool {
    let cx = w.cx;
    let now = cx.now();
    let mut a = asset.clone();
    a.created_at = now;
    a.acquired_at = Some(now);
    let (id, g) = (w.id().to_owned(), w.generation());
    let item_id = a.library_item_id.clone();
    let r = cx.store.write(|c| {
        jobs::succeed_with(c, &id, g, now, |tx| {
            repo::insert_media_asset(tx, &a).map_err(|_| jobs::JobError::Invalid("media asset"))?;
            tx.execute(
                "UPDATE library_item SET duration_ms = coalesce(duration_ms, ?2), revision = revision + 1, updated_at = ?3 WHERE id = ?1",
                params![item_id, a.duration_ms, now],
            )?;
            tx.execute(
                "UPDATE job SET checkpoint_ref = NULL WHERE id = ?1",
                params![id],
            )?;
            crate::auth::on_success(tx, &item_id, attempt, now);
            Ok(())
        })
    });
    cx.events.job_changed(&id, Some(&item_id));
    cx.events.item_changed(&item_id);
    r.is_ok()
}

/// A file an earlier attempt placed but never published. If it is still
/// there and still verifies, publish it; otherwise forget it and download
/// again. `Some(true)` when the job is finished.
fn adopt_placed<S: Store>(
    w: &Work<'_, S>,
    media: &vv_import::MediaTools,
    item_id: &str,
    root_id: &str,
    storage_ref: &str,
) -> Option<bool> {
    let cx = w.cx;
    let root = cx
        .store
        .read(|c| repo::get_storage_root(c, root_id).ok().flatten())?;
    let path = vv_import::fsutil::resolve_ref(Path::new(&root.path), storage_ref);
    let already = cx
        .store
        .read(|c| repo::asset_at(c, root_id, storage_ref).ok().flatten());
    if already.is_some() || !path.is_file() {
        return Some(false);
    }
    phase(w, "verify");
    let Ok(mut asset) = verify(media, item_id, root_id, &path) else {
        return Some(false);
    };
    // Keep the asset id the file is named after.
    if let Some(id) = Path::new(storage_ref).file_stem().and_then(|s| s.to_str()) {
        asset.id = id.to_owned();
    }
    asset.storage_ref = storage_ref.to_owned();
    if publish(w, &asset, &crate::auth::Attempt::anonymous()) {
        let _ = std::fs::remove_dir_all(staging_dir(&cx.dirs.staging, w.id()));
        Some(true)
    } else {
        Some(false)
    }
}

fn thumbnail_after<S: Store>(cx: &Context<S>, media: &vv_import::MediaTools) {
    cx.store.write(|c| {
        if let Ok(cache) =
            vv_import::thumbnail::ensure_cache_root(c, cx.clock.as_ref(), &cx.dirs.cache)
        {
            let _ = vv_import::thumbnail::generate_missing(c, cx.clock.as_ref(), media, &cache, 4);
        }
    });
}

/// Removes a finished or cancelled job's partial files (the user's
/// "Delete partial file" action). Never touches a running job.
pub fn remove_partial(staging: &Path, job: &jobs::Job) -> Result<u64, ErrorCode> {
    if job.status.is_active() {
        return Err(ErrorCode::InputRejected);
    }
    let dir = staging_dir(staging, &job.id);
    let n = storage::dir_size(&dir);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| storage::io_code(&e))?;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vv_ytdlp::progress::DownloadProgress;

    fn dl(status: DownloadStatus, done: u64, total: Option<u64>) -> ProgressEvent {
        ProgressEvent::Download(DownloadProgress {
            status,
            bytes_done: done,
            bytes_total: total,
            bytes_total_estimate: None,
            units_done: None,
            units_total: None,
            percentage: None,
            eta_seconds: None,
            speed_bytes_per_second: None,
        })
    }

    #[test]
    fn merged_downloads_are_two_parts_then_merge_never_one_percentage() {
        let mut t = Tracker {
            merge: true,
            part: 0,
            finished_parts: 0,
        };
        let (p, force) = t.map(&dl(DownloadStatus::Downloading, 10, Some(100)));
        assert!(force);
        assert_eq!(p.phase.as_deref(), Some("download_video"));
        assert_eq!((p.bytes_done, p.bytes_total), (Some(10), Some(100)));
        t.map(&dl(DownloadStatus::Finished, 100, Some(100)));
        let (p, force) = t.map(&dl(DownloadStatus::Downloading, 5, None));
        assert!(force);
        assert_eq!(p.phase.as_deref(), Some("download_audio"));
        assert_eq!(p.bytes_total, None, "unknown total stays unknown (AC-09)");
        assert_eq!(p.bytes_estimate, None);
        let mut e = dl(DownloadStatus::Downloading, 7, None);
        if let ProgressEvent::Download(d) = &mut e {
            d.bytes_total_estimate = Some(70);
        }
        let (p, _) = t.map(&e);
        assert_eq!(
            (p.bytes_total, p.bytes_estimate),
            (None, Some(70)),
            "an estimate is kept apart from the exact total"
        );
        let (p, _) = t.map(&ProgressEvent::Postprocess {
            postprocessor: "Merger".into(),
            status: PostprocessStatus::Started,
        });
        assert_eq!(p.phase.as_deref(), Some("merge"));
        assert_eq!(p.bytes_done, None);
    }
}
