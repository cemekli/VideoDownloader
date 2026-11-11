//! STATE.md F6.2 with the fake yt-dlp: the site's captions become a
//! `source_captions` transcript; translations are never taken; broken or
//! missing files fail the job and keep nothing; no caption URL is stored.

mod common;

use common::*;
use vv_engine::captions;
use vv_persistence::jobs::JobStatus;
use vv_persistence::library::Clock;
use vv_persistence::transcripts::{self, CaptionKind, Provenance, Status};

fn fetch(lib: &Lib, item: &str) -> String {
    let now = lib.clock.now_ms();
    let id = lib.write(|c| captions::enqueue(c, item, now).unwrap().id);
    lib.run();
    id
}

fn texts(lib: &Lib, transcript: &str) -> Vec<(i64, i64, String)> {
    lib.write(|c| transcripts::segments(c, transcript).unwrap())
        .into_iter()
        .map(|s| (s.start_ms, s.end_ms, s.text))
        .collect()
}

fn staging_is_empty(lib: &Lib) -> bool {
    std::fs::read_dir(&lib.cx().dirs.staging)
        .unwrap()
        .flatten()
        .all(|e| !e.file_name().to_string_lossy().starts_with("caps-"))
}

#[test]
fn manual_captions_in_the_spoken_language_become_a_transcript() {
    let lib = library(Setup::fake());
    let item = lib.save("https://fake.test/captions-manual");
    let job = fetch(&lib, &item);
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
    let ts = lib.write(|c| transcripts::list_for_item(c, &item).unwrap());
    assert_eq!(ts.len(), 1);
    let t = &ts[0];
    assert_eq!(t.origin.provenance, Provenance::SourceCaptions);
    assert_eq!(t.origin.provider, "source_captions");
    assert_eq!(t.origin.language, "tr");
    assert_eq!(t.origin.caption_kind, Some(CaptionKind::Manual));
    assert_eq!(t.status, Status::Ready);
    assert_eq!(t.covered_ms, Some(2_900));
    assert!(t.origin.source_fingerprint.starts_with("tr:manual:sha256:"));
    assert_eq!(
        texts(&lib, &t.id),
        vec![
            (0, 1_200, "Merhaba dünya".to_owned()),
            (1_200, 2_900, "İkinci satır & son".to_owned())
        ]
    );
    // Caption URLs (signed) never reach the database.
    let dump = lib.dump();
    assert!(!dump.contains(CANARY));
    assert!(!dump.contains("cdn.example.com"));
    assert!(staging_is_empty(&lib));

    // Fetching again replaces the same transcript.
    lib.clock.advance(1_000);
    let again = fetch(&lib, &item);
    assert_ne!(again, job);
    assert_eq!(lib.job(&again).status, JobStatus::Succeeded);
    let ts2 = lib.write(|c| transcripts::list_for_item(c, &item).unwrap());
    assert_eq!(ts2.len(), 1);
    assert_eq!(ts2[0].id, t.id);
    assert_eq!(texts(&lib, &t.id).len(), 2);
}

#[test]
fn automatic_original_track_is_folded_and_labelled() {
    let lib = library(Setup::fake());
    let item = lib.save("https://fake.test/captions-auto");
    let job = fetch(&lib, &item);
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
    let ts = lib.write(|c| transcripts::list_for_item(c, &item).unwrap());
    assert_eq!(ts.len(), 1);
    assert_eq!(ts[0].origin.language, "en");
    assert_eq!(ts[0].origin.caption_kind, Some(CaptionKind::Automatic));
    assert_eq!(
        texts(&lib, &ts[0].id),
        vec![
            (100, 1_000, "hello world".to_owned()),
            (1_010, 2_500, "this is fine".to_owned())
        ]
    );
}

#[test]
fn only_translations_means_no_transcript_and_a_finished_job() {
    let lib = library(Setup::fake());
    let item = lib.save("https://fake.test/captions-none");
    let job = fetch(&lib, &item);
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
    assert!(lib
        .write(|c| transcripts::list_for_item(c, &item).unwrap())
        .is_empty());
    // A site without any tracks too.
    let plain = lib.save("https://fake.test/video");
    assert_eq!(lib.job(&fetch(&lib, &plain)).status, JobStatus::Succeeded);
}

#[test]
fn cues_past_the_media_end_fail_the_job_and_keep_nothing() {
    let lib = library(Setup::fake());
    let item = lib.save("https://fake.test/captions-bad");
    let job = lib.job(&fetch(&lib, &item));
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error_code.as_deref(), Some("MEDIA_CORRUPT"));
    assert!(lib
        .write(|c| transcripts::list_for_item(c, &item).unwrap())
        .is_empty());
    assert!(staging_is_empty(&lib));
}

#[test]
fn a_missing_caption_file_fails_the_job() {
    let lib = library(Setup::fake());
    let item = lib.save("https://fake.test/captions-missing");
    let job = lib.job(&fetch(&lib, &item));
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error_code.as_deref(), Some("EXTRACTOR_FAILED"));
}
