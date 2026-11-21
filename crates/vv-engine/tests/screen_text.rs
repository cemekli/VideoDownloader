//! STATE.md F7.4, SPEC AI-C01…C04, AC-41 with the pinned FFmpeg: the
//! frames of a local video with text on screen are picked (never all of
//! them), read by the OCR (a fake here; Windows' own in the last test, on
//! Windows), merged into lines with their times, stored and found by words
//! search apart from speech. A run for all videos skips what is current;
//! cancel keeps what was read; the staging folder is removed.

mod common;

use common::*;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use vv_engine::screen_text;
use vv_import::import::{import_paths, FileOutcome, ImportOptions};
use vv_persistence::jobs::{self, CancelReason, JobStatus};
use vv_persistence::library::search::{self, Field, SearchQuery, Sort, SortField};
use vv_persistence::library::Clock;
use vv_persistence::screen_text::{self as rows, RunStatus};

/// 12 s, 10 fps, a keyframe every 2 s: "KADIKOY VAPURU" 0–4 s, "HARRIER
/// JUMP JET" 4–9 s, nothing 9–12 s.
const SLIDES: &str = "tests/fixtures/ocr/slides.mp4";

/// What the fake reads, frame by frame: a short video is read every
/// second (0 … 11 s).
const SCRIPT: &[&str] = &[
    "KADIKOY VAPURU",
    "KADIKOY VAPURU",
    "KADIKOY VAPURU",
    "KADIKOY VAPURU",
    "HARRIER JUMP JET",
    "HARRIER JUMP JET",
    "HARRIER JUMP JET",
    "HARRIER JUMP JET",
    "HARRIER JUMP JET",
    "",
    "",
    "",
];

fn import(lib: &Lib, name: &str) -> String {
    let media = lib.cx().tools().media.expect("media tools");
    let dir = lib.dir.path().join("Videos");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(name);
    std::fs::copy(repo_root().join(SLIDES), &file).unwrap();
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
        FileOutcome::Imported { item_id, .. } => item_id.clone(),
        other => panic!("{other:?}"),
    }
}

fn lib_with(ocr: Arc<dyn vv_ocr::Ocr>) -> Option<Lib> {
    media_tools()?;
    Some(library(Setup {
        ocr,
        ..Setup::fake()
    }))
}

fn read(lib: &Lib, item: Option<&str>) -> String {
    let now = lib.clock.now_ms();
    let id = lib.write(|c| screen_text::request(c, item, now).unwrap().id);
    lib.run();
    id
}

fn staging_is_clean(lib: &Lib) -> bool {
    std::fs::read_dir(&lib.cx().dirs.staging)
        .unwrap()
        .flatten()
        .all(|e| !e.file_name().to_string_lossy().starts_with("ocr-"))
}

fn find(lib: &Lib, text: &str) -> search::SearchPage {
    lib.write(|c| {
        search::search(
            c,
            &SearchQuery {
                text: text.into(),
                sort: Sort {
                    field: SortField::Relevance,
                    descending: false,
                },
                ..SearchQuery::default()
            },
        )
        .unwrap()
    })
}

#[test]
fn text_on_screen_in_a_video_is_found_with_its_time() {
    let fake = vv_ocr::Fake::new("tr", SCRIPT);
    let Some(lib) = lib_with(fake_ocr(fake.clone())) else {
        return;
    };
    let item = import(&lib, "Hava gösterisi.mp4");
    let job = read(&lib, None);
    let j = lib.job(&job);
    assert_eq!(j.status, JobStatus::Succeeded, "{:?}", j.error_code);
    assert_eq!(
        (j.progress.units_done, j.progress.units_total),
        (Some(1), Some(1))
    );
    // A frame a second, never all 120 (AI-C01).
    assert_eq!(fake.calls(), 12);
    let run = lib.write(|c| rows::get(c, &item).unwrap().unwrap());
    assert_eq!(
        (run.status, run.frames, run.chunks),
        (RunStatus::Ready, 12, 2)
    );
    assert_eq!(
        (run.provider.as_str(), run.language.as_str()),
        ("test", "tr")
    );
    let lines = lib.write(|c| rows::lines(c, &item).unwrap());
    let got: Vec<_> = lines
        .iter()
        .map(|l| (l.start_ms, l.end_ms, l.text.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            (0, 4_000, "KADIKOY VAPURU"),
            (4_000, 9_000, "HARRIER JUMP JET")
        ]
    );
    // AC-41: found by words, with the frame time, labelled as on screen.
    let p = find(&lib, "harrier");
    assert_eq!(p.hits.len(), 1);
    assert_eq!(p.hits[0].matched, [Field::OnScreen]);
    assert!(p.hits[0].transcript.is_none());
    let s = p.hits[0].on_screen.as_ref().unwrap();
    assert_eq!(s.moments[0].start_ms, 4_000);
    assert!(staging_is_clean(&lib));

    // Current: a second run for all videos reads nothing.
    let again = read(&lib, None);
    assert_eq!(lib.job(&again).status, JobStatus::Succeeded);
    assert_eq!(lib.job(&again).progress.units_total, Some(0));
    assert_eq!(fake.calls(), 12);
    // Asked for this item: read again (past the script: no text now).
    read(&lib, Some(&item));
    assert_eq!(fake.calls(), 24);
    assert!(find(&lib, "harrier").hits.is_empty());
    let run = lib.write(|c| rows::get(c, &item).unwrap().unwrap());
    assert_eq!((run.status, run.chunks), (RunStatus::Ready, 0));
}

#[test]
fn without_an_ocr_language_the_run_fails_and_reads_nothing() {
    let Some(lib) = lib_with(fake_ocr(vv_ocr::Fake::unavailable())) else {
        return;
    };
    let item = import(&lib, "a.mp4");
    let job = read(&lib, None);
    let j = lib.job(&job);
    assert_eq!(j.status, JobStatus::Failed);
    assert_eq!(j.error_code.as_deref(), Some("MODEL_MISSING"));
    assert!(lib.write(|c| rows::get(c, &item).unwrap()).is_none());
}

#[test]
fn a_missing_file_is_recorded_and_not_tried_again() {
    let fake = vv_ocr::Fake::new("tr", SCRIPT);
    let Some(lib) = lib_with(fake_ocr(fake.clone())) else {
        return;
    };
    let gone = import(&lib, "gone.mp4");
    let kept = import(&lib, "kept.mp4");
    std::fs::remove_file(lib.dir.path().join("Videos/gone.mp4")).unwrap();
    let job = read(&lib, None);
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
    let r = lib.write(|c| rows::get(c, &gone).unwrap().unwrap());
    assert_eq!(
        (r.status, r.error_code.as_deref()),
        (RunStatus::Failed, Some("FILE_MISSING"))
    );
    assert_eq!(
        lib.write(|c| rows::get(c, &kept).unwrap().unwrap()).status,
        RunStatus::Ready
    );
    assert!(lib.write(|c| screen_text::pending(c)).is_empty());
    // One item alone: its failure is the job's.
    let one = read(&lib, Some(&gone));
    assert_eq!(lib.job(&one).error_code.as_deref(), Some("FILE_MISSING"));
}

#[test]
fn cancel_before_the_run_starts_reads_nothing() {
    let fake = vv_ocr::Fake::new("tr", SCRIPT);
    let Some(lib) = lib_with(fake_ocr(fake.clone())) else {
        return;
    };
    let item = import(&lib, "a.mp4");
    let now = lib.clock.now_ms();
    let id = lib.write(|c| {
        let id = screen_text::request(c, None, now).unwrap().id;
        // One open run for all videos at a time.
        assert!(screen_text::request(c, None, now).unwrap().existing);
        jobs::cancel(c, &id, CancelReason::User, now).unwrap();
        id
    });
    lib.run();
    assert_eq!(lib.job(&id).status, JobStatus::Canceled);
    assert_eq!(fake.calls(), 0);
    assert!(lib.write(|c| rows::get(c, &item).unwrap()).is_none());
    assert_eq!(lib.write(|c| screen_text::pending(c)), [item]);
}

/// Windows' own OCR on the fixture video, where an OCR language is
/// installed (Windows 10/11 desktops have the user's; a server image may
/// not, which is printed).
#[cfg(windows)]
#[test]
fn windows_reads_the_text_of_the_fixture_video() {
    if let Err(e) = vv_ocr::system() {
        eprintln!("skipped: {e} ({:?})", vv_ocr::available_languages());
        return;
    }
    let Some(lib) = lib_with(Arc::new(vv_ocr::SystemOcr)) else {
        return;
    };
    let item = import(&lib, "slides.mp4");
    let job = read(&lib, None);
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
    let lines = lib.write(|c| rows::lines(c, &item).unwrap());
    let harrier = lines
        .iter()
        .find(|l| l.text.to_uppercase().contains("HARRIER"))
        .unwrap_or_else(|| panic!("{lines:?}"));
    assert_eq!(harrier.start_ms, 4_000, "{lines:?}");
    assert_eq!(find(&lib, "harrier").hits.len(), 1);
}
