//! STATE.md F1.7: read models behind the Library screen (SPEC §9.2, §9.3).
//! Card state comes from media assets and their root (INV-02, DATA-09).

use vv_persistence::library::repo::{self, LibraryItem, MediaAsset, StorageRoot};
use vv_persistence::library::search::*;
use vv_persistence::library::view::*;
use vv_persistence::library::*;
use vv_persistence::{Database, OpenOptions};

fn open(dir: &std::path::Path) -> Database {
    Database::open(&dir.join("library.sqlite"), &OpenOptions::default()).unwrap()
}

fn item(c: &rusqlite::Connection, kind: ItemKind, title: &str, added_at: i64) -> LibraryItem {
    let platform = if kind == ItemKind::LocalVideo {
        SourcePlatform::Local
    } else {
        SourcePlatform::Generic
    };
    let mut it = LibraryItem::new(kind, platform, added_at);
    if kind != ItemKind::LocalVideo {
        it.canonical_url = Some(format!("https://example.com/{title}"));
    }
    it.title = Some(title.to_owned());
    repo::insert_item(c, &it).unwrap();
    it
}

fn root(c: &rusqlite::Connection, kind: StorageRootKind, status: StorageRootStatus) -> StorageRoot {
    let r = StorageRoot {
        id: new_id(),
        kind,
        label: "Videos".into(),
        path: format!("/vv-test/{}", new_id()),
        volume_id: None,
        status,
        created_at: 1,
        last_verified_at: None,
    };
    repo::insert_storage_root(c, &r).unwrap();
    r
}

fn original(
    c: &rusqlite::Connection,
    item: &LibraryItem,
    root: &StorageRoot,
    file: &str,
    acq: AcquisitionStatus,
    integrity: IntegrityState,
) -> MediaAsset {
    let mut a = MediaAsset::new(
        &item.id,
        AssetRole::Original,
        MediaKind::Video,
        StorageKind::ExternalReference,
        &root.id,
        file,
        2,
    );
    a.acquisition_status = acq;
    a.integrity_state = integrity;
    a.container = Some("mp4".into());
    a.width = Some(1920);
    a.height = Some(1080);
    repo::insert_media_asset(c, &a).unwrap();
    a
}

#[test]
fn local_state_comes_from_assets_and_root_status() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let up = root(
        c,
        StorageRootKind::ExternalImport,
        StorageRootStatus::Available,
    );
    let gone = root(
        c,
        StorageRootKind::ExternalImport,
        StorageRootStatus::Disconnected,
    );

    // A `local_video` item without a ready original is not "downloaded".
    let bookmark = item(c, ItemKind::Bookmark, "b", 1);
    let partial = item(c, ItemKind::LocalVideo, "p", 2);
    original(
        c,
        &partial,
        &up,
        "p.mp4",
        AcquisitionStatus::Acquiring,
        IntegrityState::Unknown,
    );
    let ready = item(c, ItemKind::LocalVideo, "r", 3);
    original(
        c,
        &ready,
        &up,
        "r.mp4",
        AcquisitionStatus::Ready,
        IntegrityState::Verified,
    );
    let missing = item(c, ItemKind::LocalVideo, "m", 4);
    original(
        c,
        &missing,
        &up,
        "m.mp4",
        AcquisitionStatus::Ready,
        IntegrityState::Missing,
    );
    let unplugged = item(c, ItemKind::LocalVideo, "u", 5);
    original(
        c,
        &unplugged,
        &gone,
        "u.mp4",
        AcquisitionStatus::Ready,
        IntegrityState::Verified,
    );
    // One reachable copy wins over a copy on an absent drive.
    let both = item(c, ItemKind::LocalVideo, "two", 6);
    original(
        c,
        &both,
        &gone,
        "a/two.mp4",
        AcquisitionStatus::Ready,
        IntegrityState::Verified,
    );
    original(
        c,
        &both,
        &up,
        "b/two.mp4",
        AcquisitionStatus::Ready,
        IntegrityState::Verified,
    );

    let ids = [&bookmark, &partial, &ready, &missing, &unplugged, &both].map(|i| i.id.as_str());
    let s = local_states(c, &ids).unwrap();
    assert_eq!(s[&bookmark.id], LocalState::BookmarkOnly);
    assert_eq!(s[&partial.id], LocalState::BookmarkOnly);
    assert_eq!(s[&ready.id], LocalState::LocalReady);
    assert_eq!(s[&missing.id], LocalState::Missing);
    assert_eq!(s[&unplugged.id], LocalState::Disconnected);
    assert_eq!(s[&both.id], LocalState::LocalReady);
    assert!(local_states(c, &[]).unwrap().is_empty());
}

#[test]
fn asset_view_shows_file_name_not_path() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let up = root(
        c,
        StorageRootKind::ExternalImport,
        StorageRootStatus::Available,
    );
    let it = item(c, ItemKind::LocalVideo, "clip", 1);
    original(
        c,
        &it,
        &up,
        "trips/2026/clip.mp4",
        AcquisitionStatus::Ready,
        IntegrityState::Verified,
    );
    let v = asset_views(c, &it.id).unwrap();
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].file_name, "clip.mp4");
    assert_eq!(v[0].root_label, "Videos");
    assert_eq!(v[0].root_status, StorageRootStatus::Available);
    assert_eq!((v[0].width, v[0].height), (Some(1920), Some(1080)));
}

#[test]
fn thumbnail_location_only_resolves_cache_thumbnails() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let up = root(
        c,
        StorageRootKind::ExternalImport,
        StorageRootStatus::Available,
    );
    let cache = root(c, StorageRootKind::Cache, StorageRootStatus::Available);
    let it = item(c, ItemKind::LocalVideo, "clip", 1);
    let orig = original(
        c,
        &it,
        &up,
        "clip.mp4",
        AcquisitionStatus::Ready,
        IntegrityState::Verified,
    );
    let mut thumb = MediaAsset::new(
        &it.id,
        AssetRole::Thumbnail,
        MediaKind::Image,
        StorageKind::Managed,
        &cache.id,
        "thumbnails/x/t.jpg",
        3,
    );
    thumb.source_asset_id = Some(orig.id.clone());
    thumb.derivation = Some(r#"{"generator":"test"}"#.into());
    repo::insert_media_asset(c, &thumb).unwrap();
    assert_eq!(
        thumbnail_location(c, &thumb.id).unwrap(),
        Some((cache.path.clone(), "thumbnails/x/t.jpg".to_owned()))
    );
    // The user's own video is never served as a thumbnail.
    assert_eq!(thumbnail_location(c, &orig.id).unwrap(), None);
    assert_eq!(thumbnail_location(c, "no-such-id").unwrap(), None);
}

#[test]
fn recently_added_filter() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    item(c, ItemKind::Bookmark, "old", 10);
    let new = item(c, ItemKind::Bookmark, "new", 1_000);
    let page = search(
        c,
        &SearchQuery {
            filters: Filters {
                added_since: Some(500),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.hits[0].item.id, new.id);
}
