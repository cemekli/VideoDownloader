//! `ocr` jobs: the text on screen, read on this PC (STATE.md F7.4; SPEC
//! AI-C01…C04, SEA-08, AC-41, DEC-13; ADR-0039).
//!
//! 1. Frames, never all of them (AI-C01): the pinned FFmpeg decodes the
//!    item's own video file (original first, then a preview) with the
//!    fixed input allowlists and keeps a frame at least every
//!    [`FramePick::every_ms`] and at each scene change at least a second
//!    after the last one, at most [`MAX_FRAMES`] (AI-C02), as PNG in the
//!    job's staging folder. Only keyframes are decoded first; a file with
//!    too few of them (a screen recording) gets a full pass at four frames
//!    a second.
//! 2. Each frame goes to the OCR ([`vv_ocr`]; Windows' own on Windows) in
//!    the user's language. Lines with fewer than two letters or digits are
//!    noise and dropped.
//! 3. Frames in a row that show about the same text (the same words, give
//!    or take OCR slips) become one line of text on screen, from the time
//!    of its first frame to the time of the frame that no longer shows it
//!    (AI-C03). The item's lines replace what was read before, in one
//!    transaction ([`vv_persistence::screen_text`]).
//!
//! A job with an item reads that item again. A job without one ("Read
//! text on screen for all videos") goes through the live items whose video
//! was not read with this file and these rules, newest first, one at a
//! time, and can be cancelled between frames; what it read so far stays.
//! It is a heavy AI job: one at a time with transcription and the search
//! index (§13.3). Nothing here uses the network.

use crate::context::{Context, Store};
use crate::proc::{run_streaming, spawn_error_code, StdoutMode};
use crate::transcribe::asset_path;
use crate::worker::{Stop, Work};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use vv_domain::error::ErrorCode;
use vv_ffmpeg::args::{self as ff, FramePick};
use vv_persistence::jobs::{self, JobKind, Lease, NewJob, Progress};
use vv_persistence::library::{repo, AssetRole, IntegrityState, MediaKind};
use vv_persistence::rusqlite::Connection;
use vv_persistence::screen_text::{self, Line, Source};
use vv_process::ManagedCommand;
use vv_ytdlp::outcome::{Exit, Stream};

/// Bumped when the frames or the rules change; older runs are read again
/// by the next run for all videos.
pub const CONFIG_VERSION: i64 = 1;
/// Frames per video at most (AI-C02).
pub const MAX_FRAMES: u32 = 300;
/// Frames wider than this are scaled down (Windows' OCR reads up to about
/// 2,600 pixels a side; small print stays legible at 1080p).
pub const MAX_WIDTH: u32 = 1920;
/// Scene change threshold (0.3 of FFmpeg's scene score).
pub const SCENE_MILLI: u32 = 300;
/// Shortest and longest gap between sampled frames.
pub const EVERY_MIN_MS: u64 = 2_000;
pub const EVERY_MAX_MS: u64 = 30_000;
/// A frame whose words overlap the frame before it at least this much
/// (shared / all) shows the same text: one OCR slip in a five-word title
/// still counts as the same.
pub const SAME_TEXT: f64 = 0.5;

/// The idempotency key of the run for all videos: one at a time.
pub const ALL_KEY: &str = "ocr:all";

/// Which frames for a video of `duration_ms`: about 150 frames spread over
/// it, at least every 2 s and at most every 30 s, plus scene changes.
pub fn pick(duration_ms: Option<i64>, keyframes_only: bool) -> FramePick {
    let every = match duration_ms {
        Some(d) if d > 0 => (d as u64 / 150).clamp(EVERY_MIN_MS, EVERY_MAX_MS),
        _ => 5_000,
    };
    FramePick {
        every_ms: every,
        scene_gap_ms: (every / 3).max(1_000),
        scene_milli: SCENE_MILLI,
        max_frames: MAX_FRAMES,
        max_width: MAX_WIDTH,
        keyframes_only,
    }
}

/// Frames a pass over `duration_ms` should give at least; a keyframe pass
/// with fewer is done again in full.
fn expected(duration_ms: Option<i64>, p: &FramePick) -> usize {
    match duration_ms {
        Some(d) if d > 0 => ((d as u64 / p.every_ms) as usize).min(p.max_frames as usize),
        _ => 0,
    }
}

/// The video file whose frames are read: the item's original, else a
/// preview, not known to be missing.
pub fn video_asset(c: &Connection, item_id: &str) -> Option<repo::MediaAsset> {
    let mut assets: Vec<_> = repo::assets_for_item(c, item_id)
        .ok()?
        .into_iter()
        .filter(|a| {
            matches!(a.role, AssetRole::Original | AssetRole::Preview)
                && a.media_kind == MediaKind::Video
                && a.integrity_state != IntegrityState::Missing
        })
        .collect();
    assets.sort_by_key(|a| (a.role != AssetRole::Original, a.created_at));
    assets.into_iter().next()
}

/// What makes a run current: the file (its hash, else its size).
pub fn fingerprint(asset: &repo::MediaAsset) -> String {
    let media = match (&asset.content_hash, asset.size_bytes) {
        (Some(h), _) => format!("sha256:{}", h.chars().take(64).collect::<String>()),
        (None, Some(n)) => format!("size:{n}"),
        (None, None) => "unhashed".into(),
    };
    format!("asset:{}:{media}", asset.id)
}

/// Live items with a video whose text was not read with this file and
/// these rules, newest first.
pub fn pending(c: &Connection) -> Vec<String> {
    let ids: Vec<String> = c
        .prepare("SELECT id FROM library_item WHERE removed_at IS NULL ORDER BY added_at DESC, id")
        .and_then(|mut st| {
            st.query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()
        })
        .unwrap_or_default();
    ids.into_iter()
        .filter(|id| {
            let Some(asset) = video_asset(c, id) else {
                return false;
            };
            match screen_text::get(c, id).ok().flatten() {
                Some(r) => {
                    r.config_version != CONFIG_VERSION
                        || r.source_fingerprint != fingerprint(&asset)
                }
                None => true,
            }
        })
        .collect()
}

/// Live items with a video (what "all videos" means).
pub fn with_video(c: &Connection) -> i64 {
    c.query_row(
        "SELECT count(DISTINCT i.id) FROM library_item i JOIN media_asset a ON a.library_item_id = i.id \
         WHERE i.removed_at IS NULL AND a.media_kind = 'video' AND a.role IN ('original', 'preview') \
           AND a.integrity_state <> 'missing'",
        [],
        |r| r.get(0),
    )
    .unwrap_or(0)
}

/// Queues "read the text on screen" for all videos (one open run at a
/// time), or for one item again.
pub fn request(
    c: &mut Connection,
    item_id: Option<&str>,
    now: i64,
) -> jobs::Result<jobs::Enqueued> {
    let mut job = NewJob::new(
        JobKind::Ocr,
        match item_id {
            Some(id) => format!("ocr:item:{id}:v{CONFIG_VERSION}"),
            None => ALL_KEY.to_owned(),
        },
    );
    job.item_id = item_id.map(str::to_owned);
    // A failed run would fail the same way again; the user asks again.
    job.max_attempts = 1;
    jobs::enqueue(c, &job, now)
}

/// The job's staging folder.
pub fn staging_dir(staging: &Path, job_id: &str) -> PathBuf {
    staging.join(format!("ocr-{job_id}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    Ended,
    Lost,
}

fn stopped<S: Store>(w: &Work<'_, S>) -> End {
    let s = w.stopped().unwrap_or(Stop::Cancel);
    w.end_stopped(s);
    if s == Stop::Lost {
        End::Lost
    } else {
        End::Ended
    }
}

pub fn run<S: Store>(cx: &Context<S>, lease: Lease) {
    let w = Work::new(cx, lease);
    let reader = match cx.ocr.open() {
        Ok(r) => r,
        Err(e) => {
            // Not a tool that comes back after a check: the user installs
            // an OCR language in Windows and asks again.
            let msg = match e {
                vv_ocr::OcrError::Unavailable(_) => "no OCR language is installed on this PC",
                _ => "the OCR could not start",
            };
            let now = cx.now();
            let (id, g) = (w.id().to_owned(), w.generation());
            cx.store.write(|c| {
                let _ = jobs::fail(
                    c,
                    &id,
                    g,
                    &jobs::Failure::Permanent,
                    ErrorCode::ModelMissing.as_str(),
                    Some(msg),
                    now,
                );
            });
            cx.events.job_changed(&id, w.item_id());
            return;
        }
    };
    let Some(ffmpeg) = cx.tools().media.map(|m| m.ffmpeg) else {
        w.end_error(ErrorCode::ToolMissing, None);
        return;
    };
    let one = w.item_id().map(str::to_owned);
    let items = match &one {
        Some(id) => vec![id.clone()],
        None => cx.store.read(pending),
    };
    let dir = staging_dir(&cx.dirs.staging, w.id());
    let total = items.len() as i64;
    let mut end = None;
    for (k, id) in items.iter().enumerate() {
        w.progress(
            &Progress {
                phase: Some("read".into()),
                units_done: Some(k as i64),
                units_total: Some(total),
                ..Progress::default()
            },
            k == 0,
        );
        if w.should_stop() {
            end = Some(stopped(&w));
            break;
        }
        let _ = std::fs::remove_dir_all(&dir);
        if std::fs::create_dir_all(&dir).is_err() {
            w.end_error(ErrorCode::PathNotWritable, None);
            end = Some(End::Ended);
            break;
        }
        let r = read_item(&w, reader.as_ref(), &ffmpeg, &dir, id, one.is_some());
        cx.events.item_changed(id);
        if let Err(e) = r {
            end = Some(e);
            break;
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    if end.is_some() {
        return;
    }
    let now = cx.now();
    let (id, g) = (w.id().to_owned(), w.generation());
    let r = cx.store.write(|c| {
        let _ = jobs::report_progress(
            c,
            &id,
            g,
            &Progress {
                phase: Some("read".into()),
                units_done: Some(total),
                units_total: Some(total),
                ..Progress::default()
            },
            now,
        );
        jobs::succeed(c, &id, g, now)
    });
    if let Err(jobs::JobError::InvalidTransition { .. }) = r {
        w.end_stopped(Stop::Cancel);
    }
    cx.events.job_changed(&id, one.as_deref());
}

/// Reads one item; `Err` when the job was ended here. A file that cannot
/// be read is recorded for the item (and ends a job for that item alone).
fn read_item<S: Store>(
    w: &Work<'_, S>,
    reader: &dyn vv_ocr::Reader,
    ffmpeg: &Path,
    dir: &Path,
    item_id: &str,
    alone: bool,
) -> Result<(), End> {
    let cx = w.cx;
    let found = cx.store.read(|c| {
        let asset = video_asset(c, item_id)?;
        let path = asset_path(c, &asset.id);
        let duration = asset.duration_ms.or_else(|| {
            repo::get_item(c, item_id)
                .ok()
                .flatten()
                .and_then(|i| i.duration_ms)
        });
        Some((asset, path, duration))
    });
    let fail = |code: ErrorCode, msg: &str| -> Result<(), End> {
        if alone {
            w.end_error(code, Some(msg));
            Err(End::Ended)
        } else {
            Ok(())
        }
    };
    let Some((asset, path, duration)) = found else {
        return fail(ErrorCode::FileMissing, "the item has no video file");
    };
    let fp = fingerprint(&asset);
    let src = Source {
        item_id,
        asset_id: Some(&asset.id),
        provider: reader.provider(),
        language: reader.language(),
        config_version: CONFIG_VERSION,
        fingerprint: &fp,
    };
    let record_failed = |code: ErrorCode, msg: &str| -> Result<(), End> {
        let now = cx.now();
        cx.store.write(|c| {
            let _ = screen_text::save_failed(c, &src, code.as_str(), now);
        });
        fail(code, msg)
    };
    let path = match path {
        Ok(p) => p,
        // A drive that is not connected now: tried again on the next run.
        Err(ErrorCode::StorageRootUnavailable) => {
            return fail(
                ErrorCode::StorageRootUnavailable,
                "the drive is not connected",
            );
        }
        Err(code) => return record_failed(code, "the video file is not there"),
    };

    // 1. Frames: keyframes, then a full pass when they were too few.
    let mut times = frames(w, ffmpeg, &path, dir, &pick(duration, true))?;
    let full = pick(duration, false);
    if times.len() < expected(duration, &full) / 3 {
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::create_dir_all(dir);
        let more = frames(w, ffmpeg, &path, dir, &full)?;
        if more.len() > times.len() {
            times = more;
        }
    }
    if times.is_empty() {
        return record_failed(ErrorCode::MediaCorrupt, "no frames could be read");
    }

    // 2. OCR, frame by frame.
    let mut read: Vec<(i64, Vec<String>)> = Vec::with_capacity(times.len());
    let mut failures = 0usize;
    for (k, &at) in times.iter().enumerate() {
        if w.should_stop() {
            return Err(stopped(w));
        }
        let png = std::fs::read(dir.join(format!("f-{:05}.png", k + 1)));
        let lines = match png.map(|b| reader.read_png(&b)) {
            Ok(Ok(lines)) => clean(&lines),
            _ => {
                failures += 1;
                Vec::new()
            }
        };
        read.push((at, lines));
    }
    if failures == times.len() {
        return record_failed(ErrorCode::InternalError, "the OCR read no frame");
    }

    // 3. Lines of text on screen, saved for the item.
    let last = times.last().copied().unwrap_or(0);
    let end_ms = match duration {
        Some(d) if d > last => d.min(last + full.every_ms as i64),
        _ => last + full.every_ms as i64,
    };
    let lines = merge(&read, end_ms);
    if w.should_stop() {
        return Err(stopped(w));
    }
    let now = cx.now();
    cx.store.write(|c| {
        let _ = screen_text::save(c, &src, times.len() as i64, &lines, now);
    });
    Ok(())
}

/// Runs FFmpeg for one pass; the times of the frames it wrote.
fn frames<S: Store>(
    w: &Work<'_, S>,
    ffmpeg: &Path,
    input: &Path,
    dir: &Path,
    p: &FramePick,
) -> Result<Vec<i64>, End> {
    let Ok(args) = ff::frames_args(input, dir, p) else {
        w.end_error(ErrorCode::InternalError, Some("media path is not absolute"));
        return Err(End::Ended);
    };
    run_tool(w, ffmpeg, args, dir)?;
    // A file that ends early still gives the frames before the damage.
    let list = std::fs::read_to_string(dir.join(ff::FRAME_LIST)).unwrap_or_default();
    let mut times = ff::parse_frame_list(&list);
    // Only frames whose file is there, in order.
    let written = (1..=times.len())
        .take_while(|k| dir.join(format!("f-{k:05}.png")).is_file())
        .count();
    times.truncate(written);
    Ok(times)
}

fn run_tool<S: Store>(
    w: &Work<'_, S>,
    exe: &Path,
    args: Vec<OsString>,
    dir: &Path,
) -> Result<Exit, End> {
    let mut cmd = ManagedCommand::new(exe);
    cmd.args(&args).current_dir(dir).low_priority();
    match run_streaming(
        cmd,
        StdoutMode::Lines,
        None,
        |_: Stream, _: &str| {},
        || w.should_stop(),
    ) {
        Err(e) => {
            w.end_error(spawn_error_code(&e), None);
            Err(End::Ended)
        }
        Ok(s) if s.stopped => Err(stopped(w)),
        Ok(s) => Ok(s.exit),
    }
}

/// Lines with at least two letters or digits, spaces collapsed.
pub fn clean(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| l.chars().filter(|c| c.is_alphanumeric()).count() >= 2)
        .collect()
}

fn words(lines: &[String]) -> std::collections::HashSet<String> {
    lines
        .iter()
        .flat_map(|l| l.split(|c: char| !c.is_alphanumeric()))
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect()
}

/// Frames (time, lines) in time order → lines of text on screen. Frames in
/// a row with about the same words are one line, from the first frame to
/// the next frame without them (the last until `end_ms`); the frame with
/// the most words gives its text.
pub fn merge(frames: &[(i64, Vec<String>)], end_ms: i64) -> Vec<Line> {
    struct Open {
        start: i64,
        /// The words of the frame before (compared with the next one).
        last: std::collections::HashSet<String>,
        /// Words of the frame whose text is kept (the one with the most).
        most: usize,
        text: Vec<String>,
    }
    let mut out = Vec::new();
    let mut cur: Option<Open> = None;
    let close = |o: Open, at: i64, out: &mut Vec<Line>| {
        if at > o.start {
            out.push(Line {
                start_ms: o.start,
                end_ms: at,
                text: o.text.join("\n"),
            });
        }
    };
    for (at, lines) in frames {
        let w = words(lines);
        match cur.take() {
            Some(mut o) if !w.is_empty() && jaccard(&o.last, &w) >= SAME_TEXT => {
                if w.len() > o.most {
                    o.most = w.len();
                    o.text = lines.clone();
                }
                o.last = w;
                cur = Some(o);
            }
            prev => {
                if let Some(o) = prev {
                    close(o, *at, &mut out);
                }
                if !w.is_empty() {
                    cur = Some(Open {
                        start: *at,
                        most: w.len(),
                        last: w,
                        text: lines.clone(),
                    });
                }
            }
        }
    }
    if let Some(o) = cur {
        let at = end_ms.max(o.start + 1);
        close(o, at, &mut out);
    }
    out
}

/// How much two frames' words overlap: shared / all (0 when either has
/// none).
fn jaccard(a: &std::collections::HashSet<String>, b: &std::collections::HashSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let both = a.intersection(b).count();
    both as f64 / (a.len() + b.len() - both) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(at: i64, t: &str) -> (i64, Vec<String>) {
        (at, clean(&t.lines().map(str::to_owned).collect::<Vec<_>>()))
    }

    #[test]
    fn frames_spread_over_the_video_within_the_budget() {
        assert_eq!(pick(Some(60_000), true).every_ms, 2_000);
        assert_eq!(pick(Some(30 * 60_000), true).every_ms, 12_000);
        assert_eq!(pick(Some(5 * 3_600_000), true).every_ms, 30_000);
        assert_eq!(pick(None, true).every_ms, 5_000);
        let p = pick(Some(30 * 60_000), false);
        assert_eq!(
            (p.scene_gap_ms, p.max_frames, p.keyframes_only),
            (4_000, MAX_FRAMES, false)
        );
        assert_eq!(expected(Some(30 * 60_000), &p), 150);
        assert_eq!(expected(Some(10 * 3_600_000), &p), MAX_FRAMES as usize);
        assert_eq!(expected(None, &p), 0);
    }

    #[test]
    fn noise_is_dropped() {
        let l = clean(&[
            "  a ".into(),
            "|".into(),
            " İz  1 ".into(),
            "HARRIER   JET".into(),
        ]);
        assert_eq!(l, vec!["İz 1", "HARRIER JET"]);
    }

    #[test]
    fn frames_with_the_same_text_become_one_line() {
        let lines = merge(
            &[
                f(0, ""),
                f(2_000, "HARRIER JUMP JET\nMarine Corps"),
                f(4_000, "HARRIER JUMP JET\nMarine Corps 1985"),
                f(6_000, "HARRIFR JUMP JET\nMarine Corps"),
                f(8_000, "Kadıköy vapuru"),
                f(10_000, "x"),
                f(12_000, "Son"),
            ],
            13_000,
        );
        let got: Vec<(i64, i64, &str)> = lines
            .iter()
            .map(|l| (l.start_ms, l.end_ms, l.text.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                (2_000, 8_000, "HARRIER JUMP JET\nMarine Corps 1985"),
                (8_000, 10_000, "Kadıköy vapuru"),
                (12_000, 13_000, "Son"),
            ]
        );
        assert!(merge(&[], 5_000).is_empty());
        // The last line ends after its start even when the end is unknown.
        assert_eq!(merge(&[f(5_000, "Son")], 0)[0].end_ms, 5_001);
    }
}
