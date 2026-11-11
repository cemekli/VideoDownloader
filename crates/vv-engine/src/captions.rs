//! `fetch_captions` jobs: keep the site's own captions as a transcript
//! (STATE.md F6.2; SPEC AI-A01, §8.4 step 2, §11.5; ADR-0027).
//!
//! 1. The guarded yt-dlp reads the page again (`-J`) and reports which
//!    caption tracks exist: names only, never their URLs.
//! 2. [`vv_ytdlp::captions::choose`] picks the track in the spoken
//!    language (the uploader's, else the site's automatic one; automatic
//!    translations never). No track: the job succeeds without a transcript
//!    and local transcription (F6.2b) is the way.
//! 3. The same guarded yt-dlp, with the same session as step 1, writes that
//!    one track as WebVTT into the job's staging folder; the local FFmpeg
//!    converts other formats.
//! 4. The file is parsed to plain text cues, checked against §11.5
//!    ([`transcripts::validate`]) and saved as a `source_captions`
//!    transcript with its language and manual/automatic label, in the same
//!    transaction that ends the job. A second run replaces it.
//!
//! The staging folder is removed afterwards whatever happened.

use crate::context::{Context, Store};
use crate::worker::{Stop, Work};
use crate::ytdlp::{self, Mode, Run, RunResult};
use sha2::{Digest, Sha256};
use std::path::Path;
use vv_domain::error::ErrorCode;
use vv_persistence::auth::Operation;
use vv_persistence::jobs::{self, JobKind, Lease, NewJob, Progress};
use vv_persistence::library::repo;
use vv_persistence::rusqlite::Connection;
use vv_persistence::transcripts::{self, CaptionKind, NewSegment, Origin, Provenance, Status};
use vv_ytdlp::captions::{self as caps, Choice};
use vv_ytdlp::metadata::Extracted;

/// Bumped when what a captions job writes changes.
pub const CAPTIONS_CONFIG_VERSION: u32 = 1;

/// Languages an uploader track is taken in when none is in the spoken
/// language (the app's Turkish and English focus, AI-A07).
pub const PREFER: &[&str] = &["tr", "en"];

/// Queues a captions job for an item (one open job per item).
pub fn enqueue(c: &mut Connection, item_id: &str, now: i64) -> jobs::Result<jobs::Enqueued> {
    let mut job = NewJob::new(
        JobKind::FetchCaptions,
        format!("fetch_captions:{item_id}:v{CAPTIONS_CONFIG_VERSION}"),
    );
    job.item_id = Some(item_id.to_owned());
    job.priority = 1;
    jobs::enqueue(c, &job, now)
}

fn phase<S: Store>(w: &Work<'_, S>, name: &str) {
    w.progress(
        &Progress {
            phase: Some(name.to_owned()),
            ..Progress::default()
        },
        true,
    );
}

fn sha256_hex(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

/// The cues of a caption file as segments.
pub fn segments(cues: Vec<caps::Cue>) -> Vec<NewSegment> {
    cues.into_iter()
        .map(|c| NewSegment::new(c.start_ms, c.end_ms, c.text))
        .collect()
}

fn kind(k: caps::CaptionKind) -> CaptionKind {
    match k {
        caps::CaptionKind::Manual => CaptionKind::Manual,
        caps::CaptionKind::Automatic => CaptionKind::Automatic,
    }
}

pub fn run<S: Store>(cx: &Context<S>, lease: Lease) {
    let w = Work::new(cx, lease);
    let Some(item_id) = w.item_id().map(str::to_owned) else {
        w.end_error(ErrorCode::InternalError, Some("captions job without item"));
        return;
    };
    let Some(item) = cx
        .store
        .read(|c| repo::get_item(c, &item_id).ok().flatten())
    else {
        return; // The item is gone; its jobs went with it.
    };
    let Some(url) = item.canonical_url.clone() else {
        w.end_error(ErrorCode::InputRejected, Some("item has no link"));
        return;
    };
    if let Err(code) = ytdlp::precheck(&cx.policy, &url) {
        w.end_error(code, None);
        return;
    }
    let tools = cx.tools();
    let Some(yt) = tools.ytdlp.clone() else {
        let code = tools
            .report
            .ready_for_downloads()
            .err()
            .unwrap_or(ErrorCode::ToolMissing);
        w.end_error(code, None);
        return;
    };
    let dir = cx
        .dirs
        .staging
        .join(format!("caps-{}-{}", w.id(), w.generation()));
    let _ = std::fs::remove_dir_all(&dir);
    if std::fs::create_dir_all(&dir).is_err() {
        w.end_error(ErrorCode::PathNotWritable, None);
        return;
    }
    run_in(&w, &item, &url, &yt, &dir);
    let _ = std::fs::remove_dir_all(&dir);
    cx.events.item_changed(&item_id);
}

fn run_in<S: Store>(
    w: &Work<'_, S>,
    item: &repo::LibraryItem,
    url: &str,
    yt: &ytdlp::YtdlpTools,
    dir: &Path,
) {
    let cx = w.cx;
    let exec = |attempt: &crate::auth::Attempt, mode: &Mode| -> Run {
        let prepared = match attempt.prepare(cx) {
            Ok(p) => p,
            Err(code) => {
                return Run {
                    result: RunResult::Failed {
                        code,
                        message: None,
                    },
                    stdout: Vec::new(),
                    files: Vec::new(),
                    connections: Vec::new(),
                }
            }
        };
        ytdlp::run_as(
            yt,
            cx.run_policy(),
            url,
            mode,
            &prepared.args,
            Some(ytdlp::METADATA_TIMEOUT),
            |_| {},
            || w.should_stop(),
        )
    };
    // 1. Which tracks are there?
    phase(w, "captions_list");
    let op = Operation::Metadata;
    let list = Mode::Metadata {
        thumbnail_dir: None,
    };
    let mut attempt = crate::auth::first(cx, item, op);
    let mut run = exec(&attempt, &list);
    if let (None, RunResult::Failed { code, .. }) = (&attempt.profile, &run.result) {
        if crate::auth::refused(*code) {
            if let Some(a) = crate::auth::after_refusal(cx, item, op, *code) {
                attempt = a;
                run = exec(&attempt, &list);
            }
        }
    }
    if !ended_ok(w, item, &attempt, run.result) {
        return;
    }
    let meta = match vv_ytdlp::metadata::parse(&run.stdout) {
        Ok(Extracted::Video(m)) => m,
        Ok(Extracted::Playlist { .. }) => {
            w.end_error(ErrorCode::PlaylistUnsupported, None);
            return;
        }
        Err(_) => {
            w.end_error(ErrorCode::ExtractorFailed, Some("caption list unreadable"));
            return;
        }
    };
    let duration_ms = meta
        .duration_seconds
        .map(|s| (s * 1000.0).round() as i64)
        .or(item.duration_ms);
    // 2. Which one is worth keeping?
    let Some(choice) = caps::choose(&meta.captions, PREFER) else {
        finish(w, item, &attempt, None);
        return;
    };
    // 3. Fetch it.
    phase(w, "captions_fetch");
    let fetch = Mode::Captions {
        dir: dir.to_path_buf(),
        track: choice.track.clone(),
        automatic: choice.kind == caps::CaptionKind::Automatic,
    };
    let run = exec(&attempt, &fetch);
    if !ended_ok(w, item, &attempt, run.result) {
        return;
    }
    let file = dir.join(format!("caps.{}.vtt", choice.track));
    if !ytdlp::inside(dir, &file) {
        w.end_error(ErrorCode::ExtractorFailed, Some("caption file missing"));
        return;
    }
    let too_big = std::fs::metadata(&file)
        .map(|m| m.len() > caps::MAX_VTT_BYTES as u64)
        .unwrap_or(true);
    let bytes = match (too_big, std::fs::read(&file)) {
        (false, Ok(b)) => b,
        _ => {
            w.end_error(
                ErrorCode::MediaCorrupt,
                Some("caption file too large or unreadable"),
            );
            return;
        }
    };
    // 4. Check and keep it.
    phase(w, "captions_save");
    let cues = match caps::parse_vtt(&bytes) {
        Ok(c) => c,
        Err(_) => {
            w.end_error(ErrorCode::MediaCorrupt, Some("caption file is not WebVTT"));
            return;
        }
    };
    let segs = segments(cues);
    if segs.is_empty() {
        finish(w, item, &attempt, None);
        return;
    }
    let segs = match transcripts::validate(&segs, duration_ms) {
        Ok(s) => s,
        Err(e) => {
            w.end_error(ErrorCode::MediaCorrupt, Some(&e.to_string()));
            return;
        }
    };
    let origin = Origin {
        item_id: item.id.clone(),
        asset_id: None,
        language: choice.language.clone(),
        provenance: Provenance::SourceCaptions,
        provider: "source_captions".into(),
        model_id: None,
        model_version: None,
        source_fingerprint: fingerprint(&choice, &bytes),
        caption_kind: Some(kind(choice.kind)),
    };
    finish(w, item, &attempt, Some((origin, segs, duration_ms)));
}

/// `track:kind:sha256 of the file`, so a changed track is seen as one.
fn fingerprint(choice: &Choice, bytes: &[u8]) -> String {
    let k = match choice.kind {
        caps::CaptionKind::Manual => "manual",
        caps::CaptionKind::Automatic => "automatic",
    };
    format!("{}:{k}:sha256:{}", choice.track, sha256_hex(bytes))
}

/// `true` when the run succeeded; otherwise the job is ended as the run
/// says (stopped, or failed with its code).
fn ended_ok<S: Store>(
    w: &Work<'_, S>,
    item: &repo::LibraryItem,
    attempt: &crate::auth::Attempt,
    result: RunResult,
) -> bool {
    match result {
        RunResult::Succeeded => true,
        RunResult::Canceled => {
            w.end_stopped(w.stopped().unwrap_or(Stop::Cancel));
            false
        }
        RunResult::Failed { code, message } => {
            let now = w.cx.now();
            w.cx.store
                .write(|c| crate::auth::after_failure(c, &item.id, attempt, code, now));
            w.end_error(code, message.as_deref());
            false
        }
    }
}

type Found = (Origin, Vec<NewSegment>, Option<i64>);

fn finish<S: Store>(
    w: &Work<'_, S>,
    item: &repo::LibraryItem,
    attempt: &crate::auth::Attempt,
    found: Option<Found>,
) {
    let cx = w.cx;
    let now = cx.now();
    let (id, g) = (w.id().to_owned(), w.generation());
    let r = cx.store.write(|c| {
        jobs::succeed_with(c, &id, g, now, |tx| {
            crate::auth::on_success(tx, &item.id, attempt, now);
            if let Some((origin, segs, duration)) = &found {
                let covered = segs.last().map(|s| s.end_ms);
                transcripts::save(tx, origin, Status::Ready, covered, segs, *duration, now)
                    .map_err(|e| match e {
                        transcripts::TranscriptError::Sqlite(e) => jobs::JobError::Sqlite(e),
                        _ => jobs::JobError::Invalid("transcript"),
                    })?;
            }
            Ok(())
        })
    });
    match r {
        Ok(()) | Err(jobs::JobError::Stale) => {}
        // Cancelled while saving: nothing was kept.
        Err(jobs::JobError::InvalidTransition { .. }) => {
            w.end_stopped(Stop::Cancel);
        }
        Err(_) => {
            w.end_error(ErrorCode::InternalError, Some("captions not saved"));
        }
    }
    cx.events.job_changed(&id, Some(&item.id));
}
