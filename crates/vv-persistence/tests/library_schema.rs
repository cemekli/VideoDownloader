//! STATE.md F1.1, SPEC §11: migration 0002 (library schema) and the
//! repositories over it, against real database files.

use rusqlite::{params, Connection};
use vv_persistence::library::repo::{
    self, LibraryItem, MediaAsset, StorageRoot, UrlAlias, UserOverrides,
};
use vv_persistence::library::*;
use vv_persistence::migrations::{Migrator, MIGRATIONS};
use vv_persistence::{Database, OpenOptions};

fn open() -> (tempfile::TempDir, Database) {
    let d = tempfile::tempdir().unwrap();
    let db = Database::open(&d.path().join("library.sqlite"), &OpenOptions::default()).unwrap();
    (d, db)
}

fn bookmark(url: &str, now: i64) -> LibraryItem {
    let mut it = LibraryItem::new(ItemKind::Bookmark, SourcePlatform::Generic, now);
    it.canonical_url = Some(url.to_owned());
    it.original_url = Some(url.to_owned());
    it
}

/// Values listed in `CHECK (<col> IN (...))` for a column, from the schema
/// of a fully migrated database (later migrations may widen a list).
fn check_list(table: &str, col: &str) -> Vec<String> {
    let (_d, db) = open();
    let sql: String = db
        .reader()
        .unwrap()
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = ?1",
            [table],
            |r| r.get(0),
        )
        .unwrap();
    let body = sql.as_str();
    let line = body
        .lines()
        .find(|l| l.trim_start().starts_with(&format!("{col} ")))
        .unwrap_or_else(|| panic!("{table}.{col}"));
    let inner = line.split_once(" IN (").unwrap().1;
    let inner = inner.split_once(')').unwrap().0;
    inner
        .split(',')
        .map(|v| v.trim().trim_matches('\'').to_owned())
        .collect()
}

fn list<T: Copy>(all: &[T], f: fn(T) -> &'static str) -> Vec<String> {
    all.iter().map(|v| f(*v).to_owned()).collect()
}

#[test]
fn rust_enums_and_sql_checks_list_the_same_values() {
    let cases: Vec<(&str, &str, Vec<String>)> = vec![
        (
            "storage_root",
            "kind",
            list(StorageRootKind::ALL, StorageRootKind::as_str),
        ),
        (
            "storage_root",
            "status",
            list(StorageRootStatus::ALL, StorageRootStatus::as_str),
        ),
        (
            "library_item",
            "kind",
            list(ItemKind::ALL, ItemKind::as_str),
        ),
        (
            "library_item",
            "source_platform",
            list(SourcePlatform::ALL, SourcePlatform::as_str),
        ),
        (
            "library_item",
            "published_precision",
            list(PublishedPrecision::ALL, PublishedPrecision::as_str),
        ),
        (
            "library_item",
            "visibility",
            list(Visibility::ALL, Visibility::as_str),
        ),
        (
            "library_item",
            "remote_availability",
            list(RemoteAvailability::ALL, RemoteAvailability::as_str),
        ),
        (
            "library_item",
            "access_state",
            list(AccessState::ALL, AccessState::as_str),
        ),
        (
            "library_item",
            "metadata_status",
            list(MetadataStatus::ALL, MetadataStatus::as_str),
        ),
        (
            "media_asset",
            "role",
            list(AssetRole::ALL, AssetRole::as_str),
        ),
        (
            "media_asset",
            "media_kind",
            list(MediaKind::ALL, MediaKind::as_str),
        ),
        (
            "media_asset",
            "storage_kind",
            list(StorageKind::ALL, StorageKind::as_str),
        ),
        (
            "media_asset",
            "acquisition_status",
            list(AcquisitionStatus::ALL, AcquisitionStatus::as_str),
        ),
        (
            "media_asset",
            "integrity_state",
            list(IntegrityState::ALL, IntegrityState::as_str),
        ),
        (
            "duplicate_candidate",
            "reason",
            list(DuplicateReason::ALL, DuplicateReason::as_str),
        ),
        (
            "duplicate_candidate",
            "status",
            list(DuplicateStatus::ALL, DuplicateStatus::as_str),
        ),
    ];
    for (t, c, rust) in cases {
        assert_eq!(check_list(t, c), rust, "{t}.{c}");
    }
    // Domain platforms are a subset with the same wire names.
    for p in [
        vv_domain::normalize::Platform::Youtube,
        vv_domain::normalize::Platform::Generic,
        vv_domain::normalize::Platform::Vk,
        vv_domain::normalize::Platform::Dailymotion,
        vv_domain::normalize::Platform::Twitch,
        vv_domain::normalize::Platform::Reddit,
    ] {
        assert_eq!(SourcePlatform::from(p).as_str(), p.as_str());
    }
}

#[test]
fn item_round_trips_every_field() {
    let (_d, mut db) = open();
    let c = db.writer();
    let mut it = bookmark("https://example.com/a", 1_700_000_000_000);
    it.kind = ItemKind::RemoteVideo;
    it.source_platform = SourcePlatform::Youtube;
    it.source_id = Some("vvTestVid01".into());
    it.normalization_version = 1;
    it.title = Some("Başlık".into());
    it.description = Some("açıklama".into());
    it.creator = Some("kanal".into());
    it.user_overrides = UserOverrides {
        title: Some("Benim başlığım".into()),
        ..UserOverrides::empty()
    };
    it.published_date = Some("2025-03-01".into());
    it.published_precision = Some(PublishedPrecision::Date);
    it.duration_ms = Some(61_000);
    it.language = Some("tr".into());
    it.visibility = Visibility::Public;
    it.remote_availability = RemoteAvailability::Available;
    it.access_state = AccessState::PublicOk;
    it.access_checked_at = Some(5);
    it.metadata_status = MetadataStatus::Ready;
    it.user_notes = "not".into();
    it.is_favorite = true;
    it.archived_at = Some(7);
    it.last_checked_at = Some(8);
    it.metadata_provenance = Some(r#"{"adapter":"yt-dlp","version":"2026.10.26"}"#.into());
    repo::insert_item(c, &it).unwrap();
    assert_eq!(repo::get_item(c, &it.id).unwrap().unwrap(), it);
    assert_eq!(it.display_title(), "Benim başlığım");
    // The index stores text with dotless ı folded to i (ADR-0012).
    let doc = repo::get_search_document(c, &it.id).unwrap().unwrap();
    assert_eq!(
        (doc.title.as_str(), doc.notes.as_str(), doc.creator.as_str()),
        ("Benim başliğim", "not", "kanal")
    );
    assert_eq!(
        repo::item_by_source(c, SourcePlatform::Youtube, "vvTestVid01")
            .unwrap()
            .unwrap()
            .id,
        it.id
    );
}

#[test]
fn ids_are_random_uuids() {
    let a = new_id();
    assert_eq!(a.len(), 36);
    assert_eq!(a.get(14..15), Some("4"), "version 4");
    assert_ne!(a, new_id());
}

#[test]
fn unique_identity_indexes() {
    let (_d, mut db) = open();
    let c = db.writer();
    repo::insert_item(c, &bookmark("https://example.com/a", 1)).unwrap();
    let e = repo::insert_item(c, &bookmark("https://example.com/a", 2)).unwrap_err();
    assert!(e.is_unique_violation(), "{e}");

    let mut y1 = bookmark("https://www.youtube.com/watch?v=vvTestVid01", 3);
    y1.source_platform = SourcePlatform::Youtube;
    y1.source_id = Some("vvTestVid01".into());
    repo::insert_item(c, &y1).unwrap();
    let mut y2 = bookmark("https://youtu.be/vvTestVid01", 4);
    y2.source_platform = SourcePlatform::Youtube;
    y2.source_id = Some("vvTestVid01".into());
    assert!(repo::insert_item(c, &y2).unwrap_err().is_unique_violation());
    // Unknown source ids do not collide with each other.
    repo::insert_item(c, &bookmark("https://example.com/b", 5)).unwrap();
    repo::insert_item(c, &bookmark("https://example.com/c", 6)).unwrap();
    assert_eq!(repo::count_items(c).unwrap(), 4);
}

#[test]
fn schema_refuses_invalid_rows() {
    let (_d, mut db) = open();
    let c = db.writer();
    let ins = |c: &Connection, kind: &str, url: Option<&str>, platform: &str| {
        c.execute(
            "INSERT INTO library_item (id, kind, canonical_url, source_platform, added_at, updated_at) VALUES (?1, ?2, ?3, ?4, 1, 1)",
            params![new_id(), kind, url, platform],
        )
    };
    assert!(
        ins(c, "bookmark", None, "generic").is_err(),
        "remote item without URL"
    );
    assert!(
        ins(c, "local_video", None, "youtube").is_err(),
        "local item with a platform"
    );
    assert!(
        ins(c, "bookmark", Some("u"), "local").is_err(),
        "remote item as local"
    );
    assert!(
        ins(c, "download", Some("u"), "generic").is_err(),
        "unknown kind"
    );
    ins(c, "local_video", None, "local").unwrap();
    // STRICT: no silent type coercion.
    assert!(c
        .execute(
            "INSERT INTO library_item (id, kind, canonical_url, source_platform, added_at, updated_at) VALUES ('x', 'bookmark', 'u2', 'generic', 'yesterday', 1)",
            [],
        )
        .is_err());
    // §11.1 published precision.
    let mut it = bookmark("https://example.com/p", 1);
    it.published_at = Some(1);
    assert!(
        repo::insert_item(c, &it).is_err(),
        "value without precision"
    );
    it.published_precision = Some(PublishedPrecision::Date);
    assert!(
        repo::insert_item(c, &it).is_err(),
        "date precision with a datetime value"
    );
    it.published_precision = Some(PublishedPrecision::Datetime);
    repo::insert_item(c, &it).unwrap();
    let mut bad_overrides = bookmark("https://example.com/o", 1);
    bad_overrides.metadata_provenance = Some("{not json".into());
    assert!(repo::insert_item(c, &bad_overrides).is_err());
}

fn root(c: &Connection) -> StorageRoot {
    let r = StorageRoot {
        id: new_id(),
        kind: StorageRootKind::ManagedMedia,
        label: "Medya".into(),
        path: "C:\\Users\\vv\\Videos\\VideoVault".into(),
        volume_id: Some("vol-1".into()),
        status: StorageRootStatus::Available,
        created_at: 1,
        last_verified_at: None,
    };
    repo::insert_storage_root(c, &r).unwrap();
    r
}

#[test]
fn storage_roots_and_assets() {
    let (_d, mut db) = open();
    let c = db.writer();
    let r = root(c);
    assert_eq!(repo::get_storage_root(c, &r.id).unwrap().unwrap(), r);
    repo::set_storage_root_status(c, &r.id, StorageRootStatus::Disconnected, Some(9)).unwrap();
    let r2 = repo::get_storage_root(c, &r.id).unwrap().unwrap();
    assert_eq!(
        (r2.status, r2.last_verified_at),
        (StorageRootStatus::Disconnected, Some(9))
    );

    let it = bookmark("https://example.com/v", 1);
    repo::insert_item(c, &it).unwrap();
    let mut orig = MediaAsset::new(
        &it.id,
        AssetRole::Original,
        MediaKind::Video,
        StorageKind::Managed,
        &r.id,
        "2026/11/v.mp4",
        2,
    );
    orig.content_hash = Some("ab".repeat(32));
    orig.hash_algorithm = Some("sha256".into());
    orig.size_bytes = Some(1024);
    orig.audio_tracks = Some(r#"[{"lang":"tr","codec":"aac","channels":2}]"#.into());
    repo::insert_media_asset(c, &orig).unwrap();
    assert_eq!(repo::get_media_asset(c, &orig.id).unwrap().unwrap(), orig);

    // Paths are relative to the root (§11.3).
    for bad in ["/abs/v.mp4", "\\\\server\\v.mp4", "C:\\v.mp4", ""] {
        let a = MediaAsset::new(
            &it.id,
            AssetRole::Original,
            MediaKind::Video,
            StorageKind::Managed,
            &r.id,
            bad,
            2,
        );
        assert!(repo::insert_media_asset(c, &a).is_err(), "{bad:?}");
    }
    // Derived assets need a derivation; originals must not have one.
    let mut thumb = MediaAsset::new(
        &it.id,
        AssetRole::Thumbnail,
        MediaKind::Image,
        StorageKind::Managed,
        &r.id,
        "t.jpg",
        3,
    );
    assert!(repo::insert_media_asset(c, &thumb).is_err());
    thumb.source_asset_id = Some(orig.id.clone());
    thumb.derivation = Some(
        r#"{"generator":"ffmpeg","generator_version":"9.0.2","config":{"v":1},"created_at":3}"#
            .into(),
    );
    repo::insert_media_asset(c, &thumb).unwrap();
    c.execute(
        "UPDATE library_item SET thumbnail_asset_id = ?1 WHERE id = ?2",
        params![thumb.id, it.id],
    )
    .unwrap();

    // A root that still holds assets cannot be deleted.
    assert!(c
        .execute("DELETE FROM storage_root WHERE id = ?1", [&r.id])
        .is_err());
    // Deleting the source asset removes its derivatives and clears the thumbnail.
    c.execute("DELETE FROM media_asset WHERE id = ?1", [&orig.id])
        .unwrap();
    assert!(repo::assets_for_item(c, &it.id).unwrap().is_empty());
    assert_eq!(
        repo::get_item(c, &it.id)
            .unwrap()
            .unwrap()
            .thumbnail_asset_id,
        None
    );
}

#[test]
fn deleting_an_item_cascades_to_its_rows() {
    let (_d, mut db) = open();
    let c = db.writer();
    let r = root(c);
    let a = bookmark("https://example.com/a", 1);
    let b = bookmark("https://example.com/b", 2);
    repo::insert_item(c, &a).unwrap();
    repo::insert_item(c, &b).unwrap();
    repo::insert_media_asset(
        c,
        &MediaAsset::new(
            &a.id,
            AssetRole::Original,
            MediaKind::Video,
            StorageKind::ExternalReference,
            &r.id,
            "a.mp4",
            1,
        ),
    )
    .unwrap();
    repo::insert_url_alias(
        c,
        &UrlAlias {
            id: new_id(),
            item_id: a.id.clone(),
            url: "https://example.com/a-old".into(),
            normalization_version: 0,
            created_at: 1,
        },
    )
    .unwrap();
    repo::open_duplicate_candidate(c, &a.id, &b.id, DuplicateReason::SimilarUrl, 3).unwrap();
    c.execute("DELETE FROM library_item WHERE id = ?1", [&a.id])
        .unwrap();
    for t in ["media_asset", "url_alias", "duplicate_candidate"] {
        let n: i64 = c
            .query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "{t}");
    }
}

#[test]
fn aliases_resolve_to_their_item_and_are_unique() {
    let (_d, mut db) = open();
    let c = db.writer();
    let a = bookmark("https://example.com/new", 1);
    repo::insert_item(c, &a).unwrap();
    let alias = UrlAlias {
        id: new_id(),
        item_id: a.id.clone(),
        url: "https://example.com/old".into(),
        normalization_version: 1,
        created_at: 2,
    };
    repo::insert_url_alias(c, &alias).unwrap();
    assert_eq!(
        repo::item_by_alias(c, "https://example.com/old")
            .unwrap()
            .unwrap()
            .id,
        a.id
    );
    assert_eq!(
        repo::aliases_for_item(c, &a.id).unwrap(),
        vec![alias.clone()]
    );
    let dup = UrlAlias {
        id: new_id(),
        ..alias
    };
    assert!(repo::insert_url_alias(c, &dup)
        .unwrap_err()
        .is_unique_violation());
}

#[test]
fn duplicate_candidates_are_one_per_open_pair() {
    let (_d, mut db) = open();
    let c = db.writer();
    let a = bookmark("https://example.com/a", 1);
    let b = bookmark("https://example.com/b", 2);
    repo::insert_item(c, &a).unwrap();
    repo::insert_item(c, &b).unwrap();
    let d1 =
        repo::open_duplicate_candidate(c, &a.id, &b.id, DuplicateReason::ContentHash, 3).unwrap();
    let d2 =
        repo::open_duplicate_candidate(c, &b.id, &a.id, DuplicateReason::ContentHash, 4).unwrap();
    assert_eq!(d1, d2, "same unordered pair and reason");
    let other =
        repo::open_duplicate_candidate(c, &a.id, &b.id, DuplicateReason::SimilarUrl, 5).unwrap();
    assert_ne!(other.id, d1.id);
    assert!(
        repo::open_duplicate_candidate(c, &a.id, &a.id, DuplicateReason::SimilarUrl, 5).is_err()
    );
    repo::decide_duplicate_candidate(c, &d1.id, DuplicateStatus::Dismissed, 6).unwrap();
    assert_eq!(repo::open_duplicates_for_item(c, &a.id).unwrap().len(), 1);
    // After a decision the same pair can be proposed again.
    let d3 =
        repo::open_duplicate_candidate(c, &a.id, &b.id, DuplicateReason::ContentHash, 7).unwrap();
    assert_ne!(d3.id, d1.id);
}

#[test]
fn updates_check_the_revision() {
    let (_d, mut db) = open();
    let c = db.writer();
    let it = bookmark("https://example.com/r", 1);
    repo::insert_item(c, &it).unwrap();
    let v2 = repo::set_notes(c, &it.id, 1, "ilk not", 10).unwrap();
    assert_eq!(
        (v2.revision, v2.updated_at, v2.user_notes.as_str()),
        (2, 10, "ilk not")
    );
    // A writer holding revision 1 lost the race.
    match repo::set_notes(c, &it.id, 1, "eski", 11).unwrap_err() {
        e @ LibraryError::RevisionConflict { current: 2 } => {
            assert_eq!(e.code(), vv_domain::error::ErrorCode::RevisionConflict)
        }
        e => panic!("{e:?}"),
    }
    assert_eq!(
        repo::get_item(c, &it.id).unwrap().unwrap().user_notes,
        "ilk not"
    );
    assert!(matches!(
        repo::set_notes(c, "missing", 1, "x", 1),
        Err(LibraryError::NotFound)
    ));
    let v3 = repo::set_favorite(c, &it.id, 2, true, 12).unwrap();
    assert!(v3.is_favorite);
    // The search document follows every write.
    assert_eq!(
        repo::search_phrase_ids(c, "ilk not").unwrap(),
        vec![it.id.clone()]
    );
    let v4 = repo::set_user_overrides(
        c,
        &it.id,
        3,
        &UserOverrides {
            title: Some("Özel ad".into()),
            ..UserOverrides::empty()
        },
        13,
    )
    .unwrap();
    assert_eq!(v4.display_title(), "Özel ad");
    assert_eq!(
        repo::search_phrase_ids(c, "Özel ad").unwrap(),
        vec![it.id.clone()]
    );
    // Clearing the override falls back to the platform (here: derived) title.
    let v5 = repo::set_user_overrides(c, &it.id, 4, &UserOverrides::empty(), 14).unwrap();
    assert_eq!(v5.display_title(), "example.com · r");
    // Removed items leave search (§7.3) but keep their row.
    repo::set_removed(c, &it.id, 5, Some(15), 15).unwrap();
    assert!(repo::search_phrase_ids(c, "ilk not").unwrap().is_empty());
}

#[test]
fn library_schema_goes_down_with_data_and_up_again() {
    let (_d, mut db) = open();
    let c = db.writer();
    let r = root(c);
    let it = bookmark("https://example.com/v", 1);
    repo::insert_item(c, &it).unwrap();
    let a = MediaAsset::new(
        &it.id,
        AssetRole::Original,
        MediaKind::Video,
        StorageKind::Managed,
        &r.id,
        "v.mp4",
        1,
    );
    repo::insert_media_asset(c, &a).unwrap();
    c.execute("UPDATE library_item SET thumbnail_asset_id = ?1", [&a.id])
        .unwrap();
    let m = Migrator::new(MIGRATIONS).unwrap();
    m.migrate_down_to(c, 1).unwrap();
    let n: i64 = c
        .query_row("SELECT count(*) FROM sqlite_master WHERE name IN ('library_item', 'media_asset', 'search_document')", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
    m.migrate_up(c).unwrap();
    assert_eq!(repo::count_items(c).unwrap(), 0);
}
