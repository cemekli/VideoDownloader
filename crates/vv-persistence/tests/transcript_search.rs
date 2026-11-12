//! STATE.md F6.4: transcript search (SPEC SEA-06, §16.2, AC-22, ADR-0030).
//! An item matches by its metadata or by the words of a `ready`/`partial`
//! transcript; moments are grouped under the item and open at the real
//! start of the segment holding the matched word.

use vv_persistence::library::repo::{self, LibraryItem};
use vv_persistence::library::search::*;
use vv_persistence::library::*;
use vv_persistence::transcripts::{self, CaptionKind, NewSegment, Origin, Provenance, Status};
use vv_persistence::{Database, OpenOptions};

fn open() -> (tempfile::TempDir, Database) {
    let d = tempfile::tempdir().unwrap();
    let db = Database::open(&d.path().join("library.sqlite"), &OpenOptions::default()).unwrap();
    (d, db)
}

fn item(db: &mut Database, url: &str, title: &str, added_at: i64) -> String {
    let mut it = LibraryItem::new(ItemKind::Bookmark, SourcePlatform::Generic, added_at);
    it.canonical_url = Some(url.to_owned());
    it.title = Some(title.to_owned());
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
        source_fingerprint: format!("yt-dlp:subtitles:{lang}"),
        caption_kind: Some(CaptionKind::Manual),
    }
}

fn local(item_id: &str, lang: &str) -> Origin {
    Origin {
        item_id: item_id.to_owned(),
        asset_id: None,
        language: lang.to_owned(),
        provenance: Provenance::GeneratedLocal,
        provider: "whisper_cpp".into(),
        model_id: Some("whisper-base".into()),
        model_version: Some("abc".into()),
        source_fingerprint: "sha256:x".into(),
        caption_kind: None,
    }
}

fn save(
    db: &mut Database,
    o: &Origin,
    status: Status,
    covered: Option<i64>,
    segs: &[NewSegment],
    now: i64,
) -> String {
    let tx = db.writer().transaction().unwrap();
    let t = transcripts::save(&tx, o, status, covered, segs, None, now).unwrap();
    tx.commit().unwrap();
    t.id
}

fn search(db: &Database, text: &str) -> SearchPage {
    search_with(db, text, Filters::default())
}

fn search_with(db: &Database, text: &str, filters: Filters) -> SearchPage {
    let c = db.reader().unwrap();
    vv_persistence::library::search::search(
        &c,
        &SearchQuery {
            text: text.into(),
            filters,
            sort: Sort {
                field: SortField::Relevance,
                descending: true,
            },
            ..Default::default()
        },
    )
    .unwrap()
}

fn lecture() -> Vec<NewSegment> {
    vec![
        NewSegment::new(0, 2_000, "Merhaba, bugün"),
        NewSegment::new(2_000, 4_500, "PostgreSQL replikasyonunu"),
        NewSegment::new(4_500, 7_000, "ve ışık hızında yedeklemeyi konuşacağız."),
        NewSegment::new(61_000, 63_000, "Replikasyon gecikmesi ölçülür."),
    ]
}

#[test]
fn transcript_words_find_the_item_and_open_at_the_segment() {
    let (_d, mut db) = open();
    let id = item(&mut db, "https://example.com/a", "Ders 4", 1);
    let tid = save(
        &mut db,
        &captions(&id, "tr"),
        Status::Ready,
        None,
        &lecture(),
        5,
    );

    let page = search(&db, "yedeklemeyi");
    assert_eq!(page.total, 1);
    let hit = &page.hits[0];
    assert_eq!(hit.item.id, id);
    assert_eq!(hit.matched, vec![Field::Transcript]);
    assert!(hit.snippet.is_none(), "no metadata matched");
    let tr = hit.transcript.as_ref().unwrap();
    assert_eq!(tr.transcript_id, tid);
    assert_eq!(tr.language, "tr");
    assert_eq!(tr.provenance, "source_captions");
    assert!(!tr.partial);
    assert_eq!(tr.total, 1);
    // The chunk starts at 0 s; the word is in the third segment (4.5 s).
    let m = &tr.moments[0];
    assert_eq!((m.start_ms, m.end_ms), (4_500, 7_000));
    let segs = transcripts::segments(&db.reader().unwrap(), &tid).unwrap();
    assert_eq!(m.segment_id, segs[2].id);
    assert_eq!(m.snippet.field, Field::Transcript);
    assert!(m
        .snippet
        .parts
        .iter()
        .any(|p| p.highlight && p.text == "yedeklemeyi"));
}

#[test]
fn moments_are_in_time_order_with_turkish_folding_and_original_text() {
    let (_d, mut db) = open();
    let id = item(&mut db, "https://example.com/a", "Ders 4", 1);
    save(
        &mut db,
        &captions(&id, "tr"),
        Status::Ready,
        None,
        &lecture(),
        5,
    );

    // Prefix search as the user types: both chunks match "replikasyon*".
    let tr = search(&db, "replikasyon").hits[0]
        .transcript
        .clone()
        .unwrap();
    assert_eq!(tr.total, 2);
    let starts: Vec<i64> = tr.moments.iter().map(|m| m.start_ms).collect();
    assert_eq!(starts, vec![2_000, 61_000]);

    // `ı` and `I` fold to `i`; the snippet shows the words as written.
    let tr = search(&db, "ışık").hits[0].transcript.clone().unwrap();
    let text = tr.moments[0].snippet.text();
    assert!(text.contains("ışık hızında"), "{text}");
    let tr = search(&db, "ISIK").hits[0].transcript.clone().unwrap();
    assert_eq!(tr.moments[0].start_ms, 4_500);
}

#[test]
fn only_the_first_moments_are_listed_and_all_are_counted() {
    let (_d, mut db) = open();
    let id = item(&mut db, "https://example.com/a", "Uzun ders", 1);
    // One chunk per segment: each starts more than 30 s after the last.
    let segs: Vec<NewSegment> = (0..9)
        .map(|i| NewSegment::new(i * 40_000, i * 40_000 + 3_000, format!("adım {i} indeks")))
        .collect();
    save(&mut db, &captions(&id, "tr"), Status::Ready, None, &segs, 5);
    let tr = search(&db, "indeks").hits[0].transcript.clone().unwrap();
    assert_eq!(tr.total, 9);
    assert_eq!(tr.moments.len(), MAX_MOMENTS);
    assert_eq!(tr.moments[0].start_ms, 0);
    assert_eq!(tr.moments[4].start_ms, 160_000);
}

#[test]
fn metadata_and_transcript_together_rank_first() {
    let (_d, mut db) = open();
    let spoken = item(&mut db, "https://example.com/a", "Ders 4", 3);
    let titled = item(&mut db, "https://example.com/b", "Replikasyon rehberi", 1);
    let both = item(&mut db, "https://example.com/c", "Replikasyon dersi", 2);
    save(
        &mut db,
        &captions(&spoken, "tr"),
        Status::Ready,
        None,
        &lecture(),
        5,
    );
    save(
        &mut db,
        &captions(&both, "tr"),
        Status::Ready,
        None,
        &lecture(),
        5,
    );

    let page = search(&db, "replikasyon");
    assert_eq!(page.total, 3);
    let order: Vec<&str> = page.hits.iter().map(|h| h.item.id.as_str()).collect();
    assert_eq!(order[0], both, "title and transcript");
    assert_eq!(order[1], titled, "a title beats words said once");
    assert_eq!(order[2], spoken);
    let both_hit = &page.hits[0];
    assert_eq!(both_hit.matched, vec![Field::Title, Field::Transcript]);
    assert_eq!(both_hit.snippet.as_ref().unwrap().field, Field::Title);
    assert!(page.hits[1].transcript.is_none());
}

#[test]
fn one_transcript_per_item_ready_before_partial() {
    let (_d, mut db) = open();
    let id = item(&mut db, "https://example.com/a", "Ders 4", 1);
    let part = save(
        &mut db,
        &local(&id, "tr"),
        Status::Partial,
        Some(5_000),
        &lecture()[..3],
        9,
    );
    // Only the partial one yet: it is searched and says how far it goes.
    let tr = search(&db, "replikasyon").hits[0]
        .transcript
        .clone()
        .unwrap();
    assert_eq!(tr.transcript_id, part);
    assert!(tr.partial);
    assert_eq!(tr.covered_ms, Some(5_000));

    // Site captions arrive: the same words are not listed twice.
    let ready = save(
        &mut db,
        &captions(&id, "tr"),
        Status::Ready,
        None,
        &lecture(),
        5,
    );
    let page = search(&db, "replikasyon");
    assert_eq!(page.total, 1);
    let tr = page.hits[0].transcript.clone().unwrap();
    assert_eq!(tr.transcript_id, ready);
    assert!(!tr.partial);
    assert_eq!(tr.total, 2);
}

#[test]
fn stale_failed_and_deleted_transcripts_are_not_searched() {
    let (_d, mut db) = open();
    let id = item(&mut db, "https://example.com/a", "Ders 4", 1);
    let tid = save(
        &mut db,
        &captions(&id, "tr"),
        Status::Ready,
        None,
        &lecture(),
        5,
    );
    assert_eq!(search(&db, "replikasyon").total, 1);
    for s in [Status::Stale, Status::Failed, Status::Canceled] {
        transcripts::set_status(db.writer(), &tid, s, 6).unwrap();
        assert_eq!(search(&db, "replikasyon").total, 0, "{s:?}");
    }
    transcripts::set_status(db.writer(), &tid, Status::Ready, 7).unwrap();
    transcripts::delete(db.writer(), &tid).unwrap();
    assert_eq!(search(&db, "replikasyon").total, 0);
}

#[test]
fn filters_apply_to_transcript_matches() {
    let (_d, mut db) = open();
    let id = item(&mut db, "https://example.com/a", "Ders 4", 1);
    save(
        &mut db,
        &captions(&id, "tr"),
        Status::Ready,
        None,
        &lecture(),
        5,
    );
    let only_favorites = Filters {
        favorite_only: true,
        ..Default::default()
    };
    assert_eq!(search_with(&db, "replikasyon", only_favorites).total, 0);
    let youtube = Filters {
        platforms: vec![SourcePlatform::Youtube],
        ..Default::default()
    };
    assert_eq!(search_with(&db, "replikasyon", youtube).total, 0);
}

#[test]
fn search_without_text_lists_no_moments() {
    let (_d, mut db) = open();
    let id = item(&mut db, "https://example.com/a", "Ders 4", 1);
    save(
        &mut db,
        &captions(&id, "tr"),
        Status::Ready,
        None,
        &lecture(),
        5,
    );
    let page = search(&db, "");
    assert_eq!(page.total, 1);
    assert!(page.hits[0].transcript.is_none());
    assert!(page.hits[0].matched.is_empty());
}

/// NFR-04 timing on 100.000 segments (run by hand:
/// `cargo test --release -p vv-persistence --test transcript_search -- --ignored --nocapture`).
#[test]
#[ignore]
fn nfr04_hundred_thousand_segments() {
    let (_d, mut db) = open();
    let words = [
        "veri",
        "tabanı",
        "replikasyon",
        "indeks",
        "sorgu",
        "gecikme",
        "yedek",
        "ışık",
        "network",
        "latency",
        "backup",
        "query",
        "server",
        "disk",
        "memory",
        "cache",
    ];
    let mut n = 0u64;
    let mut next = || {
        n = n
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (n >> 33) as usize
    };
    for i in 0..1_000 {
        let id = item(
            &mut db,
            &format!("https://example.com/{i}"),
            &format!("Video {i}"),
            i,
        );
        let segs: Vec<NewSegment> = (0..100)
            .map(|k| {
                let text = (0..8)
                    .map(|_| words[next() % words.len()])
                    .collect::<Vec<_>>()
                    .join(" ");
                NewSegment::new(k * 4_000, k * 4_000 + 3_500, text)
            })
            .collect();
        save(&mut db, &captions(&id, "tr"), Status::Ready, None, &segs, 5);
    }
    for i in 1_000..10_000 {
        item(
            &mut db,
            &format!("https://example.com/{i}"),
            &format!("Başlık {i}"),
            i,
        );
    }
    let mut times = Vec::new();
    for q in [
        "replikasyon",
        "ışık",
        "gecikme yedek",
        "la",
        "\"disk memory\"",
        "Video",
    ] {
        for _ in 0..5 {
            let t = std::time::Instant::now();
            let p = search(&db, q);
            times.push(t.elapsed().as_secs_f64() * 1000.0);
            assert!(p.total > 0, "{q}");
        }
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p95 = times[(times.len() * 95 / 100).min(times.len() - 1)];
    println!(
        "NFR-04: {} searches, p50 {:.1} ms, p95 {:.1} ms",
        times.len(),
        times[times.len() / 2],
        p95
    );
}

#[test]
fn a_moment_is_found_again_until_its_transcript_changes() {
    let (_d, mut db) = open();
    let id = item(&mut db, "https://example.com/a", "Ders 4", 1);
    let o = captions(&id, "tr");
    let tid = save(&mut db, &o, Status::Ready, None, &lecture(), 5);
    let m = search(&db, "yedeklemeyi").hits[0]
        .transcript
        .clone()
        .unwrap()
        .moments[0]
        .clone();
    let place = transcripts::segment_place(&db.reader().unwrap(), &m.segment_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        (place.item_id.as_str(), place.transcript_id.as_str()),
        (id.as_str(), tid.as_str())
    );
    assert_eq!((place.start_ms, place.status), (4_500, Status::Ready));
    // Made again: the old segment ids are gone, so an old result says so.
    save(&mut db, &o, Status::Ready, None, &lecture(), 6);
    assert!(
        transcripts::segment_place(&db.reader().unwrap(), &m.segment_id)
            .unwrap()
            .is_none()
    );
}
