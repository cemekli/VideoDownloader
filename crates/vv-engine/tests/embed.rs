//! STATE.md F7.1, SPEC §16.3, AC-24: the embedding index jobs with the fake
//! `llama-server` (src/bin/vv-fake-embed.rs): build, search by meaning,
//! fill after new transcripts, reindex beside the active index, cancel and
//! pause, a missing model or tool.
//!
//! The last test runs the real `llama-server` (Windows CI) with the real
//! model when `VV_EMBED_TEST_MODEL` names it: a Turkish query finds the
//! passage that says the same thing in other words (DEC-12, ADR-0035).

mod common;

use common::*;
use std::time::Duration;
use vv_engine::{embed, models};
use vv_persistence::jobs::{self, CancelReason, JobKind, JobStatus};
use vv_persistence::library::Clock;
use vv_persistence::models as model_rows;
use vv_persistence::transcripts::{self, CaptionKind, NewSegment, Origin, Provenance, Status};
use vv_persistence::vectors::{self, GenerationStatus};

const MODEL: &str = "embed-bge-m3";

fn install_model(lib: &Lib) {
    install_model_from(lib, None);
}

/// The model as if the user agreed and it was downloaded and checked:
/// `real` (the pinned file), or a sparse file of the pinned size.
fn install_model_from(lib: &Lib, real: Option<&std::path::Path>) {
    let cat = models::embedding_catalog();
    let spec = cat.spec(cat.model(MODEL).unwrap()).unwrap();
    let dir = &lib.cx().dirs.models;
    std::fs::create_dir_all(dir).unwrap();
    match real {
        Some(src) => {
            std::fs::copy(src, dir.join(&spec.file_name)).unwrap();
        }
        None => {
            let f = std::fs::File::create(dir.join(&spec.file_name)).unwrap();
            f.set_len(spec.size_bytes as u64).unwrap();
        }
    }
    let now = lib.clock.now_ms();
    lib.write(|c| {
        model_rows::consent(c, &spec, now).unwrap();
        model_rows::mark_ready(c, MODEL, now).unwrap();
    });
}

fn mode(lib: &Lib, m: &str) {
    std::fs::write(lib.cx().dirs.models.join("fake-embed-mode"), m).unwrap();
}

fn calls(lib: &Lib) -> Vec<usize> {
    std::fs::read_to_string(lib.cx().dirs.models.join("fake-embed-calls"))
        .unwrap_or_default()
        .lines()
        .map(|l| l.parse().unwrap())
        .collect()
}

fn starts(lib: &Lib) -> usize {
    std::fs::read_to_string(lib.cx().dirs.models.join("fake-embed-starts"))
        .unwrap_or_default()
        .lines()
        .count()
}

/// A bookmark with a caption transcript of `lines` (one chunk each, 40 s apart).
fn with_transcript(lib: &Lib, url: &str, lines: &[&str]) -> String {
    let item = lib.save(url);
    let segs: Vec<NewSegment> = lines
        .iter()
        .enumerate()
        .map(|(i, t)| NewSegment::new(i as i64 * 40_000, i as i64 * 40_000 + 3_000, *t))
        .collect();
    let o = Origin {
        item_id: item.clone(),
        asset_id: None,
        language: "tr".into(),
        provenance: Provenance::SourceCaptions,
        provider: "source_captions".into(),
        model_id: None,
        model_version: None,
        source_fingerprint: "yt-dlp:subtitles:tr".into(),
        caption_kind: Some(CaptionKind::Manual),
    };
    let now = lib.clock.now_ms();
    lib.write(|c| {
        let tx = c.transaction().unwrap();
        transcripts::save(&tx, &o, Status::Ready, None, &segs, None, now).unwrap();
        tx.commit().unwrap();
    });
    item
}

fn request(lib: &Lib, rebuild: bool) -> Option<String> {
    let now = lib.clock.now_ms();
    let dir = lib.cx().dirs.models.clone();
    lib.write(|c| embed::request_index(c, &dir, rebuild, now))
        .unwrap()
        .map(|q| q.id)
}

fn staging_is_clean(lib: &Lib) -> bool {
    std::fs::read_dir(&lib.cx().dirs.staging)
        .unwrap()
        .flatten()
        .all(|e| !e.file_name().to_string_lossy().starts_with("embed-"))
}

/// The text of the best chunk for `query`, through the real path: the
/// fake tool embeds the query, the repository searches the active index.
fn best(lib: &Lib, query: &str) -> String {
    let tool = lib.cx().tools().embedder.unwrap();
    let m = models::embedding_catalog().model(MODEL).unwrap();
    let u = m.embedding.clone().unwrap();
    let path = lib.cx().dirs.models.join(&m.file);
    let dir = tempfile::tempdir().unwrap();
    let q = embed::embed(
        &tool,
        &path,
        &u,
        embed::Role::Query,
        &[query],
        1,
        dir.path(),
        || false,
    )
    .unwrap()
    .remove(0);
    lib.write(|c| {
        let g = vectors::active(c).unwrap().unwrap();
        let hit = vectors::search(c, &g, &q, 1).unwrap().remove(0);
        c.query_row(
            "SELECT text FROM search_chunk WHERE seq = ?1",
            [hit.chunk_seq],
            |r| r.get(0),
        )
        .unwrap()
    })
}

#[test]
fn no_model_means_no_index_and_lexical_search_is_untouched() {
    let lib = library(Setup::fake());
    with_transcript(&lib, "https://fake.test/a", &["merhaba dünya"]);
    let dir = lib.cx().dirs.models.clone();
    let now = lib.clock.now_ms();
    assert_eq!(
        lib.write(|c| embed::request_index(c, &dir, false, now)),
        Err(embed::IndexError::NoModel)
    );
    assert!(lib
        .write(|c| embed::after_chunks_changed(c, &dir, now))
        .is_none());
    assert!(lib.write(|c| vectors::active(c).unwrap()).is_none());
}

#[test]
fn build_finds_a_topic_said_in_other_words() {
    let lib = library(Setup::fake());
    install_model(&lib);
    with_transcript(
        &lib,
        "https://fake.test/a",
        &[
            "bugün PostgreSQL replikasyonu anlatıyoruz",
            "ekşi maya ile ekmek",
        ],
    );
    with_transcript(
        &lib,
        "https://fake.test/b",
        &["maraton antrenmanı için koşu"],
    );
    let job = request(&lib, false).unwrap();
    assert_eq!(lib.job(&job).kind, JobKind::Reindex);
    // Nothing answers searches before the build is complete.
    assert!(lib.write(|c| vectors::active(c).unwrap()).is_none());
    lib.run();
    let j = lib.job(&job);
    assert_eq!(j.status, JobStatus::Succeeded, "{j:?}");
    assert_eq!(j.progress.units_done, Some(3));
    assert_eq!(j.progress.units_total, Some(3));
    let g = lib.write(|c| vectors::active(c).unwrap()).unwrap();
    assert_eq!(g.producer.model_id, MODEL);
    assert_eq!(g.producer.dims, 1024);
    assert_eq!(lib.write(|c| vectors::coverage(c, g.seq).unwrap()), (3, 3));
    // One request for the three chunks (a batch), one server for the job.
    assert_eq!(calls(&lib), vec![3]);
    assert_eq!(starts(&lib), 1);
    assert_eq!(
        best(&lib, "veritabanı yedekleme"),
        "bugün PostgreSQL replikasyonu anlatıyoruz"
    );
    assert_eq!(best(&lib, "how to bake bread"), "ekşi maya ile ekmek");
    assert!(staging_is_clean(&lib));
    // Up to date: nothing to queue.
    assert!(request(&lib, false).is_none());
}

#[test]
fn new_transcripts_are_added_to_the_active_index() {
    let lib = library(Setup::fake());
    install_model(&lib);
    with_transcript(&lib, "https://fake.test/a", &["ekşi maya ile ekmek"]);
    request(&lib, false).unwrap();
    lib.run();
    with_transcript(&lib, "https://fake.test/b", &["PostgreSQL replikasyonu"]);
    let dir = lib.cx().dirs.models.clone();
    let now = lib.clock.now_ms();
    let q = lib
        .write(|c| embed::after_chunks_changed(c, &dir, now))
        .unwrap();
    assert_eq!(lib.job(&q.id).kind, JobKind::Embedding);
    // Asking again while it is open gives the same job.
    let again = lib
        .write(|c| embed::after_chunks_changed(c, &dir, now))
        .unwrap();
    assert_eq!(again.id, q.id);
    lib.run();
    assert_eq!(lib.job(&q.id).status, JobStatus::Succeeded);
    let g = lib.write(|c| vectors::active(c).unwrap()).unwrap();
    assert_eq!(lib.write(|c| vectors::coverage(c, g.seq).unwrap()), (2, 2));
    // Only the new chunk was embedded.
    assert_eq!(calls(&lib), vec![1, 1]);
    assert_eq!(best(&lib, "database backup"), "PostgreSQL replikasyonu");
}

#[test]
fn reindex_runs_beside_the_active_index_and_cancel_keeps_it() {
    // AC-24.
    let lib = library(Setup::fake());
    install_model(&lib);
    for i in 0..3 {
        with_transcript(
            &lib,
            &format!("https://fake.test/{i}"),
            &(0..40)
                .map(|_| "PostgreSQL replikasyonu")
                .collect::<Vec<_>>(),
        );
    }
    request(&lib, false).unwrap();
    lib.run();
    let first = lib.write(|c| vectors::active(c).unwrap()).unwrap();
    assert_eq!(
        lib.write(|c| vectors::coverage(c, first.seq).unwrap()),
        (120, 120)
    );
    // Four batches, one server: the model is loaded once per job.
    assert_eq!(calls(&lib), vec![32, 32, 32, 24]);
    assert_eq!(starts(&lib), 1);

    mode(&lib, "slow-after-1");
    std::fs::remove_file(lib.cx().dirs.models.join("fake-embed-calls")).unwrap();
    let job = request(&lib, true).unwrap();
    assert_eq!(lib.job(&job).kind, JobKind::Reindex);
    std::thread::scope(|s| {
        let h = s.spawn(|| lib.run());
        // Wait until the first batch is stored and the second run is under way.
        for _ in 0..1000 {
            if calls(&lib).len() >= 2 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let b = lib.write(|c| vectors::building(c).unwrap()).unwrap();
        assert_eq!(lib.write(|c| vectors::coverage(c, b.seq).unwrap()).0, 32);
        // Searches still go to the first index, complete.
        assert_eq!(
            lib.write(|c| vectors::active(c).unwrap()).unwrap().id,
            first.id
        );
        let now = lib.clock.now_ms();
        lib.write(|c| jobs::cancel(c, &job, CancelReason::User, now).unwrap());
        h.join().unwrap();
    });
    assert_eq!(lib.job(&job).status, JobStatus::Canceled);
    assert!(lib.write(|c| vectors::building(c).unwrap()).is_none());
    let still = lib.write(|c| vectors::active(c).unwrap()).unwrap();
    assert_eq!(still.id, first.id);
    assert_eq!(
        lib.write(|c| vectors::coverage(c, still.seq).unwrap()),
        (120, 120)
    );
    let n: i64 = lib.write(|c| {
        c.query_row("SELECT count(*) FROM chunk_embedding", [], |r| r.get(0))
            .unwrap()
    });
    assert_eq!(n, 120, "the canceled build's vectors are gone");
    assert!(staging_is_clean(&lib));

    // A completed rebuild replaces it.
    mode(&lib, "ok");
    let job = request(&lib, true).unwrap();
    lib.run();
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
    let g = lib.write(|c| vectors::active(c).unwrap()).unwrap();
    assert_ne!(g.id, first.id);
    assert_eq!(g.status, GenerationStatus::Active);
    let n: i64 = lib.write(|c| {
        c.query_row("SELECT count(*) FROM chunk_embedding", [], |r| r.get(0))
            .unwrap()
    });
    assert_eq!(n, 120);
}

#[test]
fn cancel_before_it_runs_drops_the_build() {
    let lib = library(Setup::fake());
    install_model(&lib);
    with_transcript(&lib, "https://fake.test/a", &["x"]);
    request(&lib, false).unwrap();
    assert!(lib.write(|c| vectors::building(c).unwrap()).is_some());
    let now = lib.clock.now_ms();
    assert!(lib.write(|c| embed::cancel_build(c, now)));
    assert!(lib.write(|c| vectors::building(c).unwrap()).is_none());
    lib.run();
    assert!(lib.write(|c| vectors::active(c).unwrap()).is_none());
}

#[test]
fn a_failing_tool_or_a_missing_tool_fails_the_job_and_keeps_no_half_index_active() {
    let lib = library(Setup::fake());
    install_model(&lib);
    with_transcript(&lib, "https://fake.test/a", &["x"]);
    mode(&lib, "short");
    let job = request(&lib, false).unwrap();
    lib.run();
    let j = lib.job(&job);
    assert!(
        matches!(j.status, JobStatus::Failed | JobStatus::RetryWait),
        "{j:?}"
    );
    assert!(lib.write(|c| vectors::active(c).unwrap()).is_none());

    let lib = library(Setup {
        embedder: None,
        ..Setup::fake()
    });
    install_model(&lib);
    with_transcript(&lib, "https://fake.test/a", &["x"]);
    let job = request(&lib, false).unwrap();
    lib.run();
    let j = lib.job(&job);
    assert_ne!(j.status, JobStatus::Succeeded);
    assert_eq!(j.error_code.as_deref(), Some("TOOL_MISSING"), "{j:?}");
    assert!(lib.write(|c| vectors::active(c).unwrap()).is_none());
}

/// The pinned llama.cpp build, its libraries, our flags and JSON reader and
/// the pinned model together: the index is built and a query in other words
/// (and in the other language) finds the right passage.
#[test]
fn real_llama_server_finds_the_topic() {
    let Some(model) = std::env::var_os("VV_EMBED_TEST_MODEL")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
    else {
        eprintln!("skipped: VV_EMBED_TEST_MODEL not set");
        return;
    };
    let Some(paths) = real_tool_paths() else {
        return;
    };
    assert!(
        paths.embedder.is_file(),
        "VV_EMBED_TEST_MODEL set but no llama-server in tools/bin"
    );
    let lib = library(Setup {
        embedder: Some(paths.embedder.clone()),
        ..Setup::fake()
    });
    install_model_from(&lib, Some(&model));
    with_transcript(
        &lib,
        "https://fake.test/a",
        &[
            "Bugün PostgreSQL'de akış replikasyonunu kuruyoruz; birincil sunucu WAL kayıtlarını yedek sunucuya gönderiyor.",
            "Ekşi maya ile ev ekmeği: hamuru bir gece mayalandırıp sabah taş fırında pişiriyoruz.",
        ],
    );
    with_transcript(
        &lib,
        "https://fake.test/b",
        &[
            "For your first marathon, build up the weekly long run slowly and rest one day a week.",
            "We compare index fund fees and why a low expense ratio matters over thirty years.",
        ],
    );
    let job = request(&lib, false).unwrap();
    lib.run();
    let j = lib.job(&job);
    assert_eq!(
        j.status,
        JobStatus::Succeeded,
        "{:?} {:?}",
        j.error_code,
        j.redacted_error_message
    );
    let g = lib.write(|c| vectors::active(c).unwrap()).unwrap();
    assert_eq!(g.producer.dims, 1024);
    assert_eq!(lib.write(|c| vectors::coverage(c, g.seq).unwrap()), (4, 4));
    assert!(best(&lib, "veritabanı yedekleme").contains("PostgreSQL"));
    assert!(best(&lib, "how to bake sourdough bread").contains("Ekşi maya"));
    assert!(best(&lib, "koşu antrenmanı").contains("marathon"));
    assert!(staging_is_clean(&lib));
}
