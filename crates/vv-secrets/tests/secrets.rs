//! Secret store contract (STATE.md F5.1, ADR-0022).

use vv_secrets::{MemoryStore, Secret, SecretError, SecretRef, SecretStore, UnavailableStore};

const CANARY: &[u8] = b"VV-CANARY-7f3c1e-secret-cookie=sessid";

fn contract(store: &dyn SecretStore) {
    let r = store.put(&Secret::new(CANARY.to_vec())).unwrap();
    assert!(r.as_str().starts_with("vvsec:"));
    assert!(!r.as_str().contains("CANARY"));
    assert_eq!(store.get(&r).unwrap().expose(), CANARY);

    // Two puts of the same value get unrelated references.
    let r2 = store.put(&Secret::new(CANARY.to_vec())).unwrap();
    assert_ne!(r, r2);
    // Reconciliation sees both (F5.3).
    let refs = store.refs().unwrap();
    assert!(refs.contains(&r) && refs.contains(&r2));

    store.delete(&r).unwrap();
    assert!(matches!(store.get(&r), Err(SecretError::NotFound)));
    // Deleting again is fine.
    store.delete(&r).unwrap();
    store.delete(&r2).unwrap();
    assert!(!store.refs().unwrap().contains(&r2));

    let big = Secret::new(vec![1; vv_secrets::MAX_SECRET_BYTES + 1]);
    assert!(matches!(store.put(&big), Err(SecretError::TooLarge)));
}

#[test]
fn memory_store_follows_the_contract() {
    let s = MemoryStore::new();
    contract(&s);
    assert!(s.is_empty());
}

#[test]
fn unavailable_store_never_stores() {
    let s = UnavailableStore;
    assert!(!s.available());
    assert!(matches!(
        s.put(&Secret::new(CANARY.to_vec())),
        Err(SecretError::Unavailable)
    ));
}

#[test]
fn secret_debug_and_errors_never_show_the_value() {
    let s = Secret::new(CANARY.to_vec());
    let dbg = format!("{s:?}");
    assert!(!dbg.contains("CANARY"), "{dbg}");
    assert!(dbg.contains("redacted"));
    let io = SecretError::Io(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "C:\\Users\\someone\\AppData\\x.vvsec",
    ));
    assert!(!io.to_string().contains("someone"));
}

#[test]
fn references_only_parse_in_the_generated_shape() {
    let r = SecretRef::generate();
    assert_eq!(SecretRef::parse(r.as_str()), Some(r));
    for bad in [
        "",
        "vvsec:",
        "vvsec:../../etc/passwd",
        "vvsec:..\\..\\x",
        "secret:0f8fad5b-d9cb-469f-a165-70867728950e",
        "vvsec:0F8FAD5B-D9CB-469F-A165-70867728950E",
        "vvsec:0f8fad5bd9cb469fa16570867728950e",
    ] {
        assert_eq!(SecretRef::parse(bad), None, "{bad}");
    }
}

#[cfg(not(windows))]
#[test]
fn os_store_is_unavailable_off_windows() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!vv_secrets::os_store(dir.path()).available());
}

#[cfg(windows)]
#[test]
fn dpapi_store_follows_the_contract_and_writes_no_plaintext() {
    let dir = tempfile::tempdir().unwrap();
    let store = vv_secrets::os_store(dir.path());
    assert!(store.available());
    contract(store.as_ref());

    let r = store.put(&Secret::new(CANARY.to_vec())).unwrap();
    let files: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(files.len(), 1, "{files:?}");
    let on_disk = std::fs::read(&files[0]).unwrap();
    assert!(
        !on_disk.windows(CANARY.len()).any(|w| w == CANARY),
        "plaintext canary on disk"
    );
    assert_eq!(store.get(&r).unwrap().expose(), CANARY);

    // A tampered blob does not decrypt into anything.
    let mut bad = on_disk.clone();
    let last = bad.len() - 1;
    bad[last] ^= 0xff;
    std::fs::write(&files[0], &bad).unwrap();
    assert!(matches!(store.get(&r), Err(SecretError::Os(_))));
    store.delete(&r).unwrap();
}
