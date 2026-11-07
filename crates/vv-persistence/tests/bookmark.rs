//! STATE.md F1.2: Save Bookmark (SPEC §7.1 CAP-02…CAP-06, CAP-08, §11.7).
//! Covers AC-01, AC-02, AC-03, AC-36, AC-52 and the AC-45 "already removed"
//! signal. Nothing here touches the network: capture is offline by design.

use std::path::Path;
use std::sync::{Arc, Barrier};
use vv_domain::error::ErrorCode;
use vv_persistence::library::bookmark::{save_bookmark, MatchedBy, SaveError, SaveOutcome};
use vv_persistence::library::repo::{self, LibraryItem, UrlAlias};
use vv_persistence::library::*;
use vv_persistence::{Database, OpenOptions};

fn open(dir: &Path) -> Database {
    Database::open(&dir.join("library.sqlite"), &OpenOptions::default()).unwrap()
}

/// Every byte the database left on disk (main file, WAL, shared memory).
fn disk_contains(dir: &Path, needle: &str) -> bool {
    std::fs::read_dir(dir).unwrap().any(|e| {
        let bytes = std::fs::read(e.unwrap().path()).unwrap();
        bytes.windows(needle.len()).any(|w| w == needle.as_bytes())
    })
}

#[test]
fn ac01_offline_save_is_pending_searchable_and_survives_restart() {
    let d = tempfile::tempdir().unwrap();
    let clock = FixedClock::new(1_762_500_000_000);
    let id = {
        let mut db = open(d.path());
        let r = save_bookmark(
            db.writer(),
            &clock,
            "  https://youtu.be/vvTestVid01?si=share  ",
        )
        .unwrap();
        assert!(r.warnings.is_empty());
        let SaveOutcome::Created(it) = r.outcome else {
            panic!("{:?}", r.outcome)
        };
        assert_eq!(it.kind, ItemKind::Bookmark);
        assert_eq!(it.metadata_status, MetadataStatus::Pending);
        assert_eq!(it.title, None, "no invented platform metadata (CAP-04)");
        assert_eq!(it.display_title(), "YouTube · vvTestVid01");
        it.id
    };
    // Restart: a new process opens the same file.
    let db = open(d.path());
    let reader = db.reader().unwrap();
    let it = repo::get_item(&reader, &id)
        .unwrap()
        .expect("item after restart");
    assert_eq!(
        it.canonical_url.as_deref(),
        Some("https://www.youtube.com/watch?v=vvTestVid01")
    );
    assert_eq!(
        it.original_url.as_deref(),
        Some("https://youtu.be/vvTestVid01?si=share")
    );
    assert_eq!(
        (it.source_platform, it.source_id.as_deref()),
        (SourcePlatform::Youtube, Some("vvTestVid01"))
    );
    assert_eq!(
        it.normalization_version,
        vv_domain::normalize::NORMALIZATION_VERSION
    );
    assert_eq!(it.added_at, 1_762_500_000_000);
    assert_eq!(
        repo::search_phrase_ids(&reader, "vvTestVid01").unwrap(),
        vec![id.clone()]
    );
    assert_eq!(
        repo::search_phrase_ids(&reader, "youtube").unwrap(),
        vec![id]
    );
}

#[test]
fn ac02_same_content_twice_returns_the_existing_item_with_its_notes() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let clock = FixedClock::new(1);
    let first = save_bookmark(
        db.writer(),
        &clock,
        "https://www.youtube.com/watch?v=vvTestVid01",
    )
    .unwrap();
    let id = first.outcome.item().id.clone();
    repo::set_notes(db.writer(), &id, 1, "önemli not", 2).unwrap();

    for variant in [
        "https://www.youtube.com/watch?v=vvTestVid01",
        "https://youtu.be/vvTestVid01",
        "https://m.youtube.com/watch?v=vvTestVid01&t=30s&si=x",
        "https://www.youtube.com/shorts/vvTestVid01",
    ] {
        clock.advance(1);
        match save_bookmark(db.writer(), &clock, variant).unwrap().outcome {
            SaveOutcome::Existing {
                item, matched_by, ..
            } => {
                assert_eq!(item.id, id, "{variant}");
                assert_eq!(matched_by, MatchedBy::CanonicalUrl);
                assert_eq!(item.user_notes, "önemli not");
                assert_eq!(item.revision, 2, "lookup writes nothing");
            }
            o => panic!("{variant}: {o:?}"),
        }
    }
    assert_eq!(repo::count_items(db.writer()).unwrap(), 1);
}

#[test]
fn ac02_match_by_source_id_and_by_alias() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    // An item whose stored URL differs but whose platform identity matches.
    let mut old = LibraryItem::new(ItemKind::RemoteVideo, SourcePlatform::Youtube, 1);
    old.canonical_url = Some("https://www.youtube.com/watch?v=vvTestVid02&feature=legacy".into());
    old.source_id = Some("vvTestVid02".into());
    old.normalization_version = 1;
    repo::insert_item(c, &old).unwrap();
    let clock = FixedClock::new(5);
    match save_bookmark(c, &clock, "https://youtu.be/vvTestVid02")
        .unwrap()
        .outcome
    {
        SaveOutcome::Existing {
            item,
            matched_by: MatchedBy::SourceId,
            ..
        } => assert_eq!(item.id, old.id),
        o => panic!("{o:?}"),
    }
    // A merged-away URL lives on as an alias of the target (§11.7, ORG-11).
    let target = save_bookmark(c, &clock, "https://example.com/clip")
        .unwrap()
        .outcome
        .item()
        .clone();
    repo::insert_url_alias(
        c,
        &UrlAlias {
            id: new_id(),
            item_id: target.id.clone(),
            url: "https://example.com/old-clip".into(),
            normalization_version: 1,
            created_at: 6,
        },
    )
    .unwrap();
    match save_bookmark(c, &clock, "https://example.com/old-clip#t=3")
        .unwrap()
        .outcome
    {
        SaveOutcome::Existing {
            item,
            matched_by: MatchedBy::Alias,
            ..
        } => assert_eq!(item.id, target.id),
        o => panic!("{o:?}"),
    }
    assert_eq!(repo::count_items(c).unwrap(), 2);
}

#[test]
fn ac03_concurrent_saves_of_the_same_url_end_in_one_item() {
    let d = tempfile::tempdir().unwrap();
    drop(open(d.path())); // create and migrate once
    const N: usize = 8;
    let barrier = Arc::new(Barrier::new(N));
    let dir = d.path().to_owned();
    let handles: Vec<_> = (0..N)
        .map(|i| {
            let barrier = barrier.clone();
            let dir = dir.clone();
            std::thread::spawn(move || {
                let mut db = open(&dir);
                let clock = FixedClock::new(1_000 + i as i64);
                // Different spellings of the same video, all at once.
                let url = if i % 2 == 0 {
                    "https://youtu.be/vvTestVid03"
                } else {
                    "https://www.youtube.com/watch?v=vvTestVid03&si=r"
                };
                barrier.wait();
                save_bookmark(db.writer(), &clock, url).unwrap().outcome
            })
        })
        .collect();
    let outcomes: Vec<SaveOutcome> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let created = outcomes.iter().filter(|o| o.is_created()).count();
    assert_eq!(created, 1, "exactly one save creates the item");
    let id = &outcomes.iter().find(|o| o.is_created()).unwrap().item().id;
    assert!(
        outcomes.iter().all(|o| &o.item().id == id),
        "everyone sees the same item"
    );
    let mut db = open(&dir);
    assert_eq!(repo::count_items(db.writer()).unwrap(), 1);
    let docs: i64 = db
        .writer()
        .query_row("SELECT count(*) FROM search_document", [], |r| r.get(0))
        .unwrap();
    assert_eq!(docs, 1, "item and search document land together");
}

#[test]
fn ac36_signature_removed_by_platform_rule_never_reaches_disk() {
    let d = tempfile::tempdir().unwrap();
    let canary = "CANARYSIG36value";
    {
        let mut db = open(d.path());
        let clock = FixedClock::new(1);
        let input = format!(
            "https://vkvideo.ru/video-12345_67890?access_key={canary}&list=x&signature={canary}"
        );
        let r = save_bookmark(db.writer(), &clock, &input).unwrap();
        assert!(
            !format!("{r:?}").contains(canary),
            "result (what UI/log sees) is clean"
        );
        let it = r.outcome.item();
        assert_eq!(
            it.canonical_url.as_deref(),
            Some("https://vk.com/video-12345_67890")
        );
        assert_eq!(
            it.original_url.as_deref(),
            Some("https://vkvideo.ru/video-12345_67890?access_key=REDACTED&list=x&signature=REDACTED")
        );
        assert!(!disk_contains(d.path(), canary), "WAL before checkpoint");
    }
    assert!(!disk_contains(d.path(), canary), "files after close");
}

#[test]
fn ac52_secret_without_canonical_form_is_refused_and_not_stored() {
    let d = tempfile::tempdir().unwrap();
    let canary = "CANARYSECRET52";
    let mut db = open(d.path());
    let clock = FixedClock::new(1);
    for input in [
        format!("https://cdn.example.com/v.mp4?Expires=1&Signature={canary}&Key-Pair-Id=K"),
        format!("https://files.example.com/v?token={canary}"),
        format!("https://bucket.example.com/v.mp4?X-Amz-Signature={canary}"),
    ] {
        let e = save_bookmark(db.writer(), &clock, &input).unwrap_err();
        assert!(
            matches!(e, SaveError::Rejected(ErrorCode::UrlHasCredentials)),
            "{e:?}"
        );
        assert_eq!(e.code(), ErrorCode::UrlHasCredentials);
        assert!(!format!("{e:?}").contains(canary));
    }
    assert_eq!(repo::count_items(db.writer()).unwrap(), 0);
    drop(db);
    assert!(!disk_contains(d.path(), canary));
}

#[test]
fn policy_rejections_write_nothing() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let clock = FixedClock::new(1);
    for (input, code) in [
        ("not a url", ErrorCode::InvalidUrl),
        ("file:///C:/x.mp4", ErrorCode::InvalidUrl),
        (
            "https://user:pw@example.com/v",
            ErrorCode::UrlHasCredentials,
        ),
        ("http://127.0.0.1/v", ErrorCode::UrlBlocked),
    ] {
        assert_eq!(
            save_bookmark(db.writer(), &clock, input)
                .unwrap_err()
                .code(),
            code,
            "{input}"
        );
    }
    assert_eq!(repo::count_items(db.writer()).unwrap(), 0);
}

#[test]
fn playlist_is_one_bookmark_with_a_warning() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let r = save_bookmark(
        db.writer(),
        &FixedClock::new(1),
        "https://www.youtube.com/playlist?list=PLvvTest",
    )
    .unwrap();
    assert_eq!(r.warnings, vec![ErrorCode::PlaylistUnsupported]);
    let it = r.outcome.item();
    assert_eq!(
        (it.source_id.as_deref(), it.display_title().as_str()),
        (None, "YouTube · liste")
    );
    assert_eq!(repo::count_items(db.writer()).unwrap(), 1);
}

#[test]
fn ac45_saving_a_removed_items_url_points_at_it_instead_of_duplicating() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let clock = FixedClock::new(1);
    let it = save_bookmark(db.writer(), &clock, "https://example.com/gone")
        .unwrap()
        .outcome
        .item()
        .clone();
    repo::set_removed(db.writer(), &it.id, 1, Some(2), 2).unwrap();
    match save_bookmark(db.writer(), &clock, "https://example.com/gone")
        .unwrap()
        .outcome
    {
        SaveOutcome::Existing { item, .. } => {
            assert_eq!(item.id, it.id);
            assert_eq!(
                item.removed_at,
                Some(2),
                "UI offers Restore / Cancel (§7.3)"
            );
        }
        o => panic!("{o:?}"),
    }
    assert_eq!(repo::count_items(db.writer()).unwrap(), 1);
}
