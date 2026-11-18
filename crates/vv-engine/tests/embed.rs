//! STATE.md F7.1, SPEC §16.3, AI-C07, AC-24 (ADR-0035): the embedding index
//! jobs against a fake OpenAI-compatible API on 127.0.0.1 (the user's
//! Ollama): build, search by meaning, fill after new transcripts, reindex
//! beside the active index, cancel, a failing API, a key, another model,
//! Offline Mode.

mod common;

use common::fake_embed::FakeEmbed;
use common::*;
use std::time::Duration;
use vv_engine::embed;
use vv_persistence::jobs::{self, CancelReason, JobKind, JobStatus};
use vv_persistence::library::Clock;
use vv_persistence::settings::{self, EmbeddingApi};
use vv_persistence::transcripts::{self, CaptionKind, NewSegment, Origin, Provenance, Status};
use vv_persistence::vectors::{self, GenerationStatus};
use vv_secrets::{Secret, SecretStore};

pub const MODEL: &str = "bge-m3";

/// The settings as after a successful "Check" of `fake`.
fn set_api(lib: &Lib, fake: &FakeEmbed, model: &str, key: Option<&str>) {
    let key_ref = key.map(|k| {
        lib.secrets
            .put(&Secret::new(k.as_bytes().to_vec()))
            .unwrap()
            .as_str()
            .to_owned()
    });
    let api = EmbeddingApi {
        url: embed::normalize_url(&fake.url).unwrap(),
        model: model.into(),
        dims: common::fake_embed::DIMS,
        key_ref,
        remote_allowed: false,
        checked_at: 1,
    };
    let now = lib.clock.now_ms();
    lib.write(|c| settings::set_embedding_api(c, Some(&api), now).unwrap());
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
    lib.write(|c| embed::request_index(c, rebuild, now))
        .unwrap()
        .map(|q| q.id)
}

/// The text of the best chunk for `query`: the API embeds the query, the
/// repository searches the active index.
fn best(lib: &Lib, fake: &FakeEmbed, query: &str) -> String {
    let api = embed::Api::new(&fake.url, MODEL, None, false, false).unwrap();
    let q = api.embed(&[query], None, || false).unwrap().remove(0);
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
fn no_api_means_no_index_and_lexical_search_is_untouched() {
    let lib = library(Setup::fake());
    with_transcript(&lib, "https://fake.test/a", &["merhaba dünya"]);
    let now = lib.clock.now_ms();
    assert_eq!(
        lib.write(|c| embed::request_index(c, false, now)),
        Err(embed::IndexError::NoApi)
    );
    assert!(lib.write(|c| embed::after_chunks_changed(c, now)).is_none());
    assert!(lib.write(|c| vectors::active(c).unwrap()).is_none());
}

#[test]
fn build_finds_a_topic_said_in_other_words() {
    let lib = library(Setup::fake());
    let fake = FakeEmbed::start(None);
    set_api(&lib, &fake, MODEL, None);
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
    assert_eq!(g.producer.provider, embed::PROVIDER);
    assert_eq!(g.producer.model_id, MODEL);
    assert_eq!(g.producer.model_version, fake.url);
    assert_eq!(g.producer.dims, 1024);
    assert_eq!(lib.write(|c| vectors::coverage(c, g.seq).unwrap()), (3, 3));
    // One request for the three chunks (a batch), with the model's name.
    assert_eq!(fake.calls(), vec![3]);
    assert_eq!(fake.models(), vec![MODEL]);
    assert_eq!(
        best(&lib, &fake, "veritabanı yedekleme"),
        "bugün PostgreSQL replikasyonu anlatıyoruz"
    );
    assert_eq!(
        best(&lib, &fake, "how to bake bread"),
        "ekşi maya ile ekmek"
    );
    // Up to date: nothing to queue.
    assert!(request(&lib, false).is_none());
}

#[test]
fn new_transcripts_are_added_to_the_active_index() {
    let lib = library(Setup::fake());
    let fake = FakeEmbed::start(None);
    set_api(&lib, &fake, MODEL, None);
    with_transcript(&lib, "https://fake.test/a", &["ekşi maya ile ekmek"]);
    request(&lib, false).unwrap();
    lib.run();
    with_transcript(&lib, "https://fake.test/b", &["PostgreSQL replikasyonu"]);
    let now = lib.clock.now_ms();
    let q = lib.write(|c| embed::after_chunks_changed(c, now)).unwrap();
    assert_eq!(lib.job(&q.id).kind, JobKind::Embedding);
    // Asking again while it is open gives the same job.
    let again = lib.write(|c| embed::after_chunks_changed(c, now)).unwrap();
    assert_eq!(again.id, q.id);
    lib.run();
    assert_eq!(lib.job(&q.id).status, JobStatus::Succeeded);
    let g = lib.write(|c| vectors::active(c).unwrap()).unwrap();
    assert_eq!(lib.write(|c| vectors::coverage(c, g.seq).unwrap()), (2, 2));
    // Only the new chunk was embedded.
    assert_eq!(fake.calls(), vec![1, 1]);
    assert_eq!(
        best(&lib, &fake, "database backup"),
        "PostgreSQL replikasyonu"
    );
}

#[test]
fn reindex_runs_beside_the_active_index_and_cancel_keeps_it() {
    // AC-24.
    let lib = library(Setup::fake());
    let fake = FakeEmbed::start(None);
    set_api(&lib, &fake, MODEL, None);
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
    assert_eq!(fake.calls(), vec![32, 32, 32, 24]);

    let fake2 = FakeEmbed::start(None);
    fake2.set_mode("slow-after-1");
    // Another server (address): its index is built beside the first.
    set_api(&lib, &fake2, MODEL, None);
    let job = request(&lib, true).unwrap();
    assert_eq!(lib.job(&job).kind, JobKind::Reindex);
    std::thread::scope(|s| {
        let h = s.spawn(|| lib.run());
        // Wait until the first batch is stored and the second request is under way.
        for _ in 0..1000 {
            if fake2.calls().len() >= 2 {
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

    // A completed rebuild replaces it.
    fake2.set_mode("ok");
    let job = request(&lib, true).unwrap();
    lib.run();
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
    let g = lib.write(|c| vectors::active(c).unwrap()).unwrap();
    assert_ne!(g.id, first.id);
    assert_eq!(g.status, GenerationStatus::Active);
    assert_eq!(g.producer.model_version, fake2.url);
    let n: i64 = lib.write(|c| {
        c.query_row("SELECT count(*) FROM chunk_embedding", [], |r| r.get(0))
            .unwrap()
    });
    assert_eq!(n, 120);
}

#[test]
fn another_model_in_the_settings_is_a_new_generation() {
    let lib = library(Setup::fake());
    let fake = FakeEmbed::start(None);
    set_api(&lib, &fake, MODEL, None);
    with_transcript(&lib, "https://fake.test/a", &["PostgreSQL replikasyonu"]);
    request(&lib, false).unwrap();
    lib.run();
    let first = lib.write(|c| vectors::active(c).unwrap()).unwrap();
    set_api(&lib, &fake, "nomic-embed-text", None);
    // New transcripts do not go into the old model's index.
    with_transcript(&lib, "https://fake.test/b", &["ekmek"]);
    let now = lib.clock.now_ms();
    assert!(lib.write(|c| embed::after_chunks_changed(c, now)).is_none());
    // Building asks for a new generation beside the old one.
    let job = request(&lib, false).unwrap();
    assert_eq!(lib.job(&job).kind, JobKind::Reindex);
    lib.run();
    let g = lib.write(|c| vectors::active(c).unwrap()).unwrap();
    assert_ne!(g.id, first.id);
    assert_eq!(g.producer.model_id, "nomic-embed-text");
}

#[test]
fn cancel_before_it_runs_drops_the_build() {
    let lib = library(Setup::fake());
    let fake = FakeEmbed::start(None);
    set_api(&lib, &fake, MODEL, None);
    with_transcript(&lib, "https://fake.test/a", &["x"]);
    request(&lib, false).unwrap();
    assert!(lib.write(|c| vectors::building(c).unwrap()).is_some());
    let now = lib.clock.now_ms();
    assert!(lib.write(|c| embed::cancel_build(c, now)));
    assert!(lib.write(|c| vectors::building(c).unwrap()).is_none());
    lib.run();
    assert!(lib.write(|c| vectors::active(c).unwrap()).is_none());
    assert!(fake.calls().is_empty());
}

#[test]
fn a_failing_api_fails_the_job_and_keeps_no_half_index_active() {
    for (mode, code) in [
        ("short", "EXTRACTOR_FAILED"),
        ("fail", "EXTRACTOR_FAILED"),
        ("dims-512", "EXTRACTOR_FAILED"),
    ] {
        let lib = library(Setup::fake());
        let fake = FakeEmbed::start(None);
        set_api(&lib, &fake, MODEL, None);
        with_transcript(&lib, "https://fake.test/a", &["x"]);
        fake.set_mode(mode);
        let job = request(&lib, false).unwrap();
        lib.run();
        let j = lib.job(&job);
        assert!(
            matches!(j.status, JobStatus::Failed | JobStatus::RetryWait),
            "{mode}: {j:?}"
        );
        assert_eq!(j.error_code.as_deref(), Some(code), "{mode}");
        assert!(lib.write(|c| vectors::active(c).unwrap()).is_none());
    }
    // Nothing listens there.
    let lib = library(Setup::fake());
    let gone = FakeEmbed::start(None);
    let mut dead = gone.clone();
    dead.url = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l);
        format!("http://127.0.0.1:{port}/v1")
    };
    set_api(&lib, &dead, MODEL, None);
    with_transcript(&lib, "https://fake.test/a", &["x"]);
    let job = request(&lib, false).unwrap();
    lib.run();
    assert_eq!(
        lib.job(&job).error_code.as_deref(),
        Some("NETWORK_UNAVAILABLE")
    );
}

#[test]
fn a_key_is_sent_from_the_credential_store() {
    let lib = library(Setup::fake());
    let fake = FakeEmbed::start(Some("k3y-123"));
    with_transcript(&lib, "https://fake.test/a", &["PostgreSQL"]);
    // Without the key: refused, the screen says the key is wrong.
    set_api(&lib, &fake, MODEL, None);
    let job = request(&lib, false).unwrap();
    lib.run();
    assert_eq!(lib.job(&job).error_code.as_deref(), Some("ACCESS_DENIED"));
    // With it: built. The key is nowhere in the library.
    set_api(&lib, &fake, MODEL, Some("k3y-123"));
    let job = request(&lib, true).unwrap();
    lib.run();
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
    assert!(!lib.dump().contains("k3y-123"));
}

#[test]
fn an_api_on_this_pc_works_in_offline_mode() {
    let lib = library(Setup::fake());
    let fake = FakeEmbed::start(None);
    set_api(&lib, &fake, MODEL, None);
    with_transcript(&lib, "https://fake.test/a", &["PostgreSQL"]);
    lib.engine.set_offline(true);
    let job = request(&lib, false).unwrap();
    lib.run();
    assert_eq!(lib.job(&job).status, JobStatus::Succeeded);
}
