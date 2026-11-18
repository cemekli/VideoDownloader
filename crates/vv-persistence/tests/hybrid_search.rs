//! STATE.md F7.2, SPEC SEA-07, §16.3, AC-40 (ADR-0036): word search and
//! search by meaning fused with reciprocal rank fusion. Vectors here are
//! hand-made unit vectors (one dimension per topic), so the meaning side is
//! exact and the tests check only the fusion, the filters and what each
//! hit says about why it was found.

use vv_persistence::library::hybrid::{self, MeaningQuery, MIN_SIMILARITY};
use vv_persistence::library::repo::{self, LibraryItem};
use vv_persistence::library::search::*;
use vv_persistence::library::*;
use vv_persistence::transcripts::{self, CaptionKind, NewSegment, Origin, Provenance, Status};
use vv_persistence::vectors::{self, Generation, Producer};
use vv_persistence::{Database, OpenOptions};

const DIMS: usize = 8;

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

/// A caption transcript with one segment per line, 40 s apart (one chunk
/// each).
fn said(db: &mut Database, item_id: &str, lines: &[&str]) {
    let o = Origin {
        item_id: item_id.to_owned(),
        asset_id: None,
        language: "tr".into(),
        provenance: Provenance::SourceCaptions,
        provider: "source_captions".into(),
        model_id: None,
        model_version: None,
        source_fingerprint: "yt-dlp:subtitles:tr".into(),
        caption_kind: Some(CaptionKind::Manual),
    };
    let segs: Vec<NewSegment> = lines
        .iter()
        .enumerate()
        .map(|(i, t)| NewSegment::new(i as i64 * 40_000, i as i64 * 40_000 + 3_000, *t))
        .collect();
    let tx = db.writer().transaction().unwrap();
    transcripts::save(&tx, &o, Status::Ready, None, &segs, None, 5).unwrap();
    tx.commit().unwrap();
}

/// A vector `sim` close to topic `i` (cosine with `topic(i)` is `sim`);
/// the rest goes to the last dimension, which no topic uses.
fn near(i: usize, sim: f32) -> Vec<f32> {
    let mut v = vec![0.0; DIMS];
    v[i] = sim;
    v[DIMS - 1] += (1.0 - sim * sim).max(0.0).sqrt();
    v
}

fn topic(i: usize) -> Vec<f32> {
    near(i, 1.0)
}

/// An active index where each chunk gets the vector `f(text)`.
fn index(db: &mut Database, f: impl Fn(&str) -> Vec<f32>) -> Generation {
    let c = db.writer();
    let p = Producer {
        provider: "openai_compatible".into(),
        model_id: "bge-m3".into(),
        model_version: "http://localhost:11434/v1".into(),
        dims: DIMS,
    };
    let g = vectors::begin_build(c, &p, "gen-1", 10).unwrap();
    for ch in vectors::pending(c, g.seq, 1000).unwrap() {
        vectors::put(c, &g, ch.chunk_seq, &ch.text, &f(&ch.text)).unwrap();
    }
    vectors::activate(c, "gen-1", 11).unwrap()
}

/// Topic 0: databases; 1: bread; 2: running; 3: anything else.
fn topic_of(text: &str) -> Vec<f32> {
    let t = text.to_lowercase();
    if t.contains("postgresql") || t.contains("pg_dump") {
        topic(0)
    } else if t.contains("maya") || t.contains("sourdough") {
        topic(1)
    } else if t.contains("koşu") || t.contains("marathon") {
        topic(2)
    } else {
        topic(3)
    }
}

fn query(text: &str) -> SearchQuery {
    SearchQuery {
        text: text.into(),
        sort: Sort {
            field: SortField::Relevance,
            descending: true,
        },
        ..Default::default()
    }
}

fn run(db: &Database, q: &SearchQuery, g: Option<&Generation>, v: &[f32]) -> SearchPage {
    let c = db.reader().unwrap();
    let m = g.map(|g| MeaningQuery {
        generation: g,
        vector: v,
    });
    hybrid::search_hybrid(&c, q, m.as_ref()).unwrap()
}

fn titles(p: &SearchPage) -> Vec<String> {
    p.hits.iter().map(|h| h.item.display_title()).collect()
}

/// Three videos: one says "yedekleme" (word match) and is about databases,
/// one is about databases in other words, one is about bread.
fn library() -> (tempfile::TempDir, Database, Generation) {
    let (d, mut db) = open();
    let a = item(&mut db, "https://e.com/a", "Ders A", 1);
    said(&mut db, &a, &["Bugün yedekleme ve PostgreSQL konuşacağız."]);
    let b = item(&mut db, "https://e.com/b", "Ders B", 2);
    said(
        &mut db,
        &b,
        &[
            "Önce selam.",
            "WAL kayıtlarını ikinci PostgreSQL sunucusuna akıtıyoruz.",
        ],
    );
    let c = item(&mut db, "https://e.com/c", "Ders C", 3);
    said(&mut db, &c, &["Ekşi maya ile ekmek."]);
    let g = index(&mut db, topic_of);
    (d, db, g)
}

#[test]
fn words_and_meaning_are_fused_and_each_hit_says_why() {
    let (_d, db, g) = library();
    // "yedekleme" (backup): only A says it; A and B are about databases.
    let p = run(&db, &query("yedekleme"), Some(&g), &topic(0));
    assert_eq!(titles(&p), ["Ders A", "Ders B"]);
    assert_eq!(p.total, 2);
    let a = &p.hits[0];
    assert_eq!(a.matched, [Field::Transcript, Field::Meaning]);
    assert_eq!(
        a.fusion,
        Some(Fusion {
            words: Some(1),
            meaning: Some(1)
        })
    );
    let b = &p.hits[1];
    assert_eq!(b.matched, [Field::Meaning], "found by meaning only");
    assert!(b.transcript.is_none() && b.snippet.is_none());
    assert_eq!(
        b.fusion,
        Some(Fusion {
            words: None,
            meaning: Some(2)
        })
    );
    // The passage opens at its segment (PLAY-06), shown whole, unmarked.
    let m = b.meaning.as_ref().unwrap();
    assert_eq!(m.total, 1);
    assert_eq!(m.moments[0].start_ms, 40_000);
    assert!(m.moments[0].snippet.text().starts_with("WAL kayıtlarını"));
    assert!(m.moments[0].snippet.parts.iter().all(|p| !p.highlight));
    assert_eq!(m.moments[0].snippet.field, Field::Meaning);
}

#[test]
fn an_item_found_both_ways_ranks_above_one_found_one_way() {
    let (_d, mut db) = open();
    // X: title says "ekmek" only. Y: says "ekmek" and is about bread.
    let x = item(&mut db, "https://e.com/x", "ekmek fiyatları", 9);
    said(&mut db, &x, &["Ekonomi haberleri."]);
    let y = item(&mut db, "https://e.com/y", "Hafta sonu", 1);
    said(&mut db, &y, &["Ekşi maya ile ekmek yapıyoruz."]);
    let g = index(&mut db, topic_of);
    // Words alone: the title match is first (title weight).
    assert_eq!(
        titles(&run(&db, &query("ekmek"), None, &[])),
        ["ekmek fiyatları", "Hafta sonu"]
    );
    // Hybrid: Y is found both ways and moves up.
    assert_eq!(
        titles(&run(&db, &query("ekmek"), Some(&g), &topic(1))),
        ["Hafta sonu", "ekmek fiyatları"]
    );
}

#[test]
fn passages_below_the_similarity_floor_do_not_match() {
    let (_d, db, g) = library();
    // A query about running: nothing is about running, the nearest chunks
    // are only weakly similar.
    let weak = MIN_SIMILARITY - 0.05;
    let p = run(&db, &query("maraton"), Some(&g), &near(0, weak));
    assert_eq!(p.total, 0, "{:?}", titles(&p));
    let p = run(
        &db,
        &query("maraton"),
        Some(&g),
        &near(0, MIN_SIMILARITY + 0.05),
    );
    assert_eq!(titles(&p), ["Ders A", "Ders B"]);
}

#[test]
fn filters_and_views_apply_to_meaning_matches() {
    let (_d, mut db, g) = library();
    let b = {
        let c = db.reader().unwrap();
        let p = hybrid::search_hybrid(
            &c,
            &query("yedekleme"),
            Some(&MeaningQuery {
                generation: &g,
                vector: &topic(0),
            }),
        )
        .unwrap();
        p.hits[1].item.id.clone()
    };
    // Archived items are hidden in normal views, also when found by meaning.
    db.writer()
        .execute(
            "UPDATE library_item SET archived_at = 5 WHERE id = ?1",
            [&b],
        )
        .unwrap();
    assert_eq!(
        titles(&run(&db, &query("yedekleme"), Some(&g), &topic(0))),
        ["Ders A"]
    );
    let mut q = query("yedekleme");
    q.filters.archived = ArchiveFilter::Only;
    assert_eq!(titles(&run(&db, &q, Some(&g), &topic(0))), ["Ders B"]);
    // A removed item never shows in the live library.
    db.writer()
        .execute(
            "UPDATE library_item SET archived_at = NULL, removed_at = 6 WHERE id = ?1",
            [&b],
        )
        .unwrap();
    assert_eq!(
        titles(&run(&db, &query("yedekleme"), Some(&g), &topic(0))),
        ["Ders A"]
    );
}

#[test]
fn without_a_query_vector_or_with_another_sort_it_is_the_word_search() {
    let (_d, db, g) = library();
    let words = run(&db, &query("yedekleme"), None, &[]);
    assert_eq!(titles(&words), ["Ders A"]);
    assert!(words.hits[0].fusion.is_none() && words.hits[0].meaning.is_none());
    // Newest first: the sort the user chose is kept, meaning is not used.
    let mut q = query("yedekleme");
    q.sort = Sort::default();
    assert_eq!(titles(&run(&db, &q, Some(&g), &topic(0))), ["Ders A"]);
    // Nothing searchable: the filtered library, with the notice (SEA-04).
    let p = run(&db, &query("' * ("), Some(&g), &topic(0));
    assert_eq!(p.total, 3);
    assert_eq!(p.notice, Some(QueryNotice::NoSearchableTerms));
}

#[test]
fn hybrid_pages_are_stable_and_complete() {
    let (_d, mut db) = open();
    for k in 0..30 {
        let id = item(
            &mut db,
            &format!("https://e.com/{k}"),
            &format!("Video {k:02}"),
            k,
        );
        if k % 2 == 0 {
            said(&mut db, &id, &["PostgreSQL yedekleme"]);
        } else {
            said(&mut db, &id, &[&format!("pg_dump ile gece {k}")]);
        }
    }
    let g = index(&mut db, topic_of);
    let mut q = query("yedekleme");
    let all = run(&db, &q, Some(&g), &topic(0));
    assert_eq!(all.total, 30);
    let mut seen = Vec::new();
    q.limit = 7;
    for page in 0..5 {
        q.offset = page * 7;
        seen.extend(titles(&run(&db, &q, Some(&g), &topic(0))));
    }
    assert_eq!(seen, titles(&all));
    // Found both ways first: the 15 that say "yedekleme".
    assert!(all.hits[..15]
        .iter()
        .all(|h| h.fusion.unwrap().words.is_some()));
}

#[test]
fn a_meaning_match_lists_its_closest_passages_in_time_order() {
    let (_d, mut db) = open();
    let id = item(&mut db, "https://e.com/l", "Uzun ders", 1);
    let lines: Vec<String> = (0..8).map(|k| format!("PostgreSQL bölüm {k}")).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    said(&mut db, &id, &refs);
    // Later passages are closer.
    let g = index(&mut db, |t| {
        let k: f32 = t.rsplit(' ').next().unwrap().parse().unwrap();
        near(0, 0.6 + k * 0.05)
    });
    let p = run(&db, &query("yedekleme"), Some(&g), &topic(0));
    let m = p.hits[0].meaning.as_ref().unwrap();
    assert_eq!(m.total, 8);
    let starts: Vec<i64> = m.moments.iter().map(|x| x.start_ms).collect();
    assert_eq!(starts, [120_000, 160_000, 200_000, 240_000, 280_000]);
}
