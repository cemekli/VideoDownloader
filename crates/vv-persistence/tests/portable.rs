//! STATE.md F2.5–F2.6: portable JSON export/import and full database
//! backup/restore against real database files (SPEC §15; ADR-0017).
//! Covers AC-15, AC-16, AC-30, AC-50, AC-51 and AC-57; the playback row
//! part of AC-49 is checked here too.

use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;
use vv_persistence::library::organize::{self, SmartRules};
use vv_persistence::library::repo::{self, LibraryItem, MediaAsset, StorageRoot};
use vv_persistence::library::*;
use vv_persistence::portable::{self, ConflictPolicy, ExportOptions, ImportOptions, PortableError};
use vv_persistence::restore::{self, RestoreError};
use vv_persistence::{Database, OpenOptions};

const DAY: i64 = 86_400_000;

fn open(dir: &Path, name: &str) -> Database {
    Database::open(&dir.join(name), &OpenOptions::default()).unwrap()
}

fn root(c: &Connection, path: &Path) -> StorageRoot {
    let r = StorageRoot {
        id: new_id(),
        kind: StorageRootKind::ManagedMedia,
        label: "Media".into(),
        path: path.to_string_lossy().into_owned(),
        volume_id: None,
        status: StorageRootStatus::Available,
        created_at: 1,
        last_verified_at: None,
    };
    repo::insert_storage_root(c, &r).unwrap();
    r
}

fn bookmark(c: &Connection, n: i64, title: &str) -> LibraryItem {
    let mut it = LibraryItem::new(ItemKind::Bookmark, SourcePlatform::Generic, n);
    it.canonical_url = Some(format!("https://example.com/v/{n}"));
    it.title = Some(title.into());
    repo::insert_item(c, &it).unwrap();
    repo::write_search_document(c, &it).unwrap();
    it
}

fn local(c: &Connection, root: &StorageRoot, rel: &str, title: &str) -> (LibraryItem, MediaAsset) {
    let mut it = LibraryItem::new(ItemKind::LocalVideo, SourcePlatform::Local, 10);
    it.title = Some(title.into());
    repo::insert_item(c, &it).unwrap();
    let mut a = MediaAsset::new(
        &it.id,
        AssetRole::Original,
        MediaKind::Video,
        StorageKind::Managed,
        &root.id,
        rel,
        10,
    );
    a.size_bytes = Some(4);
    a.content_hash = Some(format!("{:0>64}", title.len()));
    a.hash_algorithm = Some("sha256".into());
    a.container = Some("mp4".into());
    a.duration_ms = Some(120_000);
    a.acquisition_status = AcquisitionStatus::Ready;
    repo::insert_media_asset(c, &a).unwrap();
    repo::write_search_document(c, &it).unwrap();
    (it, a)
}

/// A small library: notes, tags, a collection tree, a smart collection,
/// a removed item, a local file and a playback position.
fn sample(c: &Connection, media: &Path) -> (StorageRoot, Vec<String>) {
    let r = root(c, media);
    std::fs::create_dir_all(media.join("2026")).unwrap();
    std::fs::write(media.join("2026/clip.mp4"), b"data").unwrap();
    let a = bookmark(c, 1, "Ankara sunset");
    let b = bookmark(c, 2, "Istanbul ferry");
    let (l, asset) = local(c, &r, "2026/clip.mp4", "Holiday clip");
    repo::set_notes(c, &a.id, a.revision, "watch with friends", 5).unwrap();
    let (tag, _) = organize::get_or_create_tag(c, "travel", 5).unwrap();
    organize::tag_items(c, &tag.id, &[a.id.clone(), l.id.clone()], 5).unwrap();
    let parent = organize::create_collection(c, "Trips", None, 5).unwrap();
    let child = organize::create_collection(c, "Turkey", Some(&parent.id), 5).unwrap();
    organize::add_to_collection(c, &child.id, &[a.id.clone(), b.id.clone()], 5).unwrap();
    organize::save_smart_collection(
        c,
        None,
        "Travel in Turkey",
        &SmartRules {
            tag_ids: vec![tag.id.clone()],
            collection_ids: vec![child.id.clone()],
            ..Default::default()
        },
        5,
    )
    .unwrap();
    organize::remove_items(c, std::slice::from_ref(&b.id), 6).unwrap();
    playback::save(c, &l.id, &asset.id, 42_000, Some(120_000), 7).unwrap();
    repo::reindex_items(c, &[a.id.clone(), b.id.clone(), l.id.clone()]).unwrap();
    (r, vec![a.id, b.id, l.id])
}

fn count(c: &Connection, sql: &str) -> i64 {
    c.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn snapshot(c: &Connection) -> Vec<i64> {
    [
        "library_item",
        "media_asset",
        "tag",
        "collection",
        "smart_collection",
        "library_item_tag",
        "library_item_collection",
        "storage_root",
        "playback_state",
    ]
    .iter()
    .map(|t| count(c, &format!("SELECT count(*) FROM {t}")))
    .collect()
}

fn phrase(c: &Connection, p: &str) -> usize {
    repo::search_phrase_ids(c, p).unwrap().len()
}

#[test]
fn ac15_round_trip_into_an_empty_library_keeps_notes_tags_collections_and_relinks_media() {
    let d = tempfile::tempdir().unwrap();
    let mut src = open(d.path(), "src.sqlite");
    let (_, ids) = sample(src.writer(), &d.path().join("media"));
    let doc = portable::export(src.writer(), ExportOptions::default(), "0.0.1", 100 * DAY).unwrap();
    let file = d.path().join("library.json");
    portable::write_export(&doc, &file).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        !text.contains(&d.path().to_string_lossy().into_owned()),
        "default export carries no machine paths"
    );
    assert!(!text.contains("search_document"));

    let doc = portable::read_file(&file).unwrap();
    let mut dst = open(d.path(), "dst.sqlite");
    let moved = d.path().join("moved-media");
    std::fs::create_dir_all(moved.join("2026")).unwrap();
    std::fs::write(moved.join("2026/clip.mp4"), b"data").unwrap();
    let new_root = root(dst.writer(), &moved);

    let pre = portable::preview(dst.writer(), &doc).unwrap();
    assert_eq!(
        (
            pre.items_new,
            pre.items_matched,
            pre.items_skipped,
            pre.removed
        ),
        (3, 0, 0, 1)
    );
    assert_eq!(pre.roots.len(), 1);
    assert!(!pre.roots[0].has_path);

    let opts = ImportOptions {
        root_map: HashMap::from([(pre.roots[0].id.clone(), new_root.id.clone())]),
        ..Default::default()
    };
    let rep = portable::apply(dst.writer(), &doc, &opts, 200 * DAY).unwrap();
    assert_eq!(
        (
            rep.items_added,
            rep.media_linked,
            rep.media_missing,
            rep.placeholder_roots
        ),
        (3, 1, 0, 0)
    );
    assert_eq!(rep.removed_reset, 1);

    let c = dst.writer();
    let a = repo::get_item(c, &ids[0]).unwrap().unwrap();
    assert_eq!(a.user_notes, "watch with friends");
    assert_eq!(organize::item_tags(c, &ids[0]).unwrap()[0].name, "travel");
    assert_eq!(
        count(
            c,
            "SELECT count(*) FROM collection WHERE parent_id IS NOT NULL"
        ),
        1
    );
    assert_eq!(organize::item_collections(c, &ids[0]).unwrap().len(), 1);
    let smart = organize::list_smart_collections(c).unwrap();
    assert_eq!(smart.len(), 1);
    assert_eq!(
        smart[0].rules.missing_refs(c).unwrap(),
        0,
        "rule references were remapped"
    );
    // FTS was rebuilt from the imported rows: title, notes and tags are searchable.
    assert_eq!(phrase(c, "sunset"), 1);
    assert_eq!(phrase(c, "friends"), 1);
    assert_eq!(phrase(c, "travel"), 2);
    // AC-57: the removed item's 30 days start at import.
    assert_eq!(
        repo::get_item(c, &ids[1]).unwrap().unwrap().removed_at,
        Some(200 * DAY)
    );
    // The local file was relinked under the mapped root; playback came along.
    let assets = repo::assets_for_item(c, &ids[2]).unwrap();
    assert_eq!(assets[0].storage_root_id, new_root.id);
    assert_eq!(assets[0].integrity_state, IntegrityState::Unknown);
    let p = playback::get(c, &ids[2]).unwrap().unwrap();
    assert_eq!(p.position_ms, 42_000);
    assert!(p.resumable());

    // Importing the same file again matches every item instead of duplicating.
    let before = snapshot(c);
    let again = portable::apply(c, &doc, &opts, 201 * DAY).unwrap();
    assert_eq!((again.items_added, again.items_matched), (0, 3));
    assert_eq!(snapshot(c), before);
}

#[test]
fn unmapped_roots_become_placeholders_and_conflicts_keep_existing_fields() {
    let d = tempfile::tempdir().unwrap();
    let mut src = open(d.path(), "src.sqlite");
    let (_, ids) = sample(src.writer(), &d.path().join("media"));
    let doc = portable::export(src.writer(), ExportOptions::default(), "0.0.1", 100).unwrap();

    let mut dst = open(d.path(), "dst.sqlite");
    let c = dst.writer();
    let mine = bookmark(c, 1, "My own title");
    repo::set_notes(c, &mine.id, mine.revision, "my note", 5).unwrap();
    let rep = portable::apply(c, &doc, &ImportOptions::default(), 300).unwrap();
    assert_eq!(
        (
            rep.items_added,
            rep.items_matched,
            rep.placeholder_roots,
            rep.media_missing
        ),
        (2, 1, 1, 1)
    );
    assert_eq!(
        repo::get_item(c, &mine.id).unwrap().unwrap().user_notes,
        "my note",
        "keep_existing is the default"
    );
    assert_eq!(
        organize::item_tags(c, &mine.id).unwrap().len(),
        1,
        "tags are unioned onto the match"
    );
    let a = &repo::assets_for_item(c, &ids[2]).unwrap()[0];
    assert_eq!(a.integrity_state, IntegrityState::Missing);
    let r = repo::get_storage_root(c, &a.storage_root_id)
        .unwrap()
        .unwrap();
    assert!(r.path.starts_with(portable::UNMAPPED_PREFIX));
    assert_eq!(r.status, StorageRootStatus::Disconnected);

    let replace = ImportOptions {
        policy: ConflictPolicy::ReplaceUserFields,
        ..Default::default()
    };
    portable::apply(c, &doc, &replace, 400).unwrap();
    assert_eq!(
        repo::get_item(c, &mine.id).unwrap().unwrap().user_notes,
        "watch with friends"
    );
}

#[test]
fn ac16_bad_or_future_files_leave_the_library_unchanged() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path(), "db.sqlite");
    sample(db.writer(), &d.path().join("media"));
    let before = snapshot(db.writer());
    for (bytes, future) in [
        (
            &br#"{"format":"videovault-library","schema_version":2,"items":[{"id":"x"}]}"#[..],
            true,
        ),
        (&b"{ not json"[..], false),
        (
            &br#"{"format":"videovault-library","schema_version":1,"items":"nope"}"#[..],
            false,
        ),
    ] {
        let err = portable::parse(bytes).unwrap_err();
        assert_eq!(
            matches!(err, PortableError::UnsupportedSchema { found: 2, .. }),
            future,
            "{err}"
        );
    }
    // A file that parses but breaks a schema rule rolls back as a whole.
    let mut doc = portable::export(db.writer(), ExportOptions::default(), "0", 1).unwrap();
    let mut good = doc.items[0].clone();
    good.insert("id".into(), "new-a".into());
    good.insert("canonical_url".into(), "https://example.com/fresh".into());
    let mut bad = good.clone();
    bad.insert("id".into(), "new-b".into());
    bad.insert("canonical_url".into(), "https://example.com/fresh2".into());
    bad.insert("visibility".into(), "everyone".into());
    doc.items = vec![good, bad];
    assert!(portable::apply(db.writer(), &doc, &ImportOptions::default(), 2).is_err());
    assert_eq!(snapshot(db.writer()), before);
}

#[test]
fn ac30_references_that_leave_their_root_refuse_the_whole_import() {
    let d = tempfile::tempdir().unwrap();
    let mut src = open(d.path(), "src.sqlite");
    sample(src.writer(), &d.path().join("media"));
    let doc = portable::export(src.writer(), ExportOptions::default(), "0", 1).unwrap();
    let json = serde_json::to_string(&doc).unwrap();
    let mut dst = open(d.path(), "dst.sqlite");
    let before = snapshot(dst.writer());
    for evil in [
        "../../outside.mp4",
        "..\\\\..\\\\outside.mp4",
        "C:\\\\Windows\\\\x.mp4",
        "/etc/passwd",
    ] {
        let bad = json.replace("2026/clip.mp4", evil);
        assert!(
            matches!(
                portable::parse(bad.as_bytes()),
                Err(PortableError::UnsafePath(_))
            ),
            "{evil}"
        );
    }
    // A link inside the mapped root that points outside it is refused at apply.
    let mapped = d.path().join("mapped");
    std::fs::create_dir_all(&mapped).unwrap();
    let outside = d.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("clip.mp4"), b"data").unwrap();
    if !link_dir(&outside, &mapped.join("2026")) {
        eprintln!("skipping the link half: this machine cannot make directory links");
        return;
    }
    let r = root(dst.writer(), &mapped);
    let before = {
        let mut b = before;
        b[7] += 1;
        b
    };
    let pre = portable::preview(dst.writer(), &doc).unwrap();
    let opts = ImportOptions {
        root_map: HashMap::from([(pre.roots[0].id.clone(), r.id)]),
        ..Default::default()
    };
    assert!(matches!(
        portable::apply(dst.writer(), &doc, &opts, 2),
        Err(PortableError::UnsafePath(_))
    ));
    assert_eq!(snapshot(dst.writer()), before);
}

#[cfg(unix)]
fn link_dir(target: &Path, link: &Path) -> bool {
    std::os::unix::fs::symlink(target, link).is_ok()
}

#[cfg(windows)]
fn link_dir(target: &Path, link: &Path) -> bool {
    // A junction needs no admin rights, unlike a symlink.
    std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .is_ok_and(|o| o.status.success())
}

#[test]
fn ac51_an_approved_absolute_file_becomes_an_external_reference_and_is_not_written() {
    let d = tempfile::tempdir().unwrap();
    let mut src = open(d.path(), "src.sqlite");
    sample(src.writer(), &d.path().join("media"));
    let doc = portable::export(
        src.writer(),
        ExportOptions {
            include_absolute_paths: true,
        },
        "0",
        1,
    )
    .unwrap();
    assert!(doc.storage_roots[0].contains_key("path"));
    let file = d.path().join("media/2026/clip.mp4");
    let modified = std::fs::metadata(&file).unwrap().modified().unwrap();

    // Without approval the absolute path is not used.
    let mut dst = open(d.path(), "dst.sqlite");
    let rep = portable::apply(dst.writer(), &doc, &ImportOptions::default(), 2).unwrap();
    assert_eq!((rep.external_roots, rep.placeholder_roots), (0, 1));

    let mut dst = open(d.path(), "dst2.sqlite");
    let pre = portable::preview(dst.writer(), &doc).unwrap();
    assert_eq!(pre.absolute_paths, 1);
    let rep = portable::apply(
        dst.writer(),
        &doc,
        &ImportOptions {
            use_original_locations: true,
            ..Default::default()
        },
        2,
    )
    .unwrap();
    assert_eq!(
        (rep.external_roots, rep.media_linked, rep.placeholder_roots),
        (1, 1, 0)
    );
    let c = dst.writer();
    let (kind, path, storage_kind, rel): (String, String, String, String) = c
        .query_row(
            "SELECT r.kind, r.path, a.storage_kind, a.storage_ref FROM media_asset a JOIN storage_root r ON r.id = a.storage_root_id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        (kind.as_str(), storage_kind.as_str(), rel.as_str()),
        ("external_import", "external_reference", "clip.mp4")
    );
    assert_eq!(Path::new(&path), file.parent().unwrap());
    assert_eq!(std::fs::read(&file).unwrap(), b"data");
    assert_eq!(
        std::fs::metadata(&file).unwrap().modified().unwrap(),
        modified
    );
}

#[test]
fn ac50_backup_change_restore_returns_to_the_backup_and_saves_the_current_state_first() {
    let d = tempfile::tempdir().unwrap();
    let backups = d.path().join("backups");
    let mut db = open(d.path(), "library.sqlite");
    let (_, ids) = sample(db.writer(), &d.path().join("media"));
    let saved = restore::backup_now(&db, &backups, 1_000).unwrap();
    let summary = restore::inspect(&saved).unwrap();
    assert_eq!(
        (
            summary.items,
            summary.removed,
            summary.collections,
            summary.tags,
            summary.media_files
        ),
        (2, 1, 2, 1, 1)
    );

    // Change the library after the backup.
    bookmark(db.writer(), 99, "Added later");
    let rev = repo::get_item(db.writer(), &ids[0])
        .unwrap()
        .unwrap()
        .revision;
    repo::set_notes(db.writer(), &ids[0], rev, "changed", 2_000).unwrap();

    let rep = restore::restore(&mut db, &saved, &backups, 3_000).unwrap();
    assert!(rep.migrated.is_empty());
    let c = db.writer();
    assert_eq!(repo::count_items(c).unwrap(), 3);
    assert_eq!(
        repo::get_item(c, &ids[0]).unwrap().unwrap().user_notes,
        "watch with friends"
    );
    assert_eq!(
        phrase(c, "later"),
        0,
        "search index matches the restored rows"
    );
    assert_eq!(phrase(c, "friends"), 1);
    // The state before the restore is a backup of its own, with the later item.
    let pre = restore::inspect(&rep.pre_restore_backup).unwrap();
    assert_eq!(pre.items, 3);
    let kinds: Vec<_> = restore::list(&backups)
        .iter()
        .map(|b| b.kind.as_str())
        .collect();
    assert_eq!(kinds, ["pre_restore", "manual"]);
}

#[test]
fn ac50_an_older_schema_backup_is_migrated_on_restore() {
    use vv_persistence::migrations::MIGRATIONS;
    let d = tempfile::tempdir().unwrap();
    // A library written by the F2.3 app (schema 3), as a pre-migration backup would be.
    let old_path = d.path().join("old.sqlite");
    {
        let mut old =
            Database::open_with(&old_path, &OpenOptions::default(), &MIGRATIONS[..3]).unwrap();
        bookmark(old.writer(), 1, "From schema three");
    }
    let mut db = open(d.path(), "library.sqlite");
    assert_eq!(restore::inspect(&old_path).unwrap().schema_version, 3);
    let rep = restore::restore(&mut db, &old_path, &d.path().join("backups"), 5).unwrap();
    let later: Vec<_> = (4..=MIGRATIONS.len() as u32).collect();
    assert_eq!(rep.migrated, later);
    assert_eq!(phrase(db.writer(), "three"), 1);
    assert_eq!(count(db.writer(), "SELECT count(*) FROM playback_state"), 0);
}

#[test]
fn ac57_restored_removed_items_start_their_retention_at_restore() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path(), "library.sqlite");
    let (_, ids) = sample(db.writer(), &d.path().join("media"));
    let saved = restore::backup_now(&db, &d.path().join("backups"), 7).unwrap();
    assert_eq!(
        restore::inspect(&saved).unwrap().removed,
        1,
        "the preview names the affected count"
    );
    let now = 200 * DAY;
    restore::restore(&mut db, &saved, &d.path().join("backups"), now).unwrap();
    assert_eq!(
        repo::get_item(db.writer(), &ids[1])
            .unwrap()
            .unwrap()
            .removed_at,
        Some(now)
    );
    let purged = removal::purge_expired(db.writer(), now + DAY, 30).unwrap();
    assert_eq!(
        purged.items, 0,
        "nothing is deleted right after the restore"
    );
}

#[test]
fn restore_refuses_bad_files_and_leaves_the_library_alone() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path(), "library.sqlite");
    sample(db.writer(), &d.path().join("media"));
    let before = snapshot(db.writer());
    let junk = d.path().join("junk.sqlite");
    std::fs::write(&junk, b"definitely not sqlite").unwrap();
    assert!(matches!(
        restore::restore(&mut db, &junk, d.path(), 1),
        Err(RestoreError::Unreadable)
    ));
    let foreign = d.path().join("foreign.sqlite");
    Connection::open(&foreign)
        .unwrap()
        .execute_batch("CREATE TABLE t (x INTEGER)")
        .unwrap();
    assert!(matches!(
        restore::restore(&mut db, &foreign, d.path(), 1),
        Err(RestoreError::NotALibrary)
    ));
    // A backup from a newer app (one migration this app does not know).
    let newer = d.path().join("newer.sqlite");
    restore::backup_now(&db, d.path(), 2)
        .map(|p| std::fs::rename(p, &newer))
        .unwrap()
        .unwrap();
    Connection::open(&newer)
        .unwrap()
        .execute("INSERT INTO schema_migrations (version, name, checksum, applied_at_ms) VALUES (99, 'future', 'x', 1)", params![])
        .unwrap();
    assert!(matches!(
        restore::restore(&mut db, &newer, d.path(), 1),
        Err(RestoreError::TooNew { backup: 99, .. })
    ));
    assert_eq!(snapshot(db.writer()), before);
    assert!(
        restore::list(d.path())
            .iter()
            .all(|b| b.kind != restore::BackupKind::PreRestore),
        "no pre-restore backup for a refused file"
    );
}

#[test]
fn ac49_playback_position_is_kept_per_item_and_completion_ends_the_offer() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path(), "library.sqlite");
    let c = db.writer();
    let r = root(c, d.path());
    let (it, a) = local(c, &r, "a.mp4", "Lecture");
    let other = bookmark(c, 5, "Other");
    assert!(
        playback::save(c, &other.id, &a.id, 1, None, 1).is_err(),
        "asset of another item"
    );
    assert!(
        !playback::save(c, &it.id, &a.id, 2_000, Some(600_000), 1)
            .unwrap()
            .resumable(),
        "barely started"
    );
    let s = playback::save(c, &it.id, &a.id, 300_000, Some(600_000), 2).unwrap();
    assert!(s.resumable());
    let s = playback::save(c, &it.id, &a.id, 595_000, Some(600_000), 3).unwrap();
    assert_eq!(s.completed_at, Some(3));
    assert!(!s.resumable());
    // Watching again from the start clears completion.
    let s = playback::save(c, &it.id, &a.id, 30_000, Some(600_000), 4).unwrap();
    assert!(s.completed_at.is_none() && s.resumable());
    assert_eq!(
        repo::get_item(c, &it.id).unwrap().unwrap().revision,
        it.revision,
        "not a user edit"
    );
}
