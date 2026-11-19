//! STATE.md F7.4: text read on screen (SPEC AI-C03, AI-C04, SEA-08, AC-41;
//! ADR-0039; migration 0017). Stored with its frame time, found by words
//! search, listed apart from speech, replaced when read again.

use vv_persistence::library::repo::{self, LibraryItem};
use vv_persistence::library::search::*;
use vv_persistence::library::*;
use vv_persistence::screen_text::{self, Line, RunStatus, Source};
use vv_persistence::transcripts::{self, NewSegment, Origin, Provenance, Status};
use vv_persistence::{Database, OpenOptions};

fn open() -> (tempfile::TempDir, Database) {
    let d = tempfile::tempdir().unwrap();
    let db = Database::open(&d.path().join("library.sqlite"), &OpenOptions::default()).unwrap();
    (d, db)
}

fn item(db: &mut Database, title: &str, n: i64) -> String {
    let mut it = LibraryItem::new(ItemKind::Bookmark, SourcePlatform::Generic, n);
    it.canonical_url = Some(format!("https://example.com/v/{n}"));
    it.title = Some(title.to_owned());
    repo::insert_item(db.writer(), &it).unwrap();
    it.id
}

fn src(item_id: &str) -> Source<'_> {
    Source {
        item_id,
        asset_id: None,
        provider: "test",
        language: "tr",
        config_version: 1,
        fingerprint: "size:1",
    }
}

fn line(a: i64, b: i64, t: &str) -> Line {
    Line {
        start_ms: a,
        end_ms: b,
        text: t.into(),
    }
}

fn search(db: &Database, text: &str) -> SearchPage {
    let c = db.reader().unwrap();
    vv_persistence::library::search::search(
        &c,
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
}

#[test]
fn text_on_screen_is_found_with_its_time_and_kept_apart_from_speech() {
    let (_d, mut db) = open();
    let a = item(&mut db, "Hava gösterisi", 1);
    let b = item(&mut db, "Başka video", 2);
    let run = screen_text::save(
        db.writer(),
        &src(&a),
        12,
        &[
            line(4_000, 9_000, "HARRIER JUMP JET\nMarine Corps"),
            line(9_000, 9_000, "zero length is dropped"),
            line(20_000, 26_000, "   "),
            line(30_000, 34_000, "Kadıköy vapuru 1985"),
        ],
        100,
    )
    .unwrap();
    assert_eq!(
        (run.status, run.frames, run.chunks),
        (RunStatus::Ready, 12, 2)
    );
    // Speech in the other item: same word, listed as speech there.
    let o = Origin {
        item_id: b.clone(),
        asset_id: None,
        language: "en".into(),
        provenance: Provenance::GeneratedLocal,
        provider: "whisper_cpp".into(),
        model_id: Some("whisper-base".into()),
        model_version: Some("x".into()),
        source_fingerprint: "sha256:x".into(),
        caption_kind: None,
    };
    let tx = db.writer().transaction().unwrap();
    transcripts::save(
        &tx,
        &o,
        Status::Ready,
        None,
        &[NewSegment::new(1_000, 2_000, "the harrier lands")],
        None,
        1,
    )
    .unwrap();
    tx.commit().unwrap();

    let p = search(&db, "harrier");
    assert_eq!(p.total, 2);
    let ha = p.hits.iter().find(|h| h.item.id == a).unwrap();
    assert_eq!(ha.matched, [Field::OnScreen]);
    assert!(ha.transcript.is_none(), "OCR is never speech");
    let s = ha.on_screen.as_ref().unwrap();
    assert_eq!(
        (s.provenance.as_str(), s.language.as_str(), s.total),
        ("ocr", "tr", 1)
    );
    let m = &s.moments[0];
    assert_eq!((m.start_ms, m.end_ms), (4_000, 9_000));
    assert_eq!(m.snippet.field, Field::OnScreen);
    assert!(m
        .snippet
        .parts
        .iter()
        .any(|p| p.highlight && p.text == "HARRIER"));
    let hb = p.hits.iter().find(|h| h.item.id == b).unwrap();
    assert_eq!(hb.matched, [Field::Transcript]);
    assert!(hb.on_screen.is_none());

    // Turkish folding as everywhere (ı → i).
    let p = search(&db, "kadikoy");
    assert_eq!(
        p.hits[0].on_screen.as_ref().unwrap().moments[0].start_ms,
        30_000
    );
    // The moment opens at its time.
    let c = db.reader().unwrap();
    assert_eq!(
        screen_text::place(&c, &m.segment_id).unwrap(),
        Some((a.clone(), 4_000, 9_000))
    );
    drop(c);

    // Read again: the old text and its moments are gone.
    screen_text::save(
        db.writer(),
        &src(&a),
        3,
        &[line(0, 1_000, "Yeni yazı")],
        200,
    )
    .unwrap();
    assert!(search(&db, "harrier").hits.iter().all(|h| h.item.id != a));
    let c = db.reader().unwrap();
    assert_eq!(screen_text::place(&c, &m.segment_id).unwrap(), None);
    assert_eq!(screen_text::lines(&c, &a).unwrap().len(), 1);
    assert_eq!(screen_text::counts(&c).unwrap(), (1, 0, 1));
    drop(c);

    // A file that could not be read is recorded, with no text.
    screen_text::save_failed(db.writer(), &src(&a), "MEDIA_CORRUPT", 300).unwrap();
    let c = db.reader().unwrap();
    let r = screen_text::get(&c, &a).unwrap().unwrap();
    assert_eq!(
        (r.status, r.error_code.as_deref()),
        (RunStatus::Failed, Some("MEDIA_CORRUPT"))
    );
    assert_eq!(screen_text::counts(&c).unwrap(), (0, 1, 0));
    drop(c);
    assert!(search(&db, "yeni").hits.is_empty());

    // Removing the item from the library removes its text (cascade).
    db.writer()
        .execute("DELETE FROM library_item WHERE id = ?1", [&a])
        .unwrap();
    let c = db.reader().unwrap();
    assert!(screen_text::get(&c, &a).unwrap().is_none());
}
