//! STATE.md F3.9: `storage_move` (SPEC §13.1, DATA-08, AC-43).

mod common;

use common::*;
use std::path::{Path, PathBuf};
use vv_engine::storage_move::{self, MovePayload};
use vv_persistence::jobs::{self, JobKind, JobStatus};
use vv_persistence::library::repo::{self, MediaAsset, StorageRoot};
use vv_persistence::library::*;
use vv_persistence::rusqlite::params;
use vv_persistence::settings;

struct Fixture {
    lib: Lib,
    src: StorageRoot,
    dst: StorageRoot,
    assets: Vec<(MediaAsset, Vec<u8>)>,
}

fn sha(b: &[u8]) -> String {
    let d = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(d.path(), b).unwrap();
    vv_import::import::sha256(d.path(), &Default::default())
        .unwrap()
        .unwrap()
}

fn root(lib: &Lib, dir: &Path) -> StorageRoot {
    std::fs::create_dir_all(dir).unwrap();
    lib.write(|c| {
        vv_engine::storage::ensure_root(
            c,
            lib.clock.as_ref(),
            dir,
            StorageRootKind::ManagedMedia,
            "m",
        )
        .unwrap()
    })
}

fn fixture(n: usize) -> Fixture {
    let lib = library(Setup::fake());
    let base = lib.dir.path().canonicalize().unwrap();
    let src = root(&lib, &base.join("old"));
    let dst = root(&lib, &base.join("new"));
    lib.write(|c| settings::set_download_root(c, &src.id, 1).unwrap());
    let mut assets = Vec::new();
    for i in 0..n {
        let item = lib.save(&format!("https://fake.test/m{i}"));
        let mut a = MediaAsset::new(
            &item,
            AssetRole::Original,
            MediaKind::Video,
            StorageKind::Managed,
            &src.id,
            "",
            1,
        );
        a.storage_ref = format!("media/{item}/{}.mp4", a.id);
        let bytes = format!("video bytes {i} {}", "x".repeat(1000 * (i + 1))).into_bytes();
        // Half of them have no hash yet: the move computes it first.
        if i % 2 == 0 {
            a.content_hash = Some(sha(&bytes));
            a.hash_algorithm = Some("sha256".into());
        }
        a.size_bytes = Some(bytes.len() as i64);
        a.acquisition_status = AcquisitionStatus::Ready;
        a.integrity_state = IntegrityState::Verified;
        let f = vv_import::fsutil::resolve_ref(Path::new(&src.path), &a.storage_ref);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, &bytes).unwrap();
        lib.write(|c| repo::insert_media_asset(c, &a).unwrap());
        assets.push((a, bytes));
    }
    Fixture {
        lib,
        src,
        dst,
        assets,
    }
}

fn path_of(root: &StorageRoot, a: &MediaAsset) -> PathBuf {
    vv_import::fsutil::resolve_ref(Path::new(&root.path), &a.storage_ref)
}

/// Each asset resolves to exactly one existing copy with the original bytes.
fn assert_consistent(f: &Fixture) {
    for (a, bytes) in &f.assets {
        let now = f
            .lib
            .write(|c| repo::get_media_asset(c, &a.id).unwrap().unwrap());
        let root = f.lib.write(|c| {
            repo::get_storage_root(c, &now.storage_root_id)
                .unwrap()
                .unwrap()
        });
        let p = path_of(&root, &now);
        assert_eq!(
            &std::fs::read(&p).unwrap(),
            bytes,
            "{} resolves to its bytes",
            a.id
        );
    }
}

#[test]
fn a_move_switches_every_asset_and_then_the_active_root() {
    let f = fixture(3);
    let now = f.lib.clock.now_ms();
    let job = f.lib.write(|c| {
        storage_move::enqueue(c, &f.src.id, &f.dst.id, now)
            .unwrap()
            .id
    });
    f.lib.run();
    assert_eq!(f.lib.job(&job).status, JobStatus::Succeeded);
    for (a, _) in &f.assets {
        let now = f
            .lib
            .write(|c| repo::get_media_asset(c, &a.id).unwrap().unwrap());
        assert_eq!(now.storage_root_id, f.dst.id);
        assert!(
            !path_of(&f.src, a).exists(),
            "old copy removed after commit"
        );
    }
    assert_consistent(&f);
    let s = f.lib.write(|c| settings::get(c).unwrap());
    assert_eq!(
        s.active_download_root_id.as_deref(),
        Some(f.dst.id.as_str())
    );
    // Both roots stay registered.
    assert_eq!(
        f.lib.write(|c| repo::list_storage_roots(c).unwrap()).len(),
        2
    );
}

#[test]
fn a_move_killed_at_every_step_resumes_without_losing_a_copy() {
    // Four assets, left by a "crash" at four different points.
    let f = fixture(4);
    let now = f.lib.clock.now_ms();
    let job = f.lib.write(|c| {
        storage_move::enqueue(c, &f.src.id, &f.dst.id, now)
            .unwrap()
            .id
    });
    let lease = f
        .lib
        .write(|c| jobs::claim(c, "old-run", &[JobKind::StorageMove], 1, false, now))
        .unwrap()
        .unwrap();
    let p: MovePayload = serde_json::from_value(lease.job.payload.clone()).unwrap();
    assert_eq!(p.asset_ids.len(), 4);
    let moving = Path::new(&f.dst.path).join(storage_move::MOVING_DIR);
    std::fs::create_dir_all(&moving).unwrap();
    for (i, (a, bytes)) in f.assets.iter().enumerate() {
        let step = ["pending", "copied", "verified", "switched"][i];
        let staged = moving.join(&a.id);
        let fin = path_of(&f.dst, a);
        match step {
            // Copied, but the copy is torn (killed mid-copy): must be redone.
            "copied" => std::fs::write(&staged, &bytes[..bytes.len() / 2]).unwrap(),
            // Renamed into place, but the switch was not committed.
            "verified" => {
                std::fs::create_dir_all(fin.parent().unwrap()).unwrap();
                std::fs::write(&fin, bytes).unwrap();
            }
            // Committed; the old copy is still there.
            "switched" => {
                std::fs::create_dir_all(fin.parent().unwrap()).unwrap();
                std::fs::write(&fin, bytes).unwrap();
                f.lib.write(|c| {
                    repo::relocate_asset(c, &a.id, &f.dst.id, &a.storage_ref, None).unwrap()
                });
            }
            _ => {}
        }
        let hash = sha(bytes);
        f.lib.write(|c| {
            c.execute(
                "INSERT INTO storage_move_step (job_id, asset_id, step, source_root_id, source_ref, target_ref, size_bytes, content_hash, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?7, 1)",
                params![job, a.id, step, f.src.id, a.storage_ref, bytes.len() as i64, (step != "pending").then_some(hash)],
            )
            .unwrap()
        });
    }
    // Before restart every asset already resolves to one good copy.
    assert_consistent(&f);
    f.lib.engine.reconcile();
    f.lib.run();
    assert_eq!(f.lib.job(&job).status, JobStatus::Succeeded, "AC-43");
    assert_consistent(&f);
    for (a, _) in &f.assets {
        let now = f
            .lib
            .write(|c| repo::get_media_asset(c, &a.id).unwrap().unwrap());
        assert_eq!(now.storage_root_id, f.dst.id);
        assert!(!path_of(&f.src, a).exists());
    }
    assert!(!moving.exists() || std::fs::read_dir(&moving).unwrap().next().is_none());
}

#[test]
fn a_cancelled_move_leaves_each_asset_in_exactly_one_root() {
    let f = fixture(2);
    let now = f.lib.clock.now_ms();
    let job = f.lib.write(|c| {
        storage_move::enqueue(c, &f.src.id, &f.dst.id, now)
            .unwrap()
            .id
    });
    f.lib
        .write(|c| jobs::cancel(c, &job, jobs::CancelReason::User, now).unwrap());
    f.lib.run();
    assert_eq!(f.lib.job(&job).status, JobStatus::Canceled);
    assert_consistent(&f);
    let s = f.lib.write(|c| settings::get(c).unwrap());
    assert_eq!(
        s.active_download_root_id.as_deref(),
        Some(f.src.id.as_str())
    );
}
