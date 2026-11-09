//! STATE.md F5.1: AuthProfile, consent and the OS secret store. Canary test:
//! a secret attached to a profile is in no database file, export or error
//! (SPEC §11.6, §12.1, §12.2, §20.3, AC-18, AC-54; ADR-0022).

use vv_domain::error::ErrorCode;
use vv_domain::normalize::Platform;
use vv_persistence::auth::{
    self, AuthError, BrowserKind, Method, NewProfile, Operation, ProfileState,
};
use vv_persistence::classify::{self, Gone};
use vv_persistence::library::bookmark::{save_bookmark, SaveOutcome};
use vv_persistence::library::FixedClock;
use vv_persistence::portable::{self, ExportOptions};
use vv_persistence::{Database, OpenOptions};
use vv_secrets::{MemoryStore, Secret};

const CANARY: &str = "VV-CANARY-5d1f0b reddit_session=eyJhbGciOi.canary-cookie-value";
const ALL: &[Operation] = &[Operation::Metadata, Operation::Download, Operation::Verify];

fn open(dir: &std::path::Path) -> Database {
    Database::open(&dir.join("library.sqlite"), &OpenOptions::default()).unwrap()
}

fn firefox_reddit<'a>(scope: &'a [Operation]) -> NewProfile<'a> {
    NewProfile {
        provider: Platform::Reddit,
        label: "Reddit (Firefox)",
        method: Method::BrowserCookies,
        browser_kind: Some(BrowserKind::Firefox),
        profile_locator: Some("default-release"),
        consent_scope: scope,
    }
}

fn contains(bytes: &[u8], needle: &str) -> bool {
    bytes.windows(needle.len()).any(|w| w == needle.as_bytes())
}

#[test]
fn profile_needs_consent_one_platform_and_the_right_method_fields() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();

    let no_consent = auth::create(c, &firefox_reddit(&[]), 1);
    assert!(matches!(no_consent, Err(AuthError::Invalid(_))));

    let mut generic = firefox_reddit(ALL);
    generic.provider = Platform::Generic;
    assert!(matches!(
        auth::create(c, &generic, 1),
        Err(AuthError::Invalid(_))
    ));

    let mut no_browser = firefox_reddit(ALL);
    no_browser.browser_kind = None;
    assert!(matches!(
        auth::create(c, &no_browser, 1),
        Err(AuthError::Invalid(_))
    ));

    let mut file_with_browser = firefox_reddit(ALL);
    file_with_browser.method = Method::CookieFile;
    assert!(matches!(
        auth::create(c, &file_with_browser, 1),
        Err(AuthError::Invalid(_))
    ));

    let p = auth::create(c, &firefox_reddit(&[Operation::Download]), 10).unwrap();
    assert_eq!(p.provider, Platform::Reddit);
    assert_eq!(
        p.session_state,
        ProfileState::NeedsVerification,
        "a reference is not a session (§12.2)"
    );
    assert_eq!(p.consent_scope, vec![Operation::Download]);
    assert_eq!(p.consent_at, Some(10));
    assert!(!p.has_secret);
    assert_eq!(auth::list(c).unwrap(), vec![p]);
}

#[test]
fn usable_only_for_its_platform_and_consented_operations() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let p = auth::create(c, &firefox_reddit(&[Operation::Download]), 10).unwrap();

    let ids = |pl, op| {
        auth::usable_for(c, pl, op)
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        ids(Platform::Reddit, Operation::Download),
        vec![p.id.clone()]
    );
    assert!(ids(Platform::Reddit, Operation::Metadata).is_empty());
    assert!(ids(Platform::Vk, Operation::Download).is_empty());

    // Widening and withdrawing consent.
    auth::set_consent(c, &p.id, ALL, 20).unwrap();
    assert_eq!(
        ids(Platform::Reddit, Operation::Metadata),
        vec![p.id.clone()]
    );
    let w = auth::set_consent(c, &p.id, &[], 30).unwrap();
    assert_eq!(w.consent_at, None);
    assert!(ids(Platform::Reddit, Operation::Download).is_empty());

    // Unsupported profiles are not offered.
    auth::set_consent(c, &p.id, ALL, 40).unwrap();
    auth::mark_unsupported(c, &p.id, 41).unwrap();
    assert!(ids(Platform::Reddit, Operation::Download).is_empty());
}

#[test]
fn session_state_follows_the_evidence_rule() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let p = auth::create(c, &firefox_reddit(ALL), 10).unwrap();

    // AC-54: success without evidence does not touch the profile.
    let public = classify::success_state_change(false);
    assert!(!auth::apply_state_change(c, &p.id, &public, 20).unwrap());
    let same = auth::get(c, &p.id).unwrap();
    assert_eq!(same.session_state, ProfileState::NeedsVerification);
    assert_eq!(same.last_verified_at, None);

    // Evidence: session available, verification time written.
    let proof = classify::success_state_change(true);
    assert!(auth::apply_state_change(c, &p.id, &proof, 30).unwrap());
    let ok = auth::get(c, &p.id).unwrap();
    assert_eq!(ok.session_state, ProfileState::SessionAvailable);
    assert_eq!(ok.last_verified_at, Some(30));

    // A cookie read failure does not prove the session invalid.
    let cookie = classify::state_change(ErrorCode::CookieAccessFailed, Gone::Unavailable);
    auth::apply_state_change(c, &p.id, &cookie, 40).unwrap();
    let nv = auth::get(c, &p.id).unwrap();
    assert_eq!(nv.session_state, ProfileState::NeedsVerification);
    assert_eq!(nv.last_verified_at, Some(30), "earlier proof time kept");

    // ACCESS_DENIED is about the content, not the session (AC-19).
    let denied = classify::state_change(ErrorCode::AccessDenied, Gone::Unavailable);
    assert!(!auth::apply_state_change(c, &p.id, &denied, 50).unwrap());

    let expired = classify::state_change(ErrorCode::SessionExpired, Gone::Unavailable);
    auth::apply_state_change(c, &p.id, &expired, 60).unwrap();
    assert_eq!(
        auth::get(c, &p.id).unwrap().session_state,
        ProfileState::Expired
    );

    // A forged "available" without proof is refused, in code and in the schema.
    let forged = classify::StateChange {
        session_state: Some(classify::SessionState::SessionAvailable),
        session_verified: false,
        ..Default::default()
    };
    assert!(matches!(
        auth::apply_state_change(c, &p.id, &forged, 70),
        Err(AuthError::Invalid(_))
    ));
    let q = auth::create(c, &firefox_reddit(ALL), 80).unwrap();
    assert!(c
        .execute(
            "UPDATE auth_profile SET session_state = 'session_available' WHERE id = ?1",
            [&q.id]
        )
        .is_err());
}

#[test]
fn clear_removes_secret_and_consent_and_delete_forgets_the_profile() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let store = MemoryStore::new();
    let clock = FixedClock::new(1_762_500_000_000);
    let item = match save_bookmark(
        db.writer(),
        &clock,
        "https://www.reddit.com/r/videos/comments/1abc2de/x/",
    )
    .unwrap()
    .outcome
    {
        SaveOutcome::Created(it) => it.id,
        o => panic!("{o:?}"),
    };
    let c = db.writer();
    let p = auth::create(c, &firefox_reddit(ALL), 10).unwrap();
    let with = auth::attach_secret(c, &store, &p.id, &Secret::new(b"one".to_vec()), 20).unwrap();
    assert!(with.has_secret);
    // Replacing the secret deletes the old one.
    auth::attach_secret(c, &store, &p.id, &Secret::new(b"two".to_vec()), 21).unwrap();
    assert_eq!(store.len(), 1);
    assert_eq!(
        auth::read_secret(c, &store, &p.id, Operation::Download)
            .unwrap()
            .expose(),
        b"two"
    );

    let cleared = auth::clear(c, &store, &p.id, 30).unwrap();
    assert_eq!(cleared.session_state, ProfileState::NotConnected);
    assert_eq!(cleared.consent_at, None);
    assert!(cleared.consent_scope.is_empty());
    assert!(!cleared.has_secret);
    assert!(store.is_empty());
    assert!(auth::read_secret(c, &store, &p.id, Operation::Download).is_err());
    // No consent, no secret.
    assert!(auth::attach_secret(c, &store, &p.id, &Secret::new(b"x".to_vec()), 31).is_err());
    assert!(store.is_empty());

    c.execute(
        "UPDATE library_item SET access_profile_id = ?1 WHERE id = ?2",
        [&p.id, &item],
    )
    .unwrap();
    auth::delete(c, &store, &p.id).unwrap();
    assert!(matches!(auth::get(c, &p.id), Err(AuthError::NotFound)));
    let left: Option<String> = c
        .query_row(
            "SELECT access_profile_id FROM library_item WHERE id = ?1",
            [&item],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(left, None);
}

#[test]
fn ac18_canary_secret_is_in_no_database_file_export_or_message() {
    let d = tempfile::tempdir().unwrap();
    let store = MemoryStore::new();
    let mut texts = Vec::new();
    {
        let mut db = open(d.path());
        let c = db.writer();
        let p = auth::create(c, &firefox_reddit(ALL), 10).unwrap();
        let p = auth::attach_secret(
            c,
            &store,
            &p.id,
            &Secret::new(CANARY.as_bytes().to_vec()),
            20,
        )
        .unwrap();
        auth::apply_state_change(c, &p.id, &classify::success_state_change(true), 30).unwrap();
        let s = auth::read_secret(c, &store, &p.id, Operation::Verify).unwrap();
        assert_eq!(s.expose(), CANARY.as_bytes());
        texts.push(format!("{s:?}"));
        texts.push(format!("{:?}", auth::get(c, &p.id).unwrap()));
        texts.push(serde_json::to_string(&auth::list(c).unwrap()).unwrap());
        if let Err(e) = auth::read_secret(c, &store, &p.id, Operation::Metadata).map(|_| ()) {
            texts.push(e.to_string());
        }
        auth::set_consent(c, &p.id, &[Operation::Download], 40).unwrap();
        texts.push(
            auth::read_secret(c, &store, &p.id, Operation::Verify)
                .unwrap_err()
                .to_string(),
        );
        let doc = portable::export(c, ExportOptions::default(), "0", 50).unwrap();
        texts.push(serde_json::to_string(&doc).unwrap());
        // The UI view of a profile carries no reference either.
        let ui = serde_json::to_string(&auth::get(c, &p.id).unwrap()).unwrap();
        assert!(!ui.contains("vvsec:"), "{ui}");
        assert!(!ui.contains("default-release"), "{ui}");
    }
    for t in &texts {
        assert!(!t.contains("CANARY"), "canary in {t}");
    }
    let mut files = 0;
    for e in std::fs::read_dir(d.path()).unwrap() {
        let path = e.unwrap().path();
        if path.is_file() {
            files += 1;
            assert!(
                !contains(&std::fs::read(&path).unwrap(), "CANARY"),
                "canary in {path:?}"
            );
        }
    }
    assert!(files >= 1);
    // The secret is still in the store, behind its reference.
    assert_eq!(store.len(), 1);
}

#[test]
fn schema_has_no_column_for_a_secret_value() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let cols: Vec<String> = c
        .prepare("SELECT name FROM pragma_table_info('auth_profile')")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for col in &cols {
        for word in ["cookie", "token", "password", "secret_value", "header"] {
            assert!(!col.contains(word), "{col}");
        }
    }
    // A reference that is not the store's shape is refused.
    let p = auth::create(c, &firefox_reddit(ALL), 1).unwrap();
    assert!(c
        .execute(
            "UPDATE auth_profile SET secure_local_ref = 'reddit_session=abc' WHERE id = ?1",
            [&p.id]
        )
        .is_err());
}

#[test]
fn f5_5_reconnect_restores_consent_unproven_and_only_for_browser_profiles() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let store = MemoryStore::new();
    let c = db.writer();
    let p = auth::create(c, &firefox_reddit(ALL), 10).unwrap();
    auth::clear(c, &store, &p.id, 20).unwrap();
    assert!(
        auth::reconnect(c, &p.id, &[], 25).is_err(),
        "no reconnect without consent"
    );
    let back = auth::reconnect(c, &p.id, &[Operation::Download], 30).unwrap();
    assert_eq!(back.session_state, ProfileState::NeedsVerification);
    assert_eq!(back.consent_scope, vec![Operation::Download]);
    assert_eq!(back.consent_at, Some(30));
    assert_eq!(back.last_verified_at, None);
    assert_eq!(
        auth::usable_for(c, Platform::Reddit, Operation::Download)
            .unwrap()
            .len(),
        1
    );

    let file = auth::create(
        c,
        &NewProfile {
            provider: Platform::Vk,
            label: "VK (cookie file)",
            method: Method::CookieFile,
            browser_kind: None,
            profile_locator: None,
            consent_scope: ALL,
        },
        40,
    )
    .unwrap();
    auth::clear(c, &store, &file.id, 41).unwrap();
    assert!(matches!(
        auth::reconnect(c, &file.id, ALL, 42),
        Err(AuthError::Invalid(_))
    ));
}

#[test]
fn f5_4_verification_item_is_a_live_item_that_needed_a_login() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let clock = FixedClock::new(1_762_500_000_000);
    let save = |db: &mut Database, url: &str| match save_bookmark(db.writer(), &clock, url)
        .unwrap()
        .outcome
    {
        SaveOutcome::Created(it) => it.id,
        o => panic!("{o:?}"),
    };
    let public = save(
        &mut db,
        "https://www.reddit.com/r/videos/comments/1aaa111/x/",
    );
    let gated = save(
        &mut db,
        "https://www.reddit.com/r/videos/comments/1bbb222/y/",
    );
    let other = save(&mut db, "https://vk.com/video-1_2");
    let c = db.writer();
    assert_eq!(auth::verification_item(c, Platform::Reddit).unwrap(), None);
    c.execute(
        "UPDATE library_item SET access_state = 'public_ok' WHERE id = ?1",
        [&public],
    )
    .unwrap();
    c.execute(
        "UPDATE library_item SET access_state = 'login_required' WHERE id IN (?1, ?2)",
        [&gated, &other],
    )
    .unwrap();
    assert_eq!(
        auth::verification_item(c, Platform::Reddit).unwrap(),
        Some(gated.clone())
    );
    c.execute(
        "UPDATE library_item SET removed_at = 1 WHERE id = ?1",
        [&gated],
    )
    .unwrap();
    assert_eq!(auth::verification_item(c, Platform::Reddit).unwrap(), None);
}

#[test]
fn f5_3_reconciliation_deletes_secrets_no_profile_points_at() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let store = MemoryStore::new();
    let c = db.writer();
    let p = auth::create(c, &firefox_reddit(ALL), 10).unwrap();
    auth::attach_secret(c, &store, &p.id, &Secret::new(b"kept".to_vec()), 11).unwrap();
    use vv_secrets::SecretStore;
    let orphan = store.put(&Secret::new(b"orphan".to_vec())).unwrap();
    assert_eq!(auth::reconcile_secrets(c, &store).unwrap(), 1);
    assert!(store.get(&orphan).is_err());
    assert_eq!(
        auth::read_secret(c, &store, &p.id, Operation::Download)
            .unwrap()
            .expose(),
        b"kept"
    );
    assert_eq!(auth::reconcile_secrets(c, &store).unwrap(), 0);
}
