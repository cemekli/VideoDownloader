//! STATE.md F7.1, SPEC §16.3, AC-24: migration 0014 (embedding models,
//! index generations, chunk vectors) and the vector repository.

use vv_persistence::library::repo::{self, LibraryItem};
use vv_persistence::library::{ItemKind, SourcePlatform};
use vv_persistence::models::{self, ModelSpec, ModelStatus};
use vv_persistence::transcripts::{self, CaptionKind, NewSegment, Origin, Provenance, Status};
use vv_persistence::vectors::{self, GenerationStatus, Producer};
use vv_persistence::{Database, OpenOptions};

fn open() -> (tempfile::TempDir, Database) {
    let d = tempfile::tempdir().unwrap();
    let db = Database::open(&d.path().join("library.sqlite"), &OpenOptions::default()).unwrap();
    (d, db)
}

/// One item with a two-chunk caption transcript; returns the chunk seqs.
fn transcript(db: &mut Database, url: &str) -> (String, Vec<i64>) {
    let mut it = LibraryItem::new(ItemKind::Bookmark, SourcePlatform::Generic, 1);
    it.canonical_url = Some(url.to_owned());
    it.original_url = Some(url.to_owned());
    repo::insert_item(db.writer(), &it).unwrap();
    let o = Origin {
        item_id: it.id.clone(),
        asset_id: None,
        language: "tr".into(),
        provenance: Provenance::SourceCaptions,
        provider: "source_captions".into(),
        model_id: None,
        model_version: None,
        source_fingerprint: "yt-dlp:subtitles:tr".into(),
        caption_kind: Some(CaptionKind::Manual),
    };
    let segs = vec![
        NewSegment::new(0, 2_000, "PostgreSQL replikasyonu"),
        NewSegment::new(60_000, 62_000, "ekşi maya ekmek"),
    ];
    let tx = db.writer().transaction().unwrap();
    transcripts::save(&tx, &o, Status::Ready, None, &segs, None, 5).unwrap();
    tx.commit().unwrap();
    (it.id, chunk_seqs(db, None))
}

fn chunk_seqs(db: &mut Database, item: Option<&str>) -> Vec<i64> {
    let c = db.writer();
    let mut st = c
        .prepare("SELECT seq FROM search_chunk WHERE ?1 IS NULL OR item_id = ?1 ORDER BY seq")
        .unwrap();
    st.query_map([item], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn producer(version: &str) -> Producer {
    Producer {
        provider: "llama_cpp".into(),
        model_id: "embed-qwen3-0.6b".into(),
        model_version: version.into(),
        dims: 8,
    }
}

fn unit(i: usize) -> Vec<f32> {
    let mut v = vec![0.0; 8];
    v[i] = 1.0;
    v
}

#[test]
fn embedding_models_share_the_consent_table() {
    let (_d, mut db) = open();
    let c = db.writer();
    let spec = ModelSpec {
        id: "embed-qwen3-0.6b".into(),
        provider: "llama_cpp".into(),
        version: "0123456789abcdef0123456789abcdef01234567".into(),
        file_name: "Qwen3-Embedding-0.6B-Q8_0.gguf".into(),
        size_bytes: 639_150_592,
        sha256: "b".repeat(64),
        license: "Apache-2.0".into(),
        source_url: "https://huggingface.co/Qwen/Qwen3-Embedding-0.6B-GGUF/resolve/x/f.gguf".into(),
    };
    let m = models::consent(c, &spec, 10).unwrap();
    assert_eq!(m.status, ModelStatus::Pending);
    // The schema still refuses a path, a whisper id under llama_cpp and the reverse.
    for (id, provider, file) in [
        ("embed-x", "llama_cpp", "../x.gguf"),
        ("embed-x", "llama_cpp", "sub/x.gguf"),
        ("whisper-x", "llama_cpp", "x.gguf"),
        ("embed-x", "whisper_cpp", "ggml-x.bin"),
        ("embed-x", "llama_cpp", "x.exe"),
    ] {
        let mut bad = spec.clone();
        bad.id = id.into();
        bad.provider = provider.into();
        bad.file_name = file.into();
        assert!(models::consent(c, &bad, 11).is_err(), "{id} {provider} {file}");
    }
}

#[test]
fn a_build_answers_nothing_until_activated_and_cancel_keeps_the_active_index() {
    // AC-24: build beside the active generation, swap when complete, a
    // canceled build leaves the old index as it was.
    let (_d, mut db) = open();
    let (_item, chunks) = transcript(&mut db, "https://example.com/a");
    assert_eq!(chunks.len(), 2);
    let c = db.writer();

    let g1 = vectors::begin_build(c, &producer("v1"), "g1", 20).unwrap();
    assert_eq!(g1.status, GenerationStatus::Building);
    assert!(vectors::active(c).unwrap().is_none());
    assert_eq!(vectors::pending(c, g1.seq, 10).unwrap().len(), 2);
    assert!(vectors::put(c, &g1, chunks[0], "PostgreSQL replikasyonu", &unit(0)).unwrap());
    assert!(vectors::put(c, &g1, chunks[1], "ekşi maya ekmek", &unit(1)).unwrap());
    assert_eq!(vectors::coverage(c, g1.seq).unwrap(), (2, 2));
    assert!(vectors::pending(c, g1.seq, 10).unwrap().is_empty());
    let g1 = vectors::activate(c, "g1", 30).unwrap();
    assert_eq!(g1.status, GenerationStatus::Active);

    // A reindex with a new model file starts beside it.
    let g2 = vectors::begin_build(c, &producer("v2"), "g2", 40).unwrap();
    assert_eq!(vectors::pending(c, g2.seq, 10).unwrap().len(), 2);
    vectors::put(c, &g2, chunks[0], "PostgreSQL replikasyonu", &unit(3)).unwrap();
    // Searches still use g1, and only g1's vectors.
    let a = vectors::active(c).unwrap().unwrap();
    assert_eq!(a.id, "g1");
    let hits = vectors::search(c, &a, &unit(0), 5).unwrap();
    assert_eq!(hits[0].chunk_seq, chunks[0]);
    assert!((hits[0].score - 1.0).abs() < 1e-6);
    // Cancel: g2 and its vectors are gone, g1 untouched.
    assert!(vectors::drop_generation(c, "g2").unwrap());
    assert_eq!(vectors::active(c).unwrap().unwrap().id, "g1");
    assert_eq!(vectors::coverage(c, g1.seq).unwrap(), (2, 2));
    let n: i64 = c
        .query_row("SELECT count(*) FROM chunk_embedding", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 2);

    // A completed rebuild replaces g1 and deletes its vectors.
    let g3 = vectors::begin_build(c, &producer("v3"), "g3", 50).unwrap();
    for (i, s) in chunks.iter().enumerate() {
        vectors::put(c, &g3, *s, "t", &unit(i + 4)).unwrap();
    }
    let tx = c.transaction().unwrap();
    vectors::activate(&tx, "g3", 60).unwrap();
    tx.commit().unwrap();
    assert_eq!(vectors::active(c).unwrap().unwrap().id, "g3");
    assert!(vectors::get(c, "g1").unwrap().is_none());
    let n: i64 = c
        .query_row("SELECT count(*) FROM chunk_embedding", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 2);
}

#[test]
fn same_producer_resumes_the_build_and_another_one_replaces_it() {
    let (_d, mut db) = open();
    let (_item, chunks) = transcript(&mut db, "https://example.com/a");
    let c = db.writer();
    let g = vectors::begin_build(c, &producer("v1"), "g1", 1).unwrap();
    vectors::put(c, &g, chunks[0], "x", &unit(0)).unwrap();
    // Asking again for the same model continues: the vector is kept.
    let again = vectors::begin_build(c, &producer("v1"), "other", 2).unwrap();
    assert_eq!(again.id, "g1");
    assert_eq!(vectors::coverage(c, g.seq).unwrap().0, 1);
    // Another model file: the half build is dropped.
    let g2 = vectors::begin_build(c, &producer("v2"), "g2", 3).unwrap();
    assert!(vectors::get(c, "g1").unwrap().is_none());
    assert_eq!(vectors::coverage(c, g2.seq).unwrap().0, 0);
    // The schema allows only one building generation.
    assert!(c
        .execute(
            "INSERT INTO embedding_generation (id, provider, model_id, model_version, dims, \
                 status, created_at) VALUES ('x', 'llama_cpp', 'embed-a', 'v', 8, 'building', 1)",
            [],
        )
        .is_err());
}

#[test]
fn vectors_follow_their_chunks_and_bad_vectors_are_refused() {
    let (_d, mut db) = open();
    let (item, chunks) = transcript(&mut db, "https://example.com/a");
    let c = db.writer();
    let g = vectors::begin_build(c, &producer("v1"), "g1", 1).unwrap();
    assert!(vectors::put(c, &g, chunks[0], "x", &[1.0; 3]).is_err());
    assert!(vectors::put(c, &g, chunks[0], "x", &[0.0; 8]).is_err());
    let mut nan = unit(0);
    nan[1] = f32::NAN;
    assert!(vectors::put(c, &g, chunks[0], "x", &nan).is_err());
    // Stored normalized.
    let mut big = unit(0);
    big[0] = 5.0;
    vectors::put(c, &g, chunks[0], "x", &big).unwrap();
    let blob: Vec<u8> = c
        .query_row("SELECT vector FROM chunk_embedding", [], |r| r.get(0))
        .unwrap();
    assert_eq!(vectors::decode(&blob), unit(0));
    // A chunk that no longer exists is skipped, not an error.
    assert!(!vectors::put(c, &g, 999_999, "x", &unit(0)).unwrap());
    // Deleting the item deletes its chunks and their vectors.
    c.execute("DELETE FROM library_item WHERE id = ?1", [&item])
        .unwrap();
    let n: i64 = c
        .query_row("SELECT count(*) FROM chunk_embedding", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
    let _ = chunks;
}

#[test]
fn search_orders_by_cosine_and_respects_k() {
    let (_d, mut db) = open();
    transcript(&mut db, "https://example.com/a");
    transcript(&mut db, "https://example.com/b");
    let chunks = chunk_seqs(&mut db, None);
    assert_eq!(chunks.len(), 4);
    let c = db.writer();
    let g = vectors::begin_build(c, &producer("v1"), "g1", 1).unwrap();
    let vs = [
        [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        [0.9, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    ];
    for (s, v) in chunks.iter().zip(vs) {
        vectors::put(c, &g, *s, "t", &v).unwrap();
    }
    let hits = vectors::search(c, &g, &unit(0), 3).unwrap();
    let order: Vec<i64> = hits.iter().map(|h| h.chunk_seq).collect();
    assert_eq!(order, vec![chunks[0], chunks[1], chunks[2]]);
    assert!(hits[0].score >= hits[1].score && hits[1].score >= hits[2].score);
    assert!(vectors::search(c, &g, &[1.0; 4], 3).is_err());
}
