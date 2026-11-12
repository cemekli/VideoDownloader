//! STATE.md F6.6, SPEC AI-A05: on a PC with too little memory the model is
//! not loaded silently. A transcription without the user's "use it anyway"
//! fails with `RESOURCE_LIMIT` before FFmpeg or whisper start and keeps
//! nothing; with it, the job runs.
//!
//! This file is its own test binary so the fake memory (debug builds read
//! `VV_FAKE_MEMORY_MB`) does not reach the other tests.

mod common;

use common::*;
use std::sync::atomic::AtomicBool;
use vv_domain::error::ErrorCode;
use vv_engine::{models, transcribe};
use vv_import::import::{import_paths, FileOutcome, ImportOptions};
use vv_persistence::jobs::JobStatus;
use vv_persistence::library::Clock;
use vv_persistence::models as model_rows;
use vv_persistence::transcripts;

const MODEL: &str = "whisper-tiny";

/// A 1 GB PC with 512 MB free: even Tiny (about 470 MB) does not fit next
/// to the system.
fn low_memory_pc() {
    std::env::set_var("VV_FAKE_MEMORY_MB", "1024,512");
}

fn ready_lib() -> Option<(Lib, String)> {
    media_tools()?;
    let lib = library(Setup::fake());
    let media = lib.cx().tools().media.expect("media tools");
    let src = repo_root().join("tests/fixtures/media/mp4-h264-aac.mp4");
    let dir = lib.dir.path().join("Videos");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("lecture.mp4");
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
    let item = match &r.files[0].outcome {
        FileOutcome::Imported { item_id, .. } => item_id.clone(),
        other => panic!("{other:?}"),
    };
    let cat = models::catalog();
    let spec = cat.spec(cat.model(MODEL).unwrap()).unwrap();
    let models_dir = &lib.cx().dirs.models;
    std::fs::create_dir_all(models_dir).unwrap();
    std::fs::File::create(models_dir.join(&spec.file_name))
        .unwrap()
        .set_len(spec.size_bytes as u64)
        .unwrap();
    let now = lib.clock.now_ms();
    lib.write(|c| {
        model_rows::consent(c, &spec, now).unwrap();
        model_rows::mark_ready(c, MODEL, now).unwrap();
    });
    Some((lib, item))
}

fn run(lib: &Lib, item: &str, allow: bool) -> vv_persistence::jobs::Job {
    let now = lib.clock.now_ms();
    let id = lib.write(|c| {
        transcribe::enqueue(c, item, MODEL, "auto", allow, now)
            .unwrap()
            .id
    });
    lib.run();
    lib.job(&id)
}

#[test]
fn a_model_too_big_for_this_pc_is_not_loaded_without_consent() {
    low_memory_pc();
    let Some((lib, item)) = ready_lib() else {
        return;
    };
    assert_eq!(
        transcribe::model_fit(MODEL, vv_engine::resources::memory()).map(|f| f.1),
        Some(vv_engine::resources::Fit::TooBig)
    );
    let j = run(&lib, &item, false);
    // SPEC §19: RESOURCE_LIMIT waits for the user (blocked, "model").
    assert_eq!(j.status, JobStatus::Blocked);
    assert_eq!(
        j.error_code.as_deref(),
        Some(ErrorCode::ResourceLimit.as_str())
    );
    assert!(lib
        .write(|c| transcripts::list_for_item(c, &item))
        .unwrap()
        .is_empty());
    // Nothing was started: no audio was written.
    assert!(std::fs::read_dir(&lib.cx().dirs.staging)
        .map(|d| d
            .flatten()
            .all(|e| !e.file_name().to_string_lossy().starts_with("asr-")))
        .unwrap_or(true));
}

#[test]
fn use_it_anyway_runs_the_model() {
    low_memory_pc();
    let Some((lib, item)) = ready_lib() else {
        return;
    };
    let j = run(&lib, &item, true);
    assert_eq!(j.status, JobStatus::Succeeded, "{:?}", j.error_code);
    assert_eq!(
        lib.write(|c| transcripts::list_for_item(c, &item))
            .unwrap()
            .len(),
        1
    );
}
