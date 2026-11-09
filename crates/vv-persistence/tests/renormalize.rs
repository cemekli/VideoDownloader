//! STATE.md F1.3, SPEC §11.7: AC-44 — after a rule change two items fall on
//! the same canonical value; the oldest takes it, the other keeps its old
//! value and becomes a duplicate candidate, and new captures go to the oldest.

use rusqlite::{params, Connection};
use vv_persistence::library::bookmark::{save_bookmark, SaveOutcome};
use vv_persistence::library::renormalize::{renormalize, RenormalizeReport};
use vv_persistence::library::repo;
use vv_persistence::library::*;
use vv_persistence::{Database, OpenOptions};

/// A row as an app without normalization (version 0) would have stored it.
fn raw_v0(c: &Connection, url: &str, added_at: i64) -> String {
    let id = new_id();
    c.execute(
        "INSERT INTO library_item (id, kind, canonical_url, original_url, normalization_version, source_platform, added_at, updated_at) \
         VALUES (?1, 'bookmark', ?2, ?2, 0, 'generic', ?3, ?3)",
        params![id, url, added_at],
    )
    .unwrap();
    let it = repo::get_item(c, &id).unwrap().unwrap();
    repo::write_search_document(c, &it).unwrap();
    id
}

const Y2025: i64 = 1_735_689_600_000;
const Y2026: i64 = 1_767_225_600_000;

#[test]
fn ac44_oldest_item_takes_the_new_value_and_the_other_becomes_a_candidate() {
    let d = tempfile::tempdir().unwrap();
    let mut db = Database::open(&d.path().join("library.sqlite"), &OpenOptions::default()).unwrap();
    let c = db.writer();
    // Inserted newest first so ordering by insertion would pick the wrong one.
    let b = raw_v0(c, "https://youtu.be/vvTestVid44", Y2026);
    let a = raw_v0(c, "https://www.youtube.com/watch?v=vvTestVid44&si=1", Y2025);
    let generic = raw_v0(c, "https://example.com/x#frag", Y2025 + 1);
    let fine = raw_v0(c, "https://example.com/already", Y2025 + 2);

    let clock = FixedClock::new(Y2026 + 10);
    let report = renormalize(c, &clock).unwrap();
    assert_eq!(
        report,
        RenormalizeReport {
            updated: 2,
            unchanged: 1,
            conflicts: 1,
            skipped: 0
        }
    );

    let a_row = repo::get_item(c, &a).unwrap().unwrap();
    assert_eq!(
        a_row.canonical_url.as_deref(),
        Some("https://www.youtube.com/watch?v=vvTestVid44")
    );
    assert_eq!(
        (a_row.source_platform, a_row.source_id.as_deref()),
        (SourcePlatform::Youtube, Some("vvTestVid44"))
    );
    assert_eq!(
        (a_row.normalization_version, a_row.normalization_conflict),
        (vv_domain::normalize::NORMALIZATION_VERSION, false)
    );

    let b_row = repo::get_item(c, &b).unwrap().unwrap();
    assert_eq!(
        b_row.canonical_url.as_deref(),
        Some("https://youtu.be/vvTestVid44"),
        "keeps its old value"
    );
    assert_eq!(
        (b_row.normalization_version, b_row.normalization_conflict),
        (0, true)
    );
    assert_eq!(b_row.source_id, None);

    let dups = repo::open_duplicates_for_item(c, &a).unwrap();
    assert_eq!(dups.len(), 1);
    assert_eq!(dups[0].reason, DuplicateReason::NormalizationConflict);
    assert_eq!(
        (dups[0].item_a_id.as_str(), dups[0].item_b_id.as_str()),
        (a.as_str(), b.as_str())
    );

    assert_eq!(
        repo::get_item(c, &generic)
            .unwrap()
            .unwrap()
            .canonical_url
            .as_deref(),
        Some("https://example.com/x")
    );
    let fine_row = repo::get_item(c, &fine).unwrap().unwrap();
    assert_eq!(
        (
            fine_row.canonical_url.as_deref(),
            fine_row.normalization_version
        ),
        (
            Some("https://example.com/already"),
            vv_domain::normalize::NORMALIZATION_VERSION
        )
    );
    // Both stay findable; neither is merged or hidden.
    let hits = repo::search_phrase_ids(c, "vvTestVid44").unwrap();
    assert!(hits.contains(&a) && hits.contains(&b), "{hits:?}");

    // A new capture goes to the item holding the new value, with the candidate.
    match save_bookmark(c, &clock, "https://youtu.be/vvTestVid44?si=again")
        .unwrap()
        .outcome
    {
        SaveOutcome::Existing {
            item,
            open_duplicates,
            ..
        } => {
            assert_eq!(item.id, a);
            assert_eq!(open_duplicates, vec![b.clone()]);
        }
        o => panic!("{o:?}"),
    }

    // The pass is complete: a second run has nothing left to do.
    assert_eq!(
        renormalize(c, &clock).unwrap(),
        RenormalizeReport::default()
    );
    assert_eq!(
        repo::open_duplicates_for_item(c, &b).unwrap().len(),
        1,
        "no second candidate"
    );
    assert_eq!(
        repo::count_items(c).unwrap(),
        4,
        "nothing merged automatically"
    );
}

/// ADR-0011 edge case: a stale holder already equal to the new value keeps
/// it, because "keep the old value" would break the unique index.
#[test]
fn stale_holder_with_the_new_value_already_keeps_it() {
    let d = tempfile::tempdir().unwrap();
    let mut db = Database::open(&d.path().join("library.sqlite"), &OpenOptions::default()).unwrap();
    let c = db.writer();
    let older = raw_v0(c, "https://www.youtube.com/watch?v=vvTestVid45&si=1", Y2025);
    let holder = raw_v0(c, "https://www.youtube.com/watch?v=vvTestVid45", Y2026);
    let report = renormalize(c, &FixedClock::new(Y2026 + 1)).unwrap();
    assert_eq!(
        (report.conflicts, report.updated),
        (1, 1),
        "holder gains its source id"
    );
    assert!(
        repo::get_item(c, &older)
            .unwrap()
            .unwrap()
            .normalization_conflict
    );
    let h = repo::get_item(c, &holder).unwrap().unwrap();
    assert_eq!(
        (h.normalization_version, h.source_id.as_deref()),
        (
            vv_domain::normalize::NORMALIZATION_VERSION,
            Some("vvTestVid45")
        )
    );
    assert_eq!(repo::open_duplicates_for_item(c, &older).unwrap().len(), 1);
}

#[test]
fn unusable_stored_urls_are_skipped_not_dropped() {
    let d = tempfile::tempdir().unwrap();
    let mut db = Database::open(&d.path().join("library.sqlite"), &OpenOptions::default()).unwrap();
    let c = db.writer();
    let bad = raw_v0(c, "not a url", 1);
    let report = renormalize(c, &FixedClock::new(2)).unwrap();
    assert_eq!(report.skipped, 1);
    let row = repo::get_item(c, &bad).unwrap().unwrap();
    assert_eq!(
        (row.canonical_url.as_deref(), row.normalization_version),
        (Some("not a url"), 0)
    );
}

#[test]
fn a_dailymotion_bookmark_saved_as_web_becomes_dailymotion() {
    let d = tempfile::tempdir().unwrap();
    let mut db = Database::open(&d.path().join("library.sqlite"), &OpenOptions::default()).unwrap();
    let c = db.writer();
    let id = raw_v0(c, "https://www.dailymotion.com/video/x8vvtst", 1);
    let report = renormalize(c, &FixedClock::new(2)).unwrap();
    assert_eq!(report.updated, 1);
    let row = repo::get_item(c, &id).unwrap().unwrap();
    assert_eq!(
        (row.source_platform, row.source_id.as_deref()),
        (SourcePlatform::Dailymotion, Some("x8vvtst"))
    );
}
