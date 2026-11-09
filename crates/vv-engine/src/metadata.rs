//! `metadata` jobs: remote title, creator, duration, date, format list and
//! thumbnail through the guarded yt-dlp (STATE.md F3.4; SPEC CAP-04, CAP-07,
//! §3 "uzak metadata", §12.1, §20.2; ADR-0021).
//!
//! - Saving a bookmark never waits for this job; the item keeps its honest
//!   title from the link until metadata arrives (CAP-04).
//! - What is written: platform title, creator, duration, published date,
//!   `metadata_status`, access/availability (§12.1) and the allowlisted
//!   format list. User overrides are never touched (they win on display).
//! - The thumbnail is fetched by yt-dlp through the same guard, converted
//!   to JPEG by the local FFmpeg, checked (JPEG magic, at most 4 MiB) and
//!   stored as a `thumbnail` asset in the cache root. Its `source_metadata`
//!   holds only the redacted source (`scheme://host`).

use crate::context::{Context, Store};
use crate::formats::StoredFormat;
use crate::worker::Work;
use crate::ytdlp::{self, Mode, RunResult};
use std::path::{Path, PathBuf};
use vv_domain::error::ErrorCode;
use vv_persistence::classify::{self, Gone};
use vv_persistence::jobs::{self, JobKind, JobStatus, Lease, NewJob};
use vv_persistence::library::repo::{self, MediaAsset};
use vv_persistence::library::*;
use vv_persistence::rusqlite::{params, Connection};
use vv_ytdlp::metadata::{Extracted, VideoMetadata};

/// Largest remote thumbnail kept (the thumbnail protocol's limit).
pub const MAX_THUMBNAIL_BYTES: u64 = 4 * 1024 * 1024;
/// Bumped when what a metadata job writes changes.
pub const METADATA_CONFIG_VERSION: u32 = 1;

/// Queues a metadata job for an item (idempotent: one open job per item).
pub fn enqueue(c: &mut Connection, item_id: &str, now: i64) -> jobs::Result<jobs::Enqueued> {
    let mut job = NewJob::new(
        JobKind::Metadata,
        format!("metadata:{item_id}:v{METADATA_CONFIG_VERSION}"),
    );
    job.item_id = Some(item_id.to_owned());
    job.priority = 1;
    jobs::enqueue(c, &job, now)
}

/// Items still waiting for metadata with no open metadata job (bookmarks
/// saved in M1, or before the engine ran). Queued at start-up.
pub fn enqueue_pending(c: &mut Connection, now: i64, limit: u32) -> jobs::Result<usize> {
    let ids: Vec<String> = {
        let mut st = c.prepare(
            "SELECT i.id FROM library_item i WHERE i.metadata_status = 'pending' AND i.kind <> 'local_video' \
             AND i.removed_at IS NULL AND NOT EXISTS (SELECT 1 FROM job j WHERE j.item_id = i.id AND j.type = 'metadata') \
             ORDER BY i.added_at LIMIT ?1",
        )?;
        let rows = st.query_map([limit], |r| r.get(0))?;
        rows.collect::<vv_persistence::rusqlite::Result<_>>()?
    };
    for id in &ids {
        enqueue(c, id, now)?;
    }
    Ok(ids.len())
}

fn set_status(c: &Connection, item_id: &str, status: MetadataStatus, now: i64) {
    let _ = c.execute(
        "UPDATE library_item SET metadata_status = ?2, revision = revision + 1, updated_at = ?3 WHERE id = ?1",
        params![item_id, status.as_str(), now],
    );
}

/// `20261109` -> `2026-11-09`.
fn iso_date(d: &str) -> Option<String> {
    (d.len() == 8).then(|| format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..8]))
}

/// `scheme://host[:port]` of a URL, for `source_metadata` (§11.3).
pub fn redacted_origin(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?;
    Some(format!("{scheme}://{host}"))
}

fn write_metadata(
    c: &Connection,
    item: &repo::LibraryItem,
    m: &VideoMetadata,
    tool_version: &str,
    now: i64,
) -> vv_persistence::rusqlite::Result<()> {
    let creator = m.uploader.clone().or_else(|| m.channel.clone());
    let duration = m.duration_seconds.map(|s| (s * 1000.0).round() as i64);
    // Precision follows what the platform gave: a date, or a timestamp.
    let (published_at, published_date, precision) =
        match (m.upload_date.as_deref().and_then(iso_date), m.timestamp) {
            (Some(d), _) => (None, Some(d), Some("date")),
            (None, Some(t)) => (Some(t * 1000), None, Some("datetime")),
            (None, None) => (None, None, None),
        };
    let provenance = serde_json::json!({
        "source": "yt-dlp",
        "tool_version": tool_version,
        "extractor": m.extractor_key,
        "fetched_at": now,
    })
    .to_string();
    c.execute(
        "UPDATE library_item SET title = ?2, creator = coalesce(?3, creator), duration_ms = coalesce(?4, duration_ms), \
         published_at = CASE WHEN ?7 IS NULL THEN published_at ELSE ?5 END, \
         published_date = CASE WHEN ?7 IS NULL THEN published_date ELSE ?6 END, \
         published_precision = coalesce(?7, published_precision), \
         metadata_status = 'ready', last_checked_at = ?8, metadata_provenance = ?9, \
         revision = revision + 1, updated_at = ?8 WHERE id = ?1",
        params![
            item.id,
            m.title,
            creator,
            duration,
            published_at,
            published_date,
            precision,
            now,
            provenance
        ],
    )?;
    let formats: Vec<StoredFormat> = m.formats.iter().map(StoredFormat::from).collect();
    c.execute(
        "INSERT INTO remote_formats (item_id, fetched_at, extractor_key, tool_version, formats, is_live) VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT (item_id) DO UPDATE SET fetched_at = excluded.fetched_at, extractor_key = excluded.extractor_key, \
         tool_version = excluded.tool_version, formats = excluded.formats, is_live = excluded.is_live",
        params![
            item.id,
            now,
            m.extractor_key,
            tool_version,
            serde_json::to_string(&formats).unwrap_or_else(|_| "[]".into()),
            m.is_live.unwrap_or(false)
        ],
    )?;
    Ok(())
}

/// The stored format list of an item (empty when none was fetched).
pub fn stored_formats(c: &Connection, item_id: &str) -> Option<Vec<StoredFormat>> {
    let json: String = c
        .query_row(
            "SELECT formats FROM remote_formats WHERE item_id = ?1",
            [item_id],
            |r| r.get(0),
        )
        .ok()?;
    serde_json::from_str(&json).ok()
}

/// The last metadata run saw a live stream: it is not downloaded (§6).
pub fn is_live(c: &Connection, item_id: &str) -> bool {
    c.query_row(
        "SELECT is_live FROM remote_formats WHERE item_id = ?1",
        [item_id],
        |r| r.get::<_, bool>(0),
    )
    .unwrap_or(false)
}

/// A JPEG we are willing to keep: right magic, not empty, not huge.
fn acceptable_jpeg(p: &Path) -> bool {
    let Ok(md) = std::fs::metadata(p) else {
        return false;
    };
    if !md.is_file() || md.len() < 4 || md.len() > MAX_THUMBNAIL_BYTES {
        return false;
    }
    let mut head = [0u8; 3];
    use std::io::Read;
    std::fs::File::open(p)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && head == [0xFF, 0xD8, 0xFF]
}

pub fn run<S: Store>(cx: &Context<S>, lease: Lease) {
    let w = Work::new(cx, lease);
    let Some(item_id) = w.item_id().map(str::to_owned) else {
        w.end_error(ErrorCode::InternalError, Some("metadata job without item"));
        return;
    };
    let item = cx
        .store
        .read(|c| repo::get_item(c, &item_id).ok().flatten());
    let Some(item) = item else {
        return; // The item is gone; its jobs went with it.
    };
    let Some(url) = item.canonical_url.clone() else {
        w.end_error(ErrorCode::InvalidUrl, None);
        return;
    };
    // The guard checks every connection; this refuses obviously bad input
    // before a process starts (§20.2).
    if let Err(code) = ytdlp::precheck(&cx.policy, &url) {
        finish_failed(&w, &item_id, code, None);
        return;
    }
    let tools = cx.tools();
    let Some(yt) = tools.ytdlp.clone() else {
        let code = tools
            .report
            .ready_for_downloads()
            .err()
            .unwrap_or(ErrorCode::ToolMissing);
        end(&w, &item_id, code, None);
        return;
    };
    let now = cx.now();
    cx.store
        .write(|c| set_status(c, &item_id, MetadataStatus::Fetching, now));
    cx.events.item_changed(&item_id);
    let dir: PathBuf = cx
        .dirs
        .staging
        .join(format!("meta-{}-{}", w.id(), w.generation()));
    let _ = std::fs::remove_dir_all(&dir);
    if std::fs::create_dir_all(&dir).is_err() {
        end(&w, &item_id, ErrorCode::PathNotWritable, None);
        return;
    }
    let run = ytdlp::run(
        &yt,
        cx.run_policy(),
        &url,
        &Mode::Metadata {
            thumbnail_dir: Some(dir.clone()),
        },
        Some(ytdlp::METADATA_TIMEOUT),
        |_| {},
        || w.should_stop(),
    );
    match run.result {
        RunResult::Succeeded => match vv_ytdlp::metadata::parse(&run.stdout) {
            Ok(Extracted::Video(m)) => publish(&w, &item, &m, &dir, &tools.ytdlp_version),
            Ok(Extracted::Playlist { .. }) => {
                finish_failed(&w, &item_id, ErrorCode::PlaylistUnsupported, None)
            }
            Err(_) => end(&w, &item_id, ErrorCode::ExtractorFailed, None),
        },
        RunResult::Canceled => {
            let stop = w.stopped().unwrap_or(crate::worker::Stop::Cancel);
            w.end_stopped(stop);
            cx.store
                .write(|c| set_status(c, &item_id, MetadataStatus::Pending, cx.now()));
        }
        RunResult::Failed { code, message } => end(&w, &item_id, code, message.as_deref()),
    }
    let _ = std::fs::remove_dir_all(&dir);
    cx.events.item_changed(&item_id);
}

/// Ends the attempt with `code`; the item shows what that means (§12.1).
fn end<S: Store>(w: &Work<'_, S>, item_id: &str, code: ErrorCode, message: Option<&str>) {
    let status = w.end_error(code, message);
    let now = w.cx.now();
    w.cx.store.write(|c| {
        let st = match status {
            Some(JobStatus::Failed) => MetadataStatus::Failed,
            _ => MetadataStatus::Pending,
        };
        set_status(c, item_id, st, now);
        if status == Some(JobStatus::Failed) {
            let change = classify::state_change(code, Gone::Unavailable);
            let _ = classify::apply_to_item(c, item_id, &change, now);
        }
    });
}

/// Codes that are permanent whatever the queue's rule (a playlist URL, a
/// refused URL): the job fails at once.
fn finish_failed<S: Store>(w: &Work<'_, S>, item_id: &str, code: ErrorCode, msg: Option<&str>) {
    let now = w.cx.now();
    let (id, g) = (w.id().to_owned(), w.generation());
    let failed = w.cx.store.write(|c| {
        let r = jobs::fail(
            c,
            &id,
            g,
            &jobs::Failure::Permanent,
            code.as_str(),
            msg,
            now,
        );
        set_status(c, item_id, MetadataStatus::Failed, now);
        let change = classify::state_change(code, Gone::Unavailable);
        let _ = classify::apply_to_item(c, item_id, &change, now);
        r
    });
    let _ = failed;
    w.cx.events.job_changed(&id, Some(item_id));
}

fn publish<S: Store>(
    w: &Work<'_, S>,
    item: &repo::LibraryItem,
    m: &VideoMetadata,
    dir: &Path,
    tool_version: &str,
) {
    if m.has_drm {
        finish_failed(w, &item.id, ErrorCode::DrmUnsupported, None);
        return;
    }
    let cx = w.cx;
    let now = cx.now();
    // The thumbnail file goes into the cache before the commit; if the
    // commit does not happen it is removed again.
    let thumb_src = dir.join("thumb.jpg");
    let mut thumb_file: Option<PathBuf> = None;
    let mut thumb_asset: Option<MediaAsset> = None;
    if item.thumbnail_asset_id.is_none() && acceptable_jpeg(&thumb_src) {
        let prepared = cx.store.write(|c| {
            vv_import::thumbnail::ensure_cache_root(c, cx.clock.as_ref(), &cx.dirs.cache).ok()
        });
        if let Some(root) = prepared {
            let mut a = MediaAsset::new(
                &item.id,
                AssetRole::Thumbnail,
                MediaKind::Image,
                StorageKind::Managed,
                &root.id,
                "",
                now,
            );
            a.storage_ref = format!("thumbnails/{}/{}.jpg", item.id, a.id);
            let target = vv_import::fsutil::resolve_ref(Path::new(&root.path), &a.storage_ref);
            let moved = target
                .parent()
                .map(std::fs::create_dir_all)
                .transpose()
                .ok()
                .and_then(|_| std::fs::rename(&thumb_src, &target).ok())
                .is_some();
            if moved {
                let size = std::fs::metadata(&target).map(|m| m.len() as i64).ok();
                a.mime_type = Some("image/jpeg".into());
                a.size_bytes = size;
                a.acquired_at = Some(now);
                a.acquisition_status = AcquisitionStatus::Ready;
                a.integrity_state = IntegrityState::Verified;
                a.source_metadata = Some(
                    serde_json::json!({
                        "v": 1,
                        "origin": m.webpage_url.as_deref().and_then(redacted_origin),
                    })
                    .to_string(),
                );
                a.derivation = Some(
                    serde_json::json!({
                        "generator": "yt-dlp",
                        "generator_version": tool_version,
                        "config": { "v": 1, "remote": true, "convert": "jpg" },
                        "created_at": now,
                    })
                    .to_string(),
                );
                thumb_file = Some(target);
                thumb_asset = Some(a);
            }
        }
    }
    let (id, g) = (w.id().to_owned(), w.generation());
    let r = cx.store.write(|c| {
        jobs::succeed_with(c, &id, g, now, |tx| {
            write_metadata(tx, item, m, tool_version, now)?;
            let change = classify::success_state_change(false);
            classify::apply_to_item(tx, &item.id, &change, now)?;
            if let Some(a) = &thumb_asset {
                repo::insert_media_asset(tx, a).map_err(lib)?;
                repo::set_thumbnail(tx, &item.id, Some(&a.id), now).map_err(lib)?;
            }
            let it = repo::get_item(tx, &item.id).map_err(lib)?;
            if let Some(it) = it {
                repo::write_search_document(tx, &it).map_err(lib)?;
            }
            Ok(())
        })
    });
    if r.is_err() {
        if let Some(f) = thumb_file {
            let _ = std::fs::remove_file(f);
        }
    }
    cx.events.job_changed(&id, Some(&item.id));
}

fn lib(e: LibraryError) -> jobs::JobError {
    match e {
        LibraryError::Sqlite(e) => jobs::JobError::Sqlite(e),
        _ => jobs::JobError::Invalid("library write"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins_keep_only_scheme_and_host() {
        assert_eq!(
            redacted_origin("https://user:pw@cdn.example.com:8443/a/b?sig=SECRET#x").as_deref(),
            Some("https://cdn.example.com:8443")
        );
        assert_eq!(redacted_origin("not a url"), None);
        assert_eq!(iso_date("20261109").as_deref(), Some("2026-11-09"));
    }
}
