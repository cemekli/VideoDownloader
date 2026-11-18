//! STATE.md F7.2, SPEC SEA-07, §16.3, AC-40 (ADR-0036): the search box's
//! query by meaning against a fake embedding API on 127.0.0.1: when it can
//! run and why not, the vector cache, and the hybrid result.
//!
//! The last test runs against a real API when `VV_EMBED_TEST_URL` and
//! `VV_EMBED_TEST_MODEL` name one (Cuma's Ollama: `http://localhost:11434/v1`
//! and `bge-m3`) over the TR/EN fixture and writes the AC-40 quality report
//! to `test-artifacts/hybrid-search.md`.

mod common;

use common::fake_embed::FakeEmbed;
use common::*;
use vv_engine::embed;
use vv_engine::meaning::{self, Off, QueryEmbedder};
use vv_persistence::jobs::JobStatus;
use vv_persistence::library::hybrid::{self, MeaningQuery};
use vv_persistence::library::search::{Field, SearchPage, SearchQuery, Sort, SortField};
use vv_persistence::library::Clock;
use vv_persistence::settings::{self, EmbeddingApi};
use vv_persistence::transcripts::{self, CaptionKind, NewSegment, Origin, Provenance, Status};
use vv_persistence::vectors;

const MODEL: &str = "bge-m3";

fn set_api(lib: &Lib, url: &str, model: &str, dims: usize) {
    let api = EmbeddingApi {
        url: embed::normalize_url(url).unwrap(),
        model: model.into(),
        dims,
        key_ref: None,
        remote_allowed: false,
        checked_at: 1,
    };
    let now = lib.clock.now_ms();
    lib.write(|c| settings::set_embedding_api(c, Some(&api), now).unwrap());
}

/// A bookmark with a caption transcript of `lines` (one chunk each).
fn with_transcript(lib: &Lib, url: &str, lang: &str, lines: &[&str]) -> String {
    let item = lib.save(url);
    let segs: Vec<NewSegment> = lines
        .iter()
        .enumerate()
        .map(|(i, t)| NewSegment::new(i as i64 * 40_000, i as i64 * 40_000 + 3_000, *t))
        .collect();
    let o = Origin {
        item_id: item.clone(),
        asset_id: None,
        language: lang.into(),
        provenance: Provenance::SourceCaptions,
        provider: "source_captions".into(),
        model_id: None,
        model_version: None,
        source_fingerprint: format!("yt-dlp:subtitles:{lang}"),
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

fn build_index(lib: &Lib) {
    let now = lib.clock.now_ms();
    let job = lib
        .write(|c| embed::request_index(c, false, now))
        .unwrap()
        .unwrap()
        .id;
    lib.run();
    let j = lib.job(&job);
    assert_eq!(
        j.status,
        JobStatus::Succeeded,
        "{:?} {:?}",
        j.error_code,
        j.redacted_error_message
    );
}

fn plan(lib: &Lib) -> Result<meaning::Plan, Off> {
    let secrets = lib.secrets.clone();
    let offline = lib.cx().is_offline();
    lib.write(|c| meaning::prepare(c, secrets.as_ref(), offline))
}

fn query(text: &str) -> SearchQuery {
    SearchQuery {
        text: text.into(),
        prefix_last: false,
        sort: Sort {
            field: SortField::Relevance,
            descending: true,
        },
        ..Default::default()
    }
}

/// The search box's path: plan, embed (no connection held), fuse.
fn hybrid_search(lib: &Lib, e: &QueryEmbedder, text: &str) -> SearchPage {
    let p = plan(lib).unwrap();
    let v = e.embed(&p, text).unwrap();
    lib.write(|c| {
        hybrid::search_hybrid(
            c,
            &query(text),
            Some(&MeaningQuery {
                generation: &p.generation,
                vector: &v,
            }),
        )
        .unwrap()
    })
}

fn words_search(lib: &Lib, text: &str) -> SearchPage {
    lib.write(|c| hybrid::search_hybrid(c, &query(text), None).unwrap())
}

#[test]
fn search_by_meaning_says_why_it_is_off() {
    let lib = library(Setup::fake());
    let fake = FakeEmbed::start(None);
    with_transcript(
        &lib,
        "https://fake.test/a",
        "en",
        &["PostgreSQL replication backup"],
    );
    assert_eq!(plan(&lib).err(), Some(Off::NoApi));
    set_api(&lib, &fake.url, MODEL, 1024);
    assert_eq!(plan(&lib).err(), Some(Off::NoIndex), "an API, no index yet");
    build_index(&lib);
    assert!(plan(&lib).is_ok());
    // Another model in the settings: the index cannot answer its queries.
    set_api(&lib, &fake.url, "other-model", 1024);
    assert_eq!(plan(&lib).err(), Some(Off::OtherModel));
    set_api(&lib, &fake.url, MODEL, 1024);
    // A remote API in Offline Mode: off.
    let now = lib.clock.now_ms();
    let g = lib.write(|c| vectors::active(c).unwrap()).unwrap();
    let remote = EmbeddingApi {
        url: "https://embed.example.com/v1".into(),
        model: MODEL.into(),
        dims: 1024,
        key_ref: None,
        remote_allowed: true,
        checked_at: 1,
    };
    lib.write(|c| {
        settings::set_embedding_api(c, Some(&remote), now).unwrap();
        c.execute(
            "UPDATE embedding_generation SET model_version = ?1 WHERE seq = ?2",
            vv_persistence::rusqlite::params![remote.url, g.seq],
        )
        .unwrap();
    });
    lib.engine.set_offline(true);
    assert_eq!(plan(&lib).err(), Some(Off::Offline));
    // The API removed from the settings (AC-40): search by meaning is off
    // and the word search still finds what it found before.
    let before = words_search(&lib, "replication");
    lib.write(|c| settings::set_embedding_api(c, None, now).unwrap());
    assert_eq!(plan(&lib).err(), Some(Off::NoApi));
    let after = words_search(&lib, "replication");
    assert_eq!(after.total, 1);
    assert_eq!(after.hits[0].item.id, before.hits[0].item.id);
}

#[test]
fn a_meaning_query_finds_other_words_and_is_fused_with_the_word_search() {
    let lib = library(Setup::fake());
    let fake = FakeEmbed::start(None);
    set_api(&lib, &fake.url, MODEL, 1024);
    let db = with_transcript(
        &lib,
        "https://fake.test/db",
        "en",
        &["PostgreSQL replication backup"],
    );
    let both = with_transcript(
        &lib,
        "https://fake.test/both",
        "tr",
        &["veritabanı yedekleme ve PostgreSQL"],
    );
    with_transcript(
        &lib,
        "https://fake.test/bread",
        "en",
        &["sourdough bread dough bake"],
    );
    build_index(&lib);
    let e = QueryEmbedder::new();

    // Words alone: only the video that says the words.
    let w = words_search(&lib, "veritabanı yedekleme");
    assert_eq!(
        w.hits.iter().map(|h| &h.item.id).collect::<Vec<_>>(),
        [&both]
    );

    let p = hybrid_search(&lib, &e, "veritabanı yedekleme");
    let ids: Vec<&String> = p.hits.iter().map(|h| &h.item.id).collect();
    assert_eq!(ids, [&both, &db], "found both ways first, bread not at all");
    assert_eq!(p.hits[0].matched, [Field::Transcript, Field::Meaning]);
    assert_eq!(p.hits[1].matched, [Field::Meaning]);
    let m = p.hits[1].meaning.as_ref().unwrap();
    assert_eq!(m.moments[0].snippet.text(), "PostgreSQL replication backup");
    assert_eq!(m.language, "en");
}

#[test]
fn query_vectors_are_cached() {
    let lib = library(Setup::fake());
    let fake = FakeEmbed::start(None);
    set_api(&lib, &fake.url, MODEL, 1024);
    with_transcript(
        &lib,
        "https://fake.test/a",
        "en",
        &["PostgreSQL replication backup"],
    );
    build_index(&lib);
    let index_calls = fake.calls().len();
    let e = QueryEmbedder::new();
    let p = plan(&lib).unwrap();
    for q in ["backup", "bread", "marathon", "backup"] {
        e.embed(&p, q).unwrap();
    }
    // "backup" the second time came from the cache: three requests.
    assert_eq!(e.requests(), 3);
    assert_eq!(fake.calls().len(), index_calls + 3);
    e.clear();
    e.embed(&p, "backup").unwrap();
    assert_eq!(e.requests(), 4);
}

#[test]
fn a_failing_api_is_reported_and_words_still_work() {
    let lib = library(Setup::fake());
    let fake = FakeEmbed::start(None);
    set_api(&lib, &fake.url, MODEL, 1024);
    with_transcript(
        &lib,
        "https://fake.test/a",
        "en",
        &["PostgreSQL replication backup"],
    );
    build_index(&lib);
    fake.set_mode("fail");
    let e = QueryEmbedder::new();
    let p = plan(&lib).unwrap();
    assert!(e.embed(&p, "backup").is_err());
    assert_eq!(words_search(&lib, "backup").total, 1);
}

// ------------------------------------------------------------ AC-40 report

#[derive(serde::Deserialize)]
struct Fixture {
    passages: Vec<Passage>,
    queries: Vec<FixtureQuery>,
}

#[derive(serde::Deserialize)]
struct Passage {
    id: String,
    lang: String,
    text: String,
}

#[derive(serde::Deserialize)]
struct FixtureQuery {
    id: String,
    text: String,
    relevant: Vec<String>,
}

/// AC-40 with the real tool and model: every fixture passage is one video's
/// transcript; each query is run as the search box runs it. Reports
/// hit@1, MRR and recall for words alone and for hybrid search, and the
/// similarities that the floor `MIN_SIMILARITY` is chosen from.
#[test]
fn real_model_hybrid_quality_report() {
    let (Some(url), Some(model)) = (
        std::env::var("VV_EMBED_TEST_URL")
            .ok()
            .filter(|v| !v.is_empty()),
        std::env::var("VV_EMBED_TEST_MODEL")
            .ok()
            .filter(|v| !v.is_empty()),
    ) else {
        eprintln!("skipped: VV_EMBED_TEST_URL / VV_EMBED_TEST_MODEL not set");
        return;
    };
    let lib = library(Setup::fake());
    let api = embed::Api::new(&url, &model, None, false, false).unwrap();
    let dims = embed::check(&api).unwrap();
    set_api(&lib, &url, &model, dims);
    let fx: Fixture = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("tests/fixtures/semantic-tr-en.json")).unwrap(),
    )
    .unwrap();
    let mut item_of = std::collections::HashMap::new();
    for p in &fx.passages {
        let id = with_transcript(
            &lib,
            &format!("https://fake.test/{}", p.id),
            &p.lang,
            &[&p.text],
        );
        item_of.insert(id, p.id.clone());
    }
    build_index(&lib);
    let e = QueryEmbedder::new();
    let pl = plan(&lib).unwrap();

    let mut rows = Vec::new();
    let (mut w_hit, mut h_hit, mut h_mrr, mut h_recall, mut wrong) = (0, 0, 0.0, 0.0, 0usize);
    let (mut rel_min, mut irr_max) = (f32::MAX, f32::MIN);
    for q in &fx.queries {
        let v = e.embed(&pl, &q.text).unwrap();
        // Similarities straight from the index (for the floor).
        let all = lib.write(|c| vectors::search(c, &pl.generation, &v, 100).unwrap());
        for h in &all {
            let pid: String = lib.write(|c| {
                c.query_row(
                    "SELECT item_id FROM search_chunk WHERE seq = ?1",
                    [h.chunk_seq],
                    |r| r.get::<_, String>(0),
                )
                .unwrap()
            });
            if q.relevant.contains(&item_of[&pid]) {
                rel_min = rel_min.min(h.score);
            } else {
                irr_max = irr_max.max(h.score);
            }
        }
        let words = words_search(&lib, &q.text);
        let p = lib.write(|c| {
            hybrid::search_hybrid(
                c,
                &query(&q.text),
                Some(&MeaningQuery {
                    generation: &pl.generation,
                    vector: &v,
                }),
            )
            .unwrap()
        });
        let got: Vec<&String> = p.hits.iter().map(|h| &item_of[&h.item.id]).collect();
        let wgot: Vec<&String> = words.hits.iter().map(|h| &item_of[&h.item.id]).collect();
        if wgot.first().is_some_and(|x| q.relevant.contains(x)) {
            w_hit += 1;
        }
        let first = got.iter().position(|x| q.relevant.contains(x));
        if first == Some(0) {
            h_hit += 1;
        }
        h_mrr += first.map_or(0.0, |k| 1.0 / (k as f64 + 1.0));
        let found = q.relevant.iter().filter(|r| got.contains(r)).count();
        h_recall += found as f64 / q.relevant.len() as f64;
        wrong += got.iter().filter(|x| !q.relevant.contains(x)).count();
        rows.push(format!(
            "| {} | {} | {} | {} |",
            q.id,
            q.text,
            if wgot.is_empty() {
                "—".to_owned()
            } else {
                wgot.iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            },
            got.iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let n = fx.queries.len();
    let report = format!(
        "# Hybrid search quality (AC-40, F7.2)\n\n\
         API {url}, model {model}, fixture tests/fixtures/semantic-tr-en.json ({} passages, {n} queries).\n\
         MIN_SIMILARITY = {:.2}\n\n\
         | | hit@1 | MRR | recall | unrelated results |\n|---|---|---|---|---|\n\
         | words only | {w_hit}/{n} | | | |\n\
         | hybrid | {h_hit}/{n} | {:.3} | {:.3} | {wrong} |\n\n\
         Lowest similarity of a relevant passage: {rel_min:.3}. Highest of an unrelated one: {irr_max:.3}.\n\n\
         | query | text | words only | hybrid |\n|---|---|---|---|\n{}\n",
        fx.passages.len(),
        hybrid::MIN_SIMILARITY,
        h_mrr / n as f64,
        h_recall / n as f64,
        rows.join("\n")
    );
    eprintln!("{report}");
    let out = repo_root().join("test-artifacts");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("hybrid-search.md"), &report).unwrap();
    assert!(h_hit >= n - 1, "hybrid hit@1 {h_hit}/{n}\n{report}");
}
