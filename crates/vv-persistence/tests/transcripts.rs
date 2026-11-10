//! STATE.md F6.1, SPEC §11.5, §16.2, §17.1: migration 0013 (transcripts,
//! search chunks, model installations) and their repositories.

use vv_persistence::library::repo::{self, LibraryItem};
use vv_persistence::library::{ItemKind, SourcePlatform};
use vv_persistence::models::{self, ModelSpec, ModelStatus};
use vv_persistence::transcripts::{
    self, chunk_ranges, CaptionKind, NewSegment, Origin, Provenance, Status, CHUNK_MAX_MS,
};
use vv_persistence::{Database, OpenOptions};

fn open() -> (tempfile::TempDir, Database) {
    let d = tempfile::tempdir().unwrap();
    let db = Database::open(&d.path().join("library.sqlite"), &OpenOptions::default()).unwrap();
    (d, db)
}

fn item(db: &mut Database, url: &str) -> String {
    let mut it = LibraryItem::new(ItemKind::Bookmark, SourcePlatform::Generic, 1);
    it.canonical_url = Some(url.to_owned());
    it.original_url = Some(url.to_owned());
    repo::insert_item(db.writer(), &it).unwrap();
    it.id
}

fn captions(item_id: &str, lang: &str) -> Origin {
    Origin {
        item_id: item_id.to_owned(),
        asset_id: None,
        language: lang.to_owned(),
        provenance: Provenance::SourceCaptions,
        provider: "source_captions".into(),
        model_id: None,
        model_version: None,
        source_fingerprint: "yt-dlp:subtitles:tr".into(),
        caption_kind: Some(CaptionKind::Manual),
    }
}

fn spec() -> ModelSpec {
    ModelSpec {
        id: "whisper-tiny".into(),
        provider: "whisper_cpp".into(),
        version: "0123456789abcdef0123456789abcdef01234567".into(),
        file_name: "ggml-tiny.bin".into(),
        size_bytes: 77_691_713,
        sha256: "a".repeat(64),
        license: "MIT".into(),
        source_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/x/ggml-tiny.bin".into(),
    }
}

fn hits(db: &Database, word: &str) -> Vec<(String, i64)> {
    let c = db.reader().unwrap();
    let mut st = c
        .prepare(
            "SELECT c.transcript_id, c.start_ms FROM transcript_search f \
             JOIN search_chunk c ON c.seq = f.rowid WHERE transcript_search MATCH ?1 \
             ORDER BY c.start_ms",
        )
        .unwrap();
    st.query_map([word], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn segs() -> Vec<NewSegment> {
    vec![
        NewSegment::new(0, 2_000, "Merhaba, bugün"),
        NewSegment::new(2_000, 4_500, "Işık ve gölge konuşacağız."),
        NewSegment::new(40_000, 42_000, "Second chunk starts here"),
    ]
}

#[test]
fn no_model_row_without_consent_and_consent_needs_a_pin() {
    // AC-20: a fresh library knows of no installed model.
    let (_d, mut db) = open();
    assert!(models::list(db.writer()).unwrap().is_empty());
    let mut unpinned = spec();
    unpinned.sha256 = String::new();
    assert!(models::consent(db.writer(), &unpinned, 5).is_err());
    assert!(models::list(db.writer()).unwrap().is_empty());
}

#[test]
fn consent_then_ready_and_a_new_catalog_entry_resets_it() {
    let (_d, mut db) = open();
    let c = db.writer();
    let m = models::consent(c, &spec(), 10).unwrap();
    assert_eq!(m.status, ModelStatus::Pending);
    assert_eq!(m.consent_at, 10);
    // The schema itself refuses `ready` without a verification time.
    assert!(c
        .execute(
            "UPDATE model_installation SET status = 'ready' WHERE id = 'whisper-tiny'",
            []
        )
        .is_err());
    assert!(models::set_status(c, "whisper-tiny", ModelStatus::Ready, 11).is_err());
    models::mark_ready(c, "whisper-tiny", 20).unwrap();
    let m = models::get(c, "whisper-tiny").unwrap().unwrap();
    assert_eq!(
        (m.status, m.installed_at, m.verified_at),
        (ModelStatus::Ready, Some(20), Some(20))
    );
    // Same entry again: nothing changes.
    assert_eq!(models::consent(c, &spec(), 30).unwrap().consent_at, 10);
    // A new file (other hash) is a new consent and is not ready.
    let mut next = spec();
    next.sha256 = "b".repeat(64);
    let m = models::consent(c, &next, 40).unwrap();
    assert_eq!(
        (m.status, m.consent_at, m.verified_at),
        (ModelStatus::Pending, 40, None)
    );
    assert!(models::remove(c, "whisper-tiny").unwrap().is_some());
    assert!(models::get(c, "whisper-tiny").unwrap().is_none());
}

#[test]
fn model_file_name_must_be_a_bare_ggml_name() {
    let (_d, mut db) = open();
    for bad in [
        "../ggml-tiny.bin",
        "ggml-..\\x.bin",
        "C:ggml-x.bin",
        "tiny.bin",
    ] {
        let mut s = spec();
        s.file_name = bad.into();
        assert!(models::consent(db.writer(), &s, 1).is_err(), "{bad}");
    }
}

#[test]
fn save_writes_segments_chunks_and_index() {
    let (_d, mut db) = open();
    let id = item(&mut db, "https://example.com/a");
    let tx = db.writer().transaction().unwrap();
    let t = transcripts::save(
        &tx,
        &captions(&id, "tr"),
        Status::Ready,
        Some(42_000),
        &segs(),
        Some(60_000),
        5,
    )
    .unwrap();
    tx.commit().unwrap();
    assert_eq!(t.segment_count, 3);
    assert_eq!(t.origin.caption_kind, Some(CaptionKind::Manual));
    let s = transcripts::segments(db.writer(), &t.id).unwrap();
    assert_eq!(
        s.iter().map(|s| s.ordinal).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    // Two chunks: the gap pushes the third segment past CHUNK_MAX_MS.
    assert_eq!(hits(&db, "merhaba"), vec![(t.id.clone(), 0)]);
    assert_eq!(hits(&db, "chunk"), vec![(t.id.clone(), 40_000)]);
    // Turkish folding as in search_document: dotted/dotless I and ğ.
    assert_eq!(hits(&db, "isik").len(), 1);
    assert_eq!(hits(&db, "golge").len(), 1);
}

#[test]
fn regenerating_replaces_segments_and_index_rows() {
    let (_d, mut db) = open();
    let id = item(&mut db, "https://example.com/a");
    let o = captions(&id, "tr");
    let tx = db.writer().transaction().unwrap();
    let a = transcripts::save(&tx, &o, Status::Ready, None, &segs(), None, 5).unwrap();
    let b = transcripts::save(
        &tx,
        &o,
        Status::Ready,
        None,
        &[NewSegment::new(0, 900, "yeni metin")],
        None,
        6,
    )
    .unwrap();
    tx.commit().unwrap();
    assert_eq!(a.id, b.id);
    assert_eq!(b.segment_count, 1);
    assert!(hits(&db, "merhaba").is_empty());
    assert_eq!(hits(&db, "yeni").len(), 1);
    let n: i64 = db
        .writer()
        .query_row("SELECT count(*) FROM transcript_search", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
}

#[test]
fn other_language_or_provenance_is_a_separate_transcript() {
    let (_d, mut db) = open();
    let id = item(&mut db, "https://example.com/a");
    let tx = db.writer().transaction().unwrap();
    let tr = transcripts::save(
        &tx,
        &captions(&id, "tr"),
        Status::Ready,
        None,
        &segs(),
        None,
        1,
    )
    .unwrap();
    let en = transcripts::save(
        &tx,
        &captions(&id, "en"),
        Status::Ready,
        None,
        &segs(),
        None,
        2,
    )
    .unwrap();
    let mut local = captions(&id, "tr");
    local.provenance = Provenance::GeneratedLocal;
    local.provider = "whisper_cpp".into();
    local.caption_kind = None;
    // A local transcript must name its model.
    assert!(transcripts::save(&tx, &local, Status::Ready, None, &segs(), None, 3).is_err());
    local.model_id = Some("whisper-tiny".into());
    local.model_version = Some("rev".into());
    let gen = transcripts::save(
        &tx,
        &local,
        Status::Partial,
        Some(4_500),
        &segs()[..2],
        None,
        3,
    )
    .unwrap();
    tx.commit().unwrap();
    assert_eq!(
        transcripts::list_for_item(db.writer(), &id).unwrap().len(),
        3
    );
    assert!(tr.id != en.id && en.id != gen.id);
    assert_eq!(gen.status, Status::Partial);
    assert_eq!(gen.covered_ms, Some(4_500));
}

#[test]
fn invalid_segments_are_refused_and_nothing_is_written() {
    let (_d, mut db) = open();
    let id = item(&mut db, "https://example.com/a");
    let cases: Vec<(Vec<NewSegment>, &str)> = vec![
        (vec![NewSegment::new(1_000, 1_000, "x")], "start < end"),
        (vec![NewSegment::new(-1, 10, "x")], "start < end"),
        (vec![NewSegment::new(0, 10, "  ")], "empty"),
        (
            vec![
                NewSegment::new(500, 900, "a"),
                NewSegment::new(100, 200, "b"),
            ],
            "previous",
        ),
        // Media is 10 s long: an end 1.5 s later is past the tolerance.
        (vec![NewSegment::new(9_000, 11_500, "x")], "ends after"),
        (vec![NewSegment::new(10_000, 10_500, "x")], "starts after"),
    ];
    for (s, rule) in cases {
        let tx = db.writer().transaction().unwrap();
        let e = transcripts::save(
            &tx,
            &captions(&id, "tr"),
            Status::Ready,
            None,
            &s,
            Some(10_000),
            1,
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains(rule), "{e} / {rule}");
    }
    assert!(transcripts::list_for_item(db.writer(), &id)
        .unwrap()
        .is_empty());
    // Inside the tolerance the end is cut to the duration, not invented.
    let tx = db.writer().transaction().unwrap();
    let t = transcripts::save(
        &tx,
        &captions(&id, "tr"),
        Status::Ready,
        None,
        &[NewSegment::new(9_000, 10_800, "son")],
        Some(10_000),
        1,
    )
    .unwrap();
    assert_eq!(transcripts::segments(&tx, &t.id).unwrap()[0].end_ms, 10_000);
}

#[test]
fn deleting_the_item_or_transcript_removes_index_rows() {
    let (_d, mut db) = open();
    let a = item(&mut db, "https://example.com/a");
    let b = item(&mut db, "https://example.com/b");
    let tx = db.writer().transaction().unwrap();
    let ta = transcripts::save(
        &tx,
        &captions(&a, "tr"),
        Status::Ready,
        None,
        &segs(),
        None,
        1,
    )
    .unwrap();
    transcripts::save(
        &tx,
        &captions(&b, "tr"),
        Status::Ready,
        None,
        &segs(),
        None,
        1,
    )
    .unwrap();
    tx.commit().unwrap();
    assert_eq!(hits(&db, "merhaba").len(), 2);
    transcripts::delete(db.writer(), &ta.id).unwrap();
    assert_eq!(hits(&db, "merhaba").len(), 1);
    db.writer()
        .execute("DELETE FROM library_item WHERE id = ?1", [&b])
        .unwrap();
    assert!(hits(&db, "merhaba").is_empty());
    let n: i64 = db
        .writer()
        .query_row("SELECT count(*) FROM search_chunk", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}

#[test]
fn chunks_cover_all_segments_in_order_without_overlap() {
    let segs: Vec<NewSegment> = (0..200)
        .map(|i| NewSegment::new(i * 1_000, i * 1_000 + 900, format!("kelime {i}")))
        .collect();
    let r = chunk_ranges(&segs);
    assert_eq!(r.first().unwrap().0, 0);
    assert_eq!(r.last().unwrap().1, 199);
    for w in r.windows(2) {
        assert_eq!(w[0].1 + 1, w[1].0);
    }
    for (a, b) in r {
        assert!(segs[b].end_ms - segs[a].start_ms <= CHUNK_MAX_MS);
    }
}
