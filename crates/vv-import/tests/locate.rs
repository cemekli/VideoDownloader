//! "Locate drive…" (DATA-09): a USB drive came back under another letter.
//! Files are stored relative to their root, so pointing the root at the new
//! folder brings them back without copying; a folder that does not hold the
//! root's files is refused and nothing changes.

use std::path::Path;
use vv_import::roots::{check_storage, locate, LocateError};
use vv_persistence::library::repo::{self, LibraryItem, MediaAsset, StorageRoot};
use vv_persistence::library::*;
use vv_persistence::rusqlite::Connection;
use vv_persistence::{settings, Database, OpenOptions};

fn root(c: &Connection, path: &Path) -> StorageRoot {
    let r = StorageRoot {
        id: new_id(),
        kind: StorageRootKind::ManagedMedia,
        label: "media".into(),
        path: vv_import::fsutil::canonical(path)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned(),
        volume_id: None,
        status: StorageRootStatus::Available,
        created_at: 1,
        last_verified_at: Some(1),
    };
    repo::insert_storage_root(c, &r).unwrap();
    r
}

fn video(c: &Connection, root: &StorageRoot, dir: &Path, name: &str) -> String {
    let mut it = LibraryItem::new(ItemKind::LocalVideo, SourcePlatform::Local, 1);
    it.title = Some(name.into());
    repo::insert_item(c, &it).unwrap();
    let file = format!("Videos/{name}.mp4");
    std::fs::create_dir_all(dir.join("Videos")).unwrap();
    std::fs::write(dir.join(&file), name.as_bytes()).unwrap();
    let mut a = MediaAsset::new(
        &it.id,
        AssetRole::Original,
        MediaKind::Video,
        StorageKind::Managed,
        &root.id,
        &file,
        1,
    );
    a.size_bytes = Some(name.len() as i64);
    a.acquisition_status = AcquisitionStatus::Ready;
    a.integrity_state = IntegrityState::Verified;
    repo::insert_media_asset(c, &a).unwrap();
    a.id
}

#[test]
fn a_drive_under_a_new_letter_is_located_without_copying() {
    let d = tempfile::tempdir().unwrap();
    let mut db = Database::open(&d.path().join("library.sqlite"), &OpenOptions::default()).unwrap();
    let clock = FixedClock::new(1_000);
    let c = db.writer();
    let old = d.path().join("F-drive/VideoVault");
    std::fs::create_dir_all(&old).unwrap();
    let r = root(c, &old);
    settings::set_download_root(c, &r.id, 1).unwrap();
    let ids: Vec<_> = ["deniz", "harrier", "kadikoy"]
        .iter()
        .map(|n| video(c, &r, &old, n))
        .collect();

    // The drive comes back as E: (the folder moves), F: is gone.
    let new = d.path().join("E-drive/VideoVault");
    std::fs::create_dir_all(new.parent().unwrap()).unwrap();
    std::fs::rename(&old, &new).unwrap();
    check_storage(c, &clock).unwrap();
    assert_eq!(
        repo::get_storage_root(c, &r.id).unwrap().unwrap().status,
        StorageRootStatus::Disconnected
    );

    // A folder without those files is refused; the root stays as it was.
    let wrong = d.path().join("other");
    std::fs::create_dir_all(&wrong).unwrap();
    match locate(c, &clock, &r.id, &wrong) {
        Err(LocateError::NotThere(l)) => assert_eq!((l.found, l.checked), (0, 3)),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        repo::get_storage_root(c, &r.id).unwrap().unwrap().status,
        StorageRootStatus::Disconnected
    );

    // The right folder: the root points there and every file is verified.
    let l = locate(c, &clock, &r.id, &new).unwrap();
    assert_eq!((l.found, l.checked), (3, 3));
    let after = repo::get_storage_root(c, &r.id).unwrap().unwrap();
    assert_eq!(after.status, StorageRootStatus::Available);
    assert_eq!(
        after.path,
        vv_import::fsutil::canonical(&new)
            .unwrap()
            .to_str()
            .unwrap()
    );
    for id in &ids {
        assert_eq!(
            repo::get_media_asset(c, id)
                .unwrap()
                .unwrap()
                .integrity_state,
            IntegrityState::Verified
        );
    }
    assert!(new.join("Videos/deniz.mp4").is_file(), "nothing was moved");
}

#[test]
fn a_folder_already_picked_again_in_settings_takes_the_files_over() {
    let d = tempfile::tempdir().unwrap();
    let mut db = Database::open(&d.path().join("library.sqlite"), &OpenOptions::default()).unwrap();
    let clock = FixedClock::new(1_000);
    let c = db.writer();
    let old = d.path().join("F-drive/VideoVault");
    std::fs::create_dir_all(&old).unwrap();
    let r = root(c, &old);
    settings::set_download_root(c, &r.id, 1).unwrap();
    let id = video(c, &r, &old, "harrier");
    let new = d.path().join("E-drive/VideoVault");
    std::fs::create_dir_all(new.parent().unwrap()).unwrap();
    std::fs::rename(&old, &new).unwrap();
    // The user picked E:\VideoVault in Settings: a second root for it exists.
    let again = root(c, &new);

    locate(c, &clock, &r.id, &new).unwrap();
    assert!(
        repo::get_storage_root(c, &r.id).unwrap().is_none(),
        "the old root is gone"
    );
    let a = repo::get_media_asset(c, &id).unwrap().unwrap();
    assert_eq!(a.storage_root_id, again.id);
    assert_eq!(a.integrity_state, IntegrityState::Verified);
    assert_eq!(
        settings::get(c).unwrap().active_download_root_id.as_deref(),
        Some(again.id.as_str())
    );
}
