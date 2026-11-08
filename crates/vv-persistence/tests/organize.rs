//! STATE.md F2.1–F2.3: tags, collections, smart collections, bulk flags,
//! Undo, permanent deletion, orphaned media and item merge, against real
//! database files (migration 0003, ADR-0015).

use rusqlite::{params, Connection};
use vv_persistence::library::organize::{self, ChildPolicy, SmartRules};
use vv_persistence::library::repo::{self, LibraryItem, MediaAsset, StorageRoot};
use vv_persistence::library::search::{self, Filters, SearchQuery};
use vv_persistence::library::*;
use vv_persistence::library::{merge, removal, undo};
use vv_persistence::{Database, OpenOptions};

const S: &str = "session-1";

fn open() -> (tempfile::TempDir, Database) {
    let d = tempfile::tempdir().unwrap();
    let db = Database::open(&d.path().join("library.sqlite"), &OpenOptions::default()).unwrap();
    (d, db)
}

fn item(c: &Connection, title: &str, n: i64) -> LibraryItem {
    let mut it = LibraryItem::new(ItemKind::Bookmark, SourcePlatform::Generic, n);
    it.canonical_url = Some(format!("https://example.com/v/{n}"));
    it.title = Some(title.into());
    repo::insert_item(c, &it).unwrap();
    it
}

fn items(c: &Connection, n: i64) -> Vec<String> {
    (1..=n)
        .map(|i| item(c, &format!("Video {i}"), i).id)
        .collect()
}

fn find(c: &Connection, text: &str) -> Vec<String> {
    search::search(
        c,
        &SearchQuery {
            text: text.into(),
            ..Default::default()
        },
    )
    .unwrap()
    .hits
    .into_iter()
    .map(|h| h.item.id)
    .collect()
}

fn filtered(c: &Connection, f: Filters) -> Vec<String> {
    let mut v: Vec<String> = search::search(
        c,
        &SearchQuery {
            filters: f,
            limit: 500,
            ..Default::default()
        },
    )
    .unwrap()
    .hits
    .into_iter()
    .map(|h| h.item.id)
    .collect();
    v.sort();
    v
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

fn root(c: &Connection) -> StorageRoot {
    let r = StorageRoot {
        id: new_id(),
        kind: StorageRootKind::ManagedMedia,
        label: "Medya".into(),
        path: "/vv/media".into(),
        volume_id: None,
        status: StorageRootStatus::Available,
        created_at: 1,
        last_verified_at: None,
    };
    repo::insert_storage_root(c, &r).unwrap();
    r
}

fn asset(
    c: &Connection,
    it: &str,
    root: &str,
    r: &str,
    role: AssetRole,
    kind: StorageKind,
) -> MediaAsset {
    let mut a = MediaAsset::new(it, role, MediaKind::Video, kind, root, r, 1);
    if role != AssetRole::Original {
        a.derivation = Some("{}".into());
        a.media_kind = MediaKind::Image;
    }
    repo::insert_media_asset(c, &a).unwrap();
    a
}

// --- Tags ------------------------------------------------------------------

#[test]
fn tag_names_fold_case_and_turkish_i() {
    assert_eq!(
        organize::normalize_tag("  İstanbul   gezisi "),
        "istanbul gezisi"
    );
    assert_eq!(organize::normalize_tag("ISTANBUL"), "istanbul");
    assert_eq!(organize::normalize_tag("ıstanbul"), "istanbul");
    let (_d, mut db) = open();
    let c = db.writer();
    let (a, created) = organize::get_or_create_tag(c, "İstanbul", 1).unwrap();
    assert!(created);
    let (b, created) = organize::get_or_create_tag(c, "istanbul", 2).unwrap();
    assert!(!created);
    assert_eq!(a.id, b.id);
    assert_eq!(b.name, "İstanbul");
    assert!(organize::get_or_create_tag(c, "   ", 1).is_err());
    assert!(organize::get_or_create_tag(c, &"x".repeat(101), 1).is_err());
}

#[test]
fn bulk_tag_and_archive_then_undo_ac07() {
    let (_d, mut db) = open();
    let c = db.writer();
    let ids = items(c, 25);
    let twenty = &ids[..20];
    // Five already carry the tag: only 15 change and only those are undone.
    let (tag, _) = organize::get_or_create_tag(c, "deniz", 1).unwrap();
    organize::tag_items(c, &tag.id, &ids[..5], 1).unwrap();

    let added = organize::tag_items(c, &tag.id, twenty, 2).unwrap();
    assert_eq!(added.len(), 15);
    undo::record(
        c,
        S,
        &undo::Operation::TagItems {
            tag_id: tag.id.clone(),
            tag_created: false,
            items: added,
        },
        2,
    )
    .unwrap();
    assert_eq!(sorted(find(c, "deniz")), sorted(twenty.to_vec()));

    let r = undo::undo(c, S, 3).unwrap().unwrap();
    assert_eq!((r.applied, r.skipped), (15, 0));
    assert_eq!(sorted(find(c, "deniz")), sorted(ids[..5].to_vec()));
    // Nothing more to undo.
    assert!(undo::undo(c, S, 4).unwrap().is_none());

    let rows = organize::set_archived(c, twenty, Some(10), 10).unwrap();
    assert_eq!(rows.len(), 20);
    undo::record(
        c,
        S,
        &undo::Operation::SetArchived { at: Some(10), rows },
        10,
    )
    .unwrap();
    assert_eq!(filtered(c, Filters::default()).len(), 5);
    let r = undo::undo(c, S, 11).unwrap().unwrap();
    assert_eq!(r.applied, 20);
    assert_eq!(filtered(c, Filters::default()).len(), 25);
}

#[test]
fn undo_keeps_background_changes_and_skips_user_changes_ac60() {
    let (_d, mut db) = open();
    let c = db.writer();
    let ids = items(c, 3);
    let (tag, created) = organize::get_or_create_tag(c, "konser", 1).unwrap();
    let added = organize::tag_items(c, &tag.id, &ids, 1).unwrap();
    undo::record(
        c,
        S,
        &undo::Operation::TagItems {
            tag_id: tag.id.clone(),
            tag_created: created,
            items: added,
        },
        1,
    )
    .unwrap();

    // A metadata update lands in the background: no log row, new revision.
    c.execute("UPDATE library_item SET description = 'from metadata', revision = revision + 1 WHERE id = ?1", [&ids[0]]).unwrap();
    assert!(undo::peek(c, S).unwrap().is_some());
    let r = undo::undo(c, S, 2).unwrap().unwrap();
    assert_eq!(r.applied, 3);
    let it = repo::get_item(c, &ids[0]).unwrap().unwrap();
    assert_eq!(it.description.as_deref(), Some("from metadata"));
    // The tag was created by the operation and is now unused: gone.
    assert!(organize::get_tag(c, &tag.id).unwrap().is_none());

    // Favorite three, the user unfavorites one by hand: Undo skips it.
    let fav = organize::set_favorite(c, &ids, true, 3).unwrap();
    undo::record(
        c,
        S,
        &undo::Operation::SetFavorite {
            value: true,
            items: fav,
        },
        3,
    )
    .unwrap();
    organize::set_favorite(c, &ids[..1], false, 4).unwrap();
    let r = undo::undo(c, S, 5).unwrap().unwrap();
    assert_eq!((r.applied, r.skipped), (2, 1));
}

#[test]
fn undo_is_per_session_and_only_the_newest() {
    let (_d, mut db) = open();
    let c = db.writer();
    let ids = items(c, 2);
    let f = organize::set_favorite(c, &ids[..1], true, 1).unwrap();
    undo::record(
        c,
        S,
        &undo::Operation::SetFavorite {
            value: true,
            items: f,
        },
        1,
    )
    .unwrap();
    let f = organize::set_favorite(c, &ids[1..], true, 2).unwrap();
    undo::record(
        c,
        S,
        &undo::Operation::SetFavorite {
            value: true,
            items: f,
        },
        2,
    )
    .unwrap();
    // A restart is a new session: nothing to undo.
    assert!(undo::peek(c, "session-2").unwrap().is_none());
    undo::undo(c, S, 3).unwrap().unwrap();
    // The older operation does not become undoable after the newer one.
    assert!(undo::undo(c, S, 4).unwrap().is_none());
    assert!(repo::get_item(c, &ids[0]).unwrap().unwrap().is_favorite);
    // An operation that changed nothing is not logged.
    let none = organize::set_favorite(c, &ids[..1], true, 5).unwrap();
    assert_eq!(
        undo::record(
            c,
            S,
            &undo::Operation::SetFavorite {
                value: true,
                items: none
            },
            5
        )
        .unwrap(),
        None
    );
}

#[test]
fn rename_tag_reindexes_and_undo_restores_ac39() {
    let (_d, mut db) = open();
    let c = db.writer();
    let ids = items(c, 2);
    let (t, _) = organize::get_or_create_tag(c, "gezi", 1).unwrap();
    organize::tag_items(c, &t.id, &ids, 1).unwrap();
    let old = organize::rename_tag(c, &t.id, "seyahat").unwrap();
    undo::record(
        c,
        S,
        &undo::Operation::RenameTag {
            tag_id: t.id.clone(),
            old_name: old.name,
            new_name: "seyahat".into(),
        },
        2,
    )
    .unwrap();
    assert_eq!(find(c, "seyahat").len(), 2);
    assert!(find(c, "gezi").is_empty());
    undo::undo(c, S, 3).unwrap().unwrap();
    assert_eq!(find(c, "gezi").len(), 2);
    assert!(find(c, "seyahat").is_empty());

    let (other, _) = organize::get_or_create_tag(c, "yemek", 4).unwrap();
    let e = organize::rename_tag(c, &other.id, "GEZİ").unwrap_err();
    assert!(matches!(e, LibraryError::Invalid("tag-name-taken")), "{e}");
}

#[test]
fn merge_tags_and_undo_restores_same_id_and_relations_ac46() {
    let (_d, mut db) = open();
    let c = db.writer();
    let ids = items(c, 4);
    let (a, _) = organize::get_or_create_tag(c, "film", 1).unwrap();
    let (b, _) = organize::get_or_create_tag(c, "sinema", 1).unwrap();
    organize::tag_items(c, &a.id, &ids[..3], 1).unwrap();
    organize::tag_items(c, &b.id, &ids[2..], 1).unwrap();
    let before_a = organize::tag_relations(c, &a.id).unwrap();
    let before_b = organize::tag_relations(c, &b.id).unwrap();

    let m = organize::merge_tags(c, &a.id, &b.id).unwrap();
    assert_eq!(m.target_added.len(), 2);
    undo::record(c, S, &undo::Operation::MergeTags(m), 2).unwrap();
    assert!(organize::get_tag(c, &a.id).unwrap().is_none());
    assert_eq!(organize::tag_relations(c, &b.id).unwrap().len(), 4);
    assert!(find(c, "film").is_empty());
    assert_eq!(find(c, "sinema").len(), 4);

    undo::undo(c, S, 3).unwrap().unwrap();
    assert_eq!(organize::get_tag(c, &a.id).unwrap().unwrap(), a);
    assert_eq!(organize::tag_relations(c, &a.id).unwrap(), before_a);
    assert_eq!(organize::tag_relations(c, &b.id).unwrap(), before_b);
    assert_eq!(find(c, "film").len(), 3);
}

#[test]
fn delete_tag_drops_it_from_search() {
    let (_d, mut db) = open();
    let c = db.writer();
    let ids = items(c, 1);
    let (t, _) = organize::get_or_create_tag(c, "silinecek", 1).unwrap();
    organize::tag_items(c, &t.id, &ids, 1).unwrap();
    assert_eq!(organize::delete_tag(c, &t.id).unwrap(), 1);
    assert!(find(c, "silinecek").is_empty());
    assert!(repo::get_item(c, &ids[0]).unwrap().is_some());
}

// --- Collections -----------------------------------------------------------

#[test]
fn delete_collection_keeps_items_ac06() {
    let (_d, mut db) = open();
    let c = db.writer();
    let ids = items(c, 3);
    let col = organize::create_collection(c, "Tatil", None, 1).unwrap();
    assert_eq!(
        organize::add_to_collection(c, &col.id, &ids, 1)
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        filtered(
            c,
            Filters {
                inbox: true,
                ..Default::default()
            }
        )
        .len(),
        0
    );
    organize::delete_collection(c, &col.id, ChildPolicy::MoveUp, 2).unwrap();
    assert_eq!(repo::count_items(c).unwrap(), 3);
    let n: i64 = c
        .query_row("SELECT count(*) FROM library_item_collection", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(n, 0);
    // Back in the Inbox: no collection left.
    assert_eq!(
        filtered(
            c,
            Filters {
                inbox: true,
                ..Default::default()
            }
        )
        .len(),
        3
    );
}

#[test]
fn delete_parent_collection_both_ways_ac48() {
    for policy in [ChildPolicy::MoveUp, ChildPolicy::DeleteAll] {
        let (_d, mut db) = open();
        let c = db.writer();
        let ids = items(c, 2);
        let top = organize::create_collection(c, "Üst", None, 1).unwrap();
        let p = organize::create_collection(c, "Ana", Some(&top.id), 1).unwrap();
        let kid = organize::create_collection(c, "Alt", Some(&p.id), 1).unwrap();
        let grandkid = organize::create_collection(c, "Torun", Some(&kid.id), 1).unwrap();
        organize::add_to_collection(c, &kid.id, &ids, 1).unwrap();
        organize::add_to_collection(c, &grandkid.id, &ids[..1], 1).unwrap();

        organize::delete_collection(c, &p.id, policy, 2).unwrap();
        assert_eq!(repo::count_items(c).unwrap(), 2, "{policy:?}");
        let left: Vec<String> = organize::list_collections(c)
            .unwrap()
            .into_iter()
            .map(|x| x.collection.name)
            .collect();
        match policy {
            ChildPolicy::MoveUp => {
                assert_eq!(left, ["Üst", "Alt", "Torun"]);
                assert_eq!(
                    organize::get_collection(c, &kid.id)
                        .unwrap()
                        .unwrap()
                        .parent_id,
                    Some(top.id.clone())
                );
                assert_eq!(organize::item_collections(c, &ids[0]).unwrap().len(), 2);
            }
            ChildPolicy::DeleteAll => {
                assert_eq!(left, ["Üst"]);
                assert!(organize::item_collections(c, &ids[0]).unwrap().is_empty());
            }
        }
    }
}

#[test]
fn collection_moves_refuse_cycles_and_list_depth_first() {
    let (_d, mut db) = open();
    let c = db.writer();
    let a = organize::create_collection(c, "A", None, 1).unwrap();
    let b = organize::create_collection(c, "B", Some(&a.id), 1).unwrap();
    let z = organize::create_collection(c, "Z", None, 1).unwrap();
    assert!(matches!(
        organize::move_collection(c, &a.id, Some(&b.id), 2),
        Err(LibraryError::Invalid("collection-cycle"))
    ));
    assert!(organize::move_collection(c, &a.id, Some(&a.id), 2).is_err());
    organize::move_collection(c, &z.id, Some(&b.id), 2).unwrap();
    let names: Vec<String> = organize::list_collections(c)
        .unwrap()
        .into_iter()
        .map(|x| x.collection.name)
        .collect();
    assert_eq!(names, ["A", "B", "Z"]);
    organize::move_collection(c, &z.id, None, 3).unwrap();
    assert_eq!(
        organize::get_collection(c, &z.id)
            .unwrap()
            .unwrap()
            .parent_id,
        None
    );
    organize::rename_collection(c, &z.id, "  Zeta  ", 4).unwrap();
    assert_eq!(
        organize::get_collection(c, &z.id).unwrap().unwrap().name,
        "Zeta"
    );
    // Depth limit.
    let mut parent = z.id.clone();
    for i in 0..organize::MAX_COLLECTION_DEPTH - 1 {
        parent = organize::create_collection(c, &format!("L{i}"), Some(&parent), 5)
            .unwrap()
            .id;
    }
    assert!(organize::create_collection(c, "too deep", Some(&parent), 6).is_err());
}

#[test]
fn collection_membership_undo() {
    let (_d, mut db) = open();
    let c = db.writer();
    let ids = items(c, 3);
    let col = organize::create_collection(c, "K", None, 1).unwrap();
    organize::add_to_collection(c, &col.id, &ids[..1], 1).unwrap();
    let added = organize::add_to_collection(c, &col.id, &ids, 2).unwrap();
    assert_eq!(added.len(), 2);
    undo::record(
        c,
        S,
        &undo::Operation::AddToCollection {
            collection_id: col.id.clone(),
            items: added,
        },
        2,
    )
    .unwrap();
    undo::undo(c, S, 3).unwrap();
    assert_eq!(
        filtered(
            c,
            Filters {
                collection_ids: vec![col.id.clone()],
                ..Default::default()
            }
        ),
        vec![ids[0].clone()]
    );

    let rows = organize::remove_from_collection(c, &col.id, &ids).unwrap();
    assert_eq!(rows.len(), 1);
    undo::record(
        c,
        S,
        &undo::Operation::RemoveFromCollection {
            collection_id: col.id.clone(),
            rows,
        },
        4,
    )
    .unwrap();
    undo::undo(c, S, 5).unwrap();
    assert_eq!(
        organize::item_collections(c, &ids[0]).unwrap(),
        vec![col.id.clone()]
    );
}

// --- Smart collections -----------------------------------------------------

#[test]
fn smart_collection_rules_validate_and_filter() {
    let (_d, mut db) = open();
    let c = db.writer();
    let ids = items(c, 4);
    c.execute(
        "UPDATE library_item SET duration_ms = 600000 WHERE id = ?1",
        [&ids[0]],
    )
    .unwrap();
    c.execute(
        "UPDATE library_item SET duration_ms = 60000 WHERE id = ?1",
        [&ids[1]],
    )
    .unwrap();
    let (t, _) = organize::get_or_create_tag(c, "uzun", 1).unwrap();
    organize::tag_items(c, &t.id, &ids[..2], 1).unwrap();

    assert_eq!(SmartRules::default().validate(), Err("no condition"));
    let bad: std::result::Result<SmartRules, _> =
        serde_json::from_str(r#"{"platforms":["youtube"],"sql":"1=1"}"#);
    assert!(bad.is_err(), "unknown rule fields are refused");
    let rules = SmartRules {
        tag_ids: vec![t.id.clone()],
        min_duration_ms: Some(300_000),
        ..Default::default()
    };
    let sc = organize::save_smart_collection(c, None, "Uzun videolar", &rules, 1).unwrap();
    assert_eq!(filtered(c, sc.rules.filters(1)), vec![ids[0].clone()]);
    assert_eq!(organize::list_smart_collections(c).unwrap().len(), 1);
    let bad = SmartRules {
        min_duration_ms: Some(10),
        max_duration_ms: Some(5),
        ..Default::default()
    };
    assert!(organize::save_smart_collection(c, Some(&sc.id), "x", &bad, 2).is_err());

    // A deleted tag makes the rule visibly invalid, not silently empty.
    organize::delete_tag(c, &t.id).unwrap();
    assert_eq!(sc.rules.missing_refs(c).unwrap(), 1);
    organize::delete_smart_collection(c, &sc.id).unwrap();
    assert!(organize::list_smart_collections(c).unwrap().is_empty());
}

// --- Removal, retention and orphaned media ---------------------------------

#[test]
fn remove_undo_then_retention_deletes_and_orphans_managed_media_ac37() {
    let (_d, mut db) = open();
    let c = db.writer();
    let r = root(c);
    let it = item(c, "Yönetilen", 1);
    let orig = asset(
        c,
        &it.id,
        &r.id,
        "media/a.mp4",
        AssetRole::Original,
        StorageKind::Managed,
    );
    let mut th = MediaAsset::new(
        &it.id,
        AssetRole::Thumbnail,
        MediaKind::Image,
        StorageKind::Managed,
        &r.id,
        "thumbs/a.jpg",
        1,
    );
    th.derivation = Some("{}".into());
    th.source_asset_id = Some(orig.id.clone());
    repo::insert_media_asset(c, &th).unwrap();
    let ext = item(c, "Harici", 2);
    asset(
        c,
        &ext.id,
        &r.id,
        "ext/b.mp4",
        AssetRole::Original,
        StorageKind::ExternalReference,
    );

    let ids = vec![it.id.clone(), ext.id.clone()];
    let removed = organize::remove_items(c, &ids, 1_000).unwrap();
    undo::record(
        c,
        S,
        &undo::Operation::Remove {
            at: 1_000,
            items: removed,
        },
        1_000,
    )
    .unwrap();
    assert!(filtered(c, Filters::default()).is_empty());
    assert_eq!(
        filtered(
            c,
            Filters {
                removed: true,
                ..Default::default()
            }
        )
        .len(),
        2
    );
    undo::undo(c, S, 1_001).unwrap();
    assert_eq!(filtered(c, Filters::default()).len(), 2);

    organize::remove_items(c, &ids, 2_000).unwrap();
    // Not yet expired.
    let day = removal::DAY_MS;
    assert_eq!(
        removal::purge_expired(c, 2_000 + 29 * day, 30)
            .unwrap()
            .items,
        0
    );
    let rep = removal::purge_expired(c, 2_001 + 30 * day, 30).unwrap();
    assert_eq!(rep.items, 2);
    assert_eq!(rep.orphaned, 1, "managed original kept");
    // Only the derived thumbnail file is deleted; the external file never.
    assert_eq!(
        rep.files_to_delete,
        vec![(r.id.clone(), "thumbs/a.jpg".to_owned())]
    );
    assert_eq!(repo::count_items(c).unwrap(), 0);
    let o = removal::list_orphans(c).unwrap();
    assert_eq!(o.len(), 1);
    assert_eq!(o[0].storage_ref, "media/a.mp4");
    assert_eq!(o[0].former_item_title.as_deref(), Some("Yönetilen"));
}

#[test]
fn explicit_delete_with_media_returns_the_file_and_live_items_are_refused() {
    let (_d, mut db) = open();
    let c = db.writer();
    let r = root(c);
    let it = item(c, "x", 1);
    asset(
        c,
        &it.id,
        &r.id,
        "media/x.mp4",
        AssetRole::Original,
        StorageKind::Managed,
    );
    assert!(
        removal::delete_permanently(c, &it.id, true, 2).is_err(),
        "live item"
    );
    organize::remove_items(c, std::slice::from_ref(&it.id), 3).unwrap();
    let rep = removal::delete_permanently(c, &it.id, true, 4).unwrap();
    assert_eq!(rep.orphaned, 0);
    assert_eq!(
        rep.files_to_delete,
        vec![(r.id.clone(), "media/x.mp4".to_owned())]
    );
    assert!(removal::list_orphans(c).unwrap().is_empty());
}

#[test]
fn reimport_orphan_makes_a_local_item_without_moving_ac53() {
    let (_d, mut db) = open();
    let c = db.writer();
    let r = root(c);
    let it = item(c, "Eski kayıt", 1);
    asset(
        c,
        &it.id,
        &r.id,
        "media/eski.mp4",
        AssetRole::Original,
        StorageKind::Managed,
    );
    organize::remove_items(c, std::slice::from_ref(&it.id), 2).unwrap();
    removal::delete_permanently(c, &it.id, false, 3).unwrap();
    let o = removal::list_orphans(c).unwrap().remove(0);

    let back = removal::reimport_orphan(c, &o.id, 4).unwrap();
    assert_eq!(back.kind, ItemKind::LocalVideo);
    assert_eq!(back.title.as_deref(), Some("Eski kayıt"));
    let assets = repo::assets_for_item(c, &back.id).unwrap();
    assert_eq!(assets.len(), 1);
    assert_eq!(
        (
            assets[0].storage_root_id.as_str(),
            assets[0].storage_ref.as_str()
        ),
        (r.id.as_str(), "media/eski.mp4")
    );
    assert_eq!(assets[0].storage_kind, StorageKind::Managed);
    assert!(removal::list_orphans(c).unwrap().is_empty());
}

#[test]
fn deleting_an_orphan_needs_a_connected_root() {
    let (_d, mut db) = open();
    let c = db.writer();
    let r = root(c);
    c.execute(
        "INSERT INTO orphan_media (id, storage_root_id, storage_ref, orphaned_at) VALUES ('o1', ?1, 'm/a.mp4', 1)",
        [&r.id],
    )
    .unwrap();
    c.execute("UPDATE storage_root SET status = 'disconnected'", [])
        .unwrap();
    assert!(removal::delete_orphan(c, "o1").is_err());
    c.execute("UPDATE storage_root SET status = 'available'", [])
        .unwrap();
    assert_eq!(
        removal::delete_orphan(c, "o1").unwrap(),
        (r.id.clone(), "m/a.mp4".to_owned())
    );
}

// --- Merge (F2.3) ----------------------------------------------------------

fn candidate(c: &Connection, a: &str, b: &str) -> String {
    repo::open_duplicate_candidate(c, a, b, DuplicateReason::SimilarUrl, 1)
        .unwrap()
        .id
}

#[test]
fn merge_preview_merge_and_undo() {
    let (_d, mut db) = open();
    let c = db.writer();
    let r = root(c);
    let t = item(c, "Hedef", 1);
    let s = item(c, "Kaynak", 2);
    let t = repo::set_notes(c, &t.id, t.revision, "hedef notu", 3).unwrap();
    let s = repo::set_notes(c, &s.id, s.revision, "kaynak notu", 3).unwrap();
    organize::set_favorite(c, std::slice::from_ref(&s.id), true, 3).unwrap();
    let media = asset(
        c,
        &s.id,
        &r.id,
        "media/k.mp4",
        AssetRole::Original,
        StorageKind::ExternalReference,
    );
    let (tag, _) = organize::get_or_create_tag(c, "ortak", 3).unwrap();
    let (only_s, _) = organize::get_or_create_tag(c, "yalnızkaynak", 3).unwrap();
    organize::tag_items(c, &tag.id, &[t.id.clone(), s.id.clone()], 3).unwrap();
    organize::tag_items(c, &only_s.id, std::slice::from_ref(&s.id), 3).unwrap();
    let col = organize::create_collection(c, "K", None, 3).unwrap();
    organize::add_to_collection(c, &col.id, std::slice::from_ref(&s.id), 3).unwrap();
    let cand = candidate(c, &t.id, &s.id);

    let p = merge::preview(c, &cand, &t.id).unwrap();
    assert_eq!(p.notes_after, "hedef notu\n\nkaynak notu");
    assert_eq!(
        p.tags_to_add
            .iter()
            .map(|x| x.name.as_str())
            .collect::<Vec<_>>(),
        ["yalnızkaynak"]
    );
    assert_eq!(p.collections_to_add.len(), 1);
    assert_eq!(p.assets_to_move.len(), 1);
    assert_eq!(p.alias_url.as_deref(), Some("https://example.com/v/2"));
    assert!(p.becomes_favorite);
    assert!(merge::preview(c, &cand, "someone-else").is_err());

    let m = merge::merge(c, &cand, &t.id, 10).unwrap();
    undo::record(c, S, &undo::Operation::MergeItems(m), 10).unwrap();
    let after = repo::get_item(c, &t.id).unwrap().unwrap();
    assert_eq!(after.user_notes, "hedef notu\n\nkaynak notu");
    assert!(after.is_favorite);
    assert_eq!(repo::assets_for_item(c, &t.id).unwrap()[0].id, media.id);
    assert!(repo::get_item(c, &s.id)
        .unwrap()
        .unwrap()
        .removed_at
        .is_some());
    assert!(merge::open_candidates(c).unwrap().is_empty());
    assert_eq!(find(c, "yalnizkaynak"), vec![t.id.clone()]);
    // Saving the source URL again now lands on the target.
    let saved =
        bookmark::save_bookmark(c, &FixedClock::new(11), "https://example.com/v/2").unwrap();
    assert_eq!(saved.outcome.item().id, t.id);

    let r = undo::undo(c, S, 12).unwrap().unwrap();
    assert_eq!(r.skipped, 0);
    let t2 = repo::get_item(c, &t.id).unwrap().unwrap();
    assert_eq!(t2.user_notes, "hedef notu");
    assert!(!t2.is_favorite);
    assert!(repo::get_item(c, &s.id)
        .unwrap()
        .unwrap()
        .removed_at
        .is_none());
    assert_eq!(repo::assets_for_item(c, &s.id).unwrap()[0].id, media.id);
    assert!(organize::item_collections(c, &t.id).unwrap().is_empty());
    assert_eq!(organize::item_tags(c, &t.id).unwrap().len(), 1);
    assert!(repo::aliases_for_item(c, &t.id).unwrap().is_empty());
    assert_eq!(merge::open_candidates(c).unwrap().len(), 1);
}

#[test]
fn merge_undo_skips_notes_edited_since_and_dismiss_closes() {
    let (_d, mut db) = open();
    let c = db.writer();
    let t = item(c, "Hedef", 1);
    let s = item(c, "Kaynak", 2);
    repo::set_notes(c, &s.id, s.revision, "kaynak", 3).unwrap();
    let cand = candidate(c, &t.id, &s.id);
    let m = merge::merge(c, &cand, &t.id, 10).unwrap();
    undo::record(c, S, &undo::Operation::MergeItems(m), 10).unwrap();
    let cur = repo::get_item(c, &t.id).unwrap().unwrap();
    repo::set_notes(c, &t.id, cur.revision, "kullanıcı yazdı", 11).unwrap();
    let r = undo::undo(c, S, 12).unwrap().unwrap();
    assert_eq!(r.skipped, 1);
    assert_eq!(
        repo::get_item(c, &t.id).unwrap().unwrap().user_notes,
        "kullanıcı yazdı"
    );

    let other = item(c, "Başka", 3);
    let cand2 = candidate(c, &t.id, &other.id);
    merge::dismiss(c, &cand2, 13).unwrap();
    let st: String = c
        .query_row(
            "SELECT status FROM duplicate_candidate WHERE id = ?1",
            params![cand2],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(st, "dismissed");
    assert!(merge::merge(c, &cand2, &t.id, 14).is_err());
}
