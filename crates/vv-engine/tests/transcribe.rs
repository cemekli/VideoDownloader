//! STATE.md F6.2 (second half), SPEC AI-A02 with the pinned FFmpeg and the
//! fake whisper-cli (`src/bin/vv-fake-whisper.rs`): a local file becomes a
//! `generated_local` transcript with whisper's own times; silence keeps
//! nothing; a missing model or transcriber and a failing run fail the job
//! and keep nothing; cancel stops it; the staging folder is always removed.
//!
//! The last test runs the real whisper-cli (Windows CI) with the Tiny model
//! when `VV_WHISPER_TEST_MODEL` names it.

mod common;

use common::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use vv_engine::{models, transcribe};
use vv_import::import::{import_paths, FileOutcome, ImportOptions};
use vv_persistence::jobs::{self, CancelReason, JobStatus};
use vv_persistence::library::Clock;
use vv_persistence::models as model_rows;
use vv_persistence::transcripts::{self, Provenance, Status};

const MODEL: &str = "whisper-tiny";

/// An imported copy of the 3 s H.264/AAC fixture: (item, asset).
fn import_fixture(lib: &Lib) -> (String, String) {
    let media = lib.cx().tools().media.expect("media tools");
    let src = repo_root().join("tests/fixtures/media/mp4-h264-aac.mp4");
    let dir = lib.dir.path().join("Videos");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("Ders kaydı.mp4");
    std::fs::copy(src, &file).unwrap();
    let clock = lib.clock.clone();
    let r = lib.write(|c| {
        import_paths(
            c,
            clock.as_ref(),
            &media,
            std::slice::from_ref(&file),
            &ImportOptions::default(),
            &AtomicBool::new(false),
        )
        .unwrap()
    });
    match &r.files[0].outcome {
        FileOutcome::Imported {
            item_id, asset_id, ..
        } => (item_id.clone(), asset_id.clone()),
        other => panic!("{other:?}"),
    }
}

/// The Tiny model as if the user agreed and it was downloaded and checked:
/// `content` (a real model), or a sparse file of the pinned size.
fn install_model(lib: &Lib, content: Option<&Path>) -> PathBuf {
    let cat = models::catalog();
    let spec = cat.spec(cat.model(MODEL).unwrap()).unwrap();
    let dir = &lib.cx().dirs.models;
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(&spec.file_name);
    match content {
        Some(real) => {
            std::fs::copy(real, &path).unwrap();
        }
        None => {
            let f = std::fs::File::create(&path).unwrap();
            f.set_len(spec.size_bytes as u64).unwrap();
        }
    }
    let now = lib.clock.now_ms();
    lib.write(|c| {
        model_rows::consent(c, &spec, now).unwrap();
        model_rows::mark_ready(c, MODEL, now).unwrap();
    });
    path
}

fn mode(lib: &Lib, m: &str) {
    std::fs::write(lib.cx().dirs.models.join("fake-whisper-mode"), m).unwrap();
}

fn transcribe(lib: &Lib, item: &str, language: &str) -> String {
    let now = lib.clock.now_ms();
    let id = lib.write(|c| {
        transcribe::enqueue(c, item, MODEL, language, false, now)
            .unwrap()
            .id
    });
    lib.run();
    id
}

fn staging_is_clean(lib: &Lib) -> bool {
    std::fs::read_dir(&lib.cx().dirs.staging)
        .unwrap()
        .flatten()
        .all(|e| !e.file_name().to_string_lossy().starts_with("asr-"))
}

fn ready_lib() -> Option<(Lib, String, String)> {
    media_tools()?;
    let lib = library(Setup::fake());
    let (item, asset) = import_fixture(&lib);
    install_model(&lib, None);
    Some((lib, item, asset))
}

#[test]
fn a_local_file_becomes_a_generated_transcript() {
    let Some((lib, item, asset)) = ready_lib() else {
        return;
    };
    let job = transcribe(&lib, &item, "auto");
    let j = lib.job(&job);
    assert_eq!(j.status, JobStatus::Succeeded, "{:?}", j.error_code);
    let ts = lib.write(|c| transcripts::list_for_item(c, &item).unwrap());
    assert_eq!(ts.len(), 1);
    let t = &ts[0];
    assert_eq!(t.origin.provenance, Provenance::GeneratedLocal);
    assert_eq!(t.origin.provider, "whisper_cpp");
    assert_eq!(t.origin.model_id.as_deref(), Some(MODEL));
    assert_eq!(
        t.origin.model_version.as_deref(),
        models::catalog().revision.as_deref()
    );
    assert_eq!(t.origin.asset_id.as_deref(), Some(asset.as_str()));
    // `auto`: the language the model detected.
    assert_eq!(t.origin.language, "tr");
    assert_eq!(t.origin.caption_kind, None);
    assert_eq!(t.status, Status::Ready);
    assert!(t
        .origin
        .source_fingerprint
        .starts_with(&format!("asset:{asset}:sha256:")));
    assert!(t
        .origin
        .source_fingerprint
        .contains(":whisper.cpp:1.9.2:lang:auto"));
    let segs = lib.write(|c| transcripts::segments(c, &t.id).unwrap());
    // The zero-length segment is dropped; times are whisper's own.
    let texts: Vec<_> = segs.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(
        texts,
        ["Merhaba dünya, \"bu\" bir deneme.", "İkinci satır."]
    );
    let audio_ms = t.covered_ms.unwrap();
    assert!((2_500..=3_500).contains(&audio_ms), "{audio_ms}");
    assert_eq!((segs[0].start_ms, segs[1].end_ms), (0, audio_ms));
    assert!(staging_is_clean(&lib));
    // The whisper stdout text is not in the job record.
    assert!(lib.job(&job).redacted_error_message.is_none());

    // Asked again in Turkish: a separate run, the language on record.
    let again = transcribe(&lib, &item, "tr");
    assert_eq!(lib.job(&again).status, JobStatus::Succeeded);
    let ts = lib.write(|c| transcripts::list_for_item(c, &item).unwrap());
    assert_eq!(
        ts.len(),
        1,
        "same item, provenance, language and model: replaced"
    );
    assert!(ts[0].origin.source_fingerprint.ends_with(":lang:tr"));
}

#[test]
fn an_end_just_past_the_audio_is_cut_to_it() {
    let Some((lib, item, _)) = ready_lib() else {
        return;
    };
    mode(&lib, "beyond");
    let job = transcribe(&lib, &item, "en");
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
    let t = lib
        .write(|c| transcripts::list_for_item(c, &item).unwrap())
        .remove(0);
    assert_eq!(t.origin.language, "en");
    let segs = lib.write(|c| transcripts::segments(c, &t.id).unwrap());
    assert_eq!(segs.last().unwrap().end_ms, t.covered_ms.unwrap());
}

#[test]
fn lines_start_at_their_first_spoken_word() {
    // NFR-10: whisper's own segment start can be seconds off; the DTW time
    // of the first word (`-ojf -dtw tiny`) is where a line starts.
    let Some((lib, item, _)) = ready_lib() else {
        return;
    };
    mode(&lib, "late-words");
    let job = transcribe(&lib, &item, "en");
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
    let t = lib
        .write(|c| transcripts::list_for_item(c, &item).unwrap())
        .remove(0);
    let segs = lib.write(|c| transcripts::segments(c, &t.id).unwrap());
    let half = segs[1].start_ms - 400;
    assert_eq!(segs[0].start_ms, 400);
    assert_eq!(segs[0].end_ms, half);
    assert_eq!(segs[1].end_ms, t.covered_ms.unwrap());
}

#[test]
fn silence_succeeds_without_a_transcript() {
    let Some((lib, item, _)) = ready_lib() else {
        return;
    };
    mode(&lib, "silence");
    let job = transcribe(&lib, &item, "auto");
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
    assert!(lib
        .write(|c| transcripts::list_for_item(c, &item).unwrap())
        .is_empty());
    assert!(staging_is_clean(&lib));
}

#[test]
fn failures_keep_nothing() {
    let Some((lib, item, _)) = ready_lib() else {
        return;
    };
    for (m, code) in [("fail", "INTERNAL_ERROR"), ("garbage", "INTERNAL_ERROR")] {
        mode(&lib, m);
        let job = transcribe(&lib, &item, "auto");
        let j = lib.job(&job);
        assert_eq!(j.status, JobStatus::Failed, "{m}");
        assert_eq!(j.error_code.as_deref(), Some(code), "{m}");
        assert!(lib
            .write(|c| transcripts::list_for_item(c, &item).unwrap())
            .is_empty());
        assert!(staging_is_clean(&lib), "{m}");
    }
}

#[test]
fn without_a_ready_model_or_the_tool_nothing_runs() {
    let Some(_) = media_tools() else { return };
    // No model agreed to.
    let lib = library(Setup::fake());
    let (item, _) = import_fixture(&lib);
    let job = transcribe(&lib, &item, "auto");
    let j = lib.job(&job);
    assert_eq!(j.status, JobStatus::Blocked);
    assert_eq!(j.error_code.as_deref(), Some("MODEL_MISSING"));

    // A model file of the wrong size is not used.
    let path = install_model(&lib, None);
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(10)
        .unwrap();
    assert!(lib
        .write(|c| transcribe::usable_model(c, &lib.cx().dirs.models, MODEL))
        .is_none());

    // No whisper-cli (Linux, or it failed its check).
    let lib = library(Setup {
        whisper: None,
        ..Setup::fake()
    });
    let (item, _) = import_fixture(&lib);
    install_model(&lib, None);
    let job = transcribe(&lib, &item, "auto");
    assert_eq!(lib.job(&job).error_code.as_deref(), Some("TOOL_MISSING"));

    // A bookmark has no media file.
    let lib = library(Setup::fake());
    install_model(&lib, None);
    let item = lib.save("https://fake.test/ok");
    let job = transcribe(&lib, &item, "auto");
    assert_eq!(lib.job(&job).error_code.as_deref(), Some("FILE_MISSING"));
}

#[test]
fn a_file_without_audio_says_so() {
    let Some((lib, item, asset)) = ready_lib() else {
        return;
    };
    // Replace the file with a video-only copy made by the pinned FFmpeg.
    let media = lib.cx().tools().media.unwrap();
    let path = lib.write(|c| transcribe::asset_path(c, &asset).unwrap());
    let tmp = path.with_file_name("silent.mp4");
    let st = std::process::Command::new(&media.ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(&path)
        .args(["-map", "0:v:0", "-c", "copy"])
        .arg(&tmp)
        .status()
        .unwrap();
    assert!(st.success());
    std::fs::rename(&tmp, &path).unwrap();
    let job = transcribe(&lib, &item, "auto");
    let j = lib.job(&job);
    assert_eq!(j.status, JobStatus::Failed);
    assert_eq!(j.error_code.as_deref(), Some("MEDIA_CORRUPT"));
    assert_eq!(
        j.redacted_error_message.as_deref(),
        Some("the media has no audio")
    );
}

#[test]
fn cancel_stops_whisper() {
    let Some((lib, item, _)) = ready_lib() else {
        return;
    };
    mode(&lib, "slow");
    let now = lib.clock.now_ms();
    let job = lib.write(|c| {
        transcribe::enqueue(c, &item, MODEL, "auto", false, now)
            .unwrap()
            .id
    });
    std::thread::scope(|s| {
        let h = s.spawn(|| lib.run());
        for _ in 0..1000 {
            if lib.job(&job).progress.phase.as_deref() == Some("transcribe") {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let now = lib.clock.now_ms();
        lib.write(|c| jobs::cancel(c, &job, CancelReason::User, now).unwrap());
        h.join().unwrap();
    });
    assert_eq!(lib.job(&job).status, JobStatus::Canceled);
    assert!(lib
        .write(|c| transcripts::list_for_item(c, &item).unwrap())
        .is_empty());
    assert!(staging_is_clean(&lib));
}

/// The real whisper-cli with the real Tiny model (Windows CI downloads it).
/// The fixture's audio is a tone, so there may be no words; what matters is
/// that the pinned binary, its libraries and our flags and JSON reader work
/// together.
#[test]
fn real_whisper_cli_runs_with_the_tiny_model() {
    let Some(model) = std::env::var_os("VV_WHISPER_TEST_MODEL")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
    else {
        eprintln!("skipped: VV_WHISPER_TEST_MODEL not set");
        return;
    };
    let Some(paths) = real_tool_paths() else {
        return;
    };
    assert!(
        paths.whisper.is_file(),
        "VV_WHISPER_TEST_MODEL set but no whisper-cli in tools/bin"
    );
    let lib = library(Setup {
        whisper: Some(paths.whisper.clone()),
        ..Setup::fake()
    });
    let (item, _) = import_fixture(&lib);
    install_model(&lib, Some(&model));
    let job = transcribe(&lib, &item, "en");
    let j = lib.job(&job);
    assert_eq!(
        j.status,
        JobStatus::Succeeded,
        "{:?} {:?}",
        j.error_code,
        j.redacted_error_message
    );
    for t in lib.write(|c| transcripts::list_for_item(c, &item).unwrap()) {
        assert_eq!(t.origin.language, "en");
        assert!(t.covered_ms.unwrap() > 0);
    }
    assert!(staging_is_clean(&lib));
    // F6.3: the real binary reads the part WAVs the engine writes itself.
    short_parts(&lib);
    let job = transcribe(&lib, &item, "auto");
    let j = lib.job(&job);
    assert_eq!(
        j.status,
        JobStatus::Succeeded,
        "parts: {:?} {:?}",
        j.error_code,
        j.redacted_error_message
    );
    assert!(staging_is_clean(&lib));
}

// ---- F6.3: parts, checkpoint, restart and cancel (AI-A03, AI-A04, AC-23)

/// Parts of 1 s, so the 3 s fixture runs in several.
fn short_parts(lib: &Lib) {
    let mut tools = lib.cx().tools.write().unwrap();
    tools.whisper.as_mut().expect("fake whisper").chunk_ms = 1_000;
}

/// One line per whisper-cli run: "<wav ms> <language>".
fn calls(lib: &Lib) -> Vec<String> {
    std::fs::read_to_string(lib.cx().dirs.models.join("fake-whisper-calls"))
        .map(|s| s.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

fn clear_calls(lib: &Lib) {
    let _ = std::fs::remove_file(lib.cx().dirs.models.join("fake-whisper-calls"));
}

fn checkpoint(lib: &Lib, job: &str) -> Option<transcribe::Checkpoint> {
    let dir = transcribe::staging_dir(&lib.cx().dirs.staging, job);
    let raw = std::fs::read(dir.join("checkpoint.json")).ok()?;
    serde_json::from_slice(&raw).ok()
}

/// Starts the job, waits until whisper-cli runs for the second part, then
/// calls `then` while it is still running.
fn during_second_part(lib: &Lib, item: &str, then: impl FnOnce(&str)) -> String {
    let now = lib.clock.now_ms();
    let job = lib.write(|c| {
        transcribe::enqueue(c, item, MODEL, "auto", false, now)
            .unwrap()
            .id
    });
    std::thread::scope(|s| {
        let h = s.spawn(|| lib.run());
        for _ in 0..2000 {
            if calls(lib).len() >= 2 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(calls(lib).len(), 2, "the second part never started");
        then(&job);
        h.join().unwrap();
    });
    job
}

fn assert_no_repeats(segs: &[transcripts::Segment], wav_ms: i64) {
    for w in segs.windows(2) {
        assert!(w[0].end_ms <= w[1].start_ms, "overlap: {segs:?}");
    }
    assert!(segs.last().unwrap().end_ms <= wav_ms);
}

#[test]
fn a_long_recording_runs_in_parts_without_repeats() {
    let Some((lib, item, _)) = ready_lib() else {
        return;
    };
    short_parts(&lib);
    let job = transcribe(&lib, &item, "auto");
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
    let runs = calls(&lib);
    assert!(runs.len() >= 2, "{runs:?}");
    // The first part detects the language; the rest are told it.
    assert!(runs[0].ends_with(" auto"), "{runs:?}");
    assert!(runs[1..].iter().all(|r| r.ends_with(" tr")), "{runs:?}");
    let ts = lib.write(|c| transcripts::list_for_item(c, &item).unwrap());
    assert_eq!(ts.len(), 1);
    assert_eq!(ts[0].status, Status::Ready);
    assert_eq!(ts[0].origin.language, "tr");
    let segs = lib.write(|c| transcripts::segments(c, &ts[0].id).unwrap());
    // Two segments per part (the zero-length one is dropped), in order.
    assert_eq!(segs.len(), 2 * runs.len());
    assert_eq!(segs[0].start_ms, 0);
    assert_no_repeats(&segs, ts[0].covered_ms.unwrap());
    // The parts' lengths add up to the recording.
    let total: i64 = runs
        .iter()
        .map(|r| r.split(' ').next().unwrap().parse::<i64>().unwrap())
        .sum();
    assert!((total - ts[0].covered_ms.unwrap()).abs() <= runs.len() as i64);
    assert!(staging_is_clean(&lib));
}

#[test]
fn a_restart_continues_after_the_last_finished_part() {
    let Some((lib, item, _)) = ready_lib() else {
        return;
    };
    short_parts(&lib);
    mode(&lib, "slow-after-1");
    let job = during_second_part(&lib, &item, |_| {
        // The app is closed mid-part and opened again (§13.2).
        mode(&lib, "ok");
        let now = lib.clock.now_ms();
        lib.write(|c| jobs::recover(c, now).unwrap());
    });
    assert_eq!(lib.job(&job).status, JobStatus::Queued);
    let cp = checkpoint(&lib, &job).expect("checkpoint kept for the next run");
    assert_eq!(cp.done, 1);
    let parts = cp.chunks.len();
    assert!(parts >= 2);
    // The first part's text is there, labelled partial.
    let ts = lib.write(|c| transcripts::list_for_item(c, &item).unwrap());
    assert_eq!(ts.len(), 1);
    assert_eq!(ts[0].status, Status::Partial);
    assert_eq!(ts[0].covered_ms, Some(cp.chunks[0].1));
    // Next start: only the parts not done run.
    lib.run();
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
    assert_eq!(calls(&lib).len(), 2 + (parts - 1));
    let ts = lib.write(|c| transcripts::list_for_item(c, &item).unwrap());
    assert_eq!(ts.len(), 1);
    assert_eq!(ts[0].status, Status::Ready);
    let segs = lib.write(|c| transcripts::segments(c, &ts[0].id).unwrap());
    assert_eq!(segs.len(), 2 * parts, "no part twice, none missing");
    assert_no_repeats(&segs, ts[0].covered_ms.unwrap());
    assert!(staging_is_clean(&lib));
}

#[test]
fn a_checkpoint_from_other_settings_is_not_used() {
    let Some((lib, item, _)) = ready_lib() else {
        return;
    };
    short_parts(&lib);
    let now = lib.clock.now_ms();
    let job = lib.write(|c| {
        transcribe::enqueue(c, &item, MODEL, "auto", false, now)
            .unwrap()
            .id
    });
    // Left by a run with another model: says one part is done.
    let dir = transcribe::staging_dir(&lib.cx().dirs.staging, &job);
    std::fs::create_dir_all(&dir).unwrap();
    let stale = transcribe::Checkpoint {
        version: transcribe::TRANSCRIBE_CONFIG_VERSION,
        fingerprint: "asset:x:model:other".into(),
        wav_ms: 3000,
        chunks: vec![(0, 1500), (1500, 3000)],
        done: 1,
        language: Some("en".into()),
        segments: vec![(0, 1000, "stale".into())],
    };
    std::fs::write(
        dir.join("checkpoint.json"),
        serde_json::to_vec(&stale).unwrap(),
    )
    .unwrap();
    lib.run();
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
    let runs = calls(&lib);
    assert!(runs[0].ends_with(" auto"), "started over: {runs:?}");
    let t = lib
        .write(|c| transcripts::list_for_item(c, &item).unwrap())
        .remove(0);
    let segs = lib.write(|c| transcripts::segments(c, &t.id).unwrap());
    assert!(segs.iter().all(|s| s.text != "stale"));
    assert_eq!(segs.len(), 2 * runs.len());
}

#[test]
fn cancel_keeps_the_text_of_finished_parts_as_partial() {
    let Some((lib, item, _)) = ready_lib() else {
        return;
    };
    short_parts(&lib);
    mode(&lib, "slow-after-1");
    let job = during_second_part(&lib, &item, |job| {
        let now = lib.clock.now_ms();
        lib.write(|c| jobs::cancel(c, job, CancelReason::User, now).unwrap());
    });
    assert_eq!(lib.job(&job).status, JobStatus::Canceled);
    let ts = lib.write(|c| transcripts::list_for_item(c, &item).unwrap());
    assert_eq!(ts.len(), 1);
    assert_eq!(ts[0].status, Status::Partial);
    let covered = ts[0].covered_ms.unwrap();
    let segs = lib.write(|c| transcripts::segments(c, &ts[0].id).unwrap());
    assert_eq!(segs.len(), 2);
    assert!(segs.iter().all(|s| s.end_ms <= covered));
    assert!(staging_is_clean(&lib));
}

#[test]
fn a_failing_part_keeps_the_partial_and_cleans_up() {
    let Some((lib, item, _)) = ready_lib() else {
        return;
    };
    short_parts(&lib);
    mode(&lib, "fail-after-1");
    let job = transcribe(&lib, &item, "auto");
    let j = lib.job(&job);
    assert_eq!(j.status, JobStatus::Failed);
    assert_eq!(j.error_code.as_deref(), Some("INTERNAL_ERROR"));
    let ts = lib.write(|c| transcripts::list_for_item(c, &item).unwrap());
    assert_eq!(ts.len(), 1);
    assert_eq!(ts[0].status, Status::Partial);
    assert!(staging_is_clean(&lib));
}

#[test]
fn a_rerun_never_hides_a_finished_transcript_behind_a_partial() {
    let Some((lib, item, _)) = ready_lib() else {
        return;
    };
    short_parts(&lib);
    transcribe(&lib, &item, "auto");
    let before = lib.write(|c| transcripts::list_for_item(c, &item).unwrap());
    assert_eq!(before[0].status, Status::Ready);
    let n = lib
        .write(|c| transcripts::segments(c, &before[0].id).unwrap())
        .len();
    clear_calls(&lib);
    mode(&lib, "slow-after-1");
    let job = during_second_part(&lib, &item, |job| {
        let now = lib.clock.now_ms();
        lib.write(|c| jobs::cancel(c, job, CancelReason::User, now).unwrap());
    });
    assert_eq!(lib.job(&job).status, JobStatus::Canceled);
    let after = lib.write(|c| transcripts::list_for_item(c, &item).unwrap());
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].status, Status::Ready);
    assert_eq!(
        lib.write(|c| transcripts::segments(c, &after[0].id).unwrap())
            .len(),
        n
    );
}
