//! STATE.md F3.7: error classification and state mapping, table driven. Covers
//! DL-08 (disk, session, region, deleted content and extractor errors each get
//! their own job behaviour, state and action)
//! (SPEC §12.1, §19; ADR-0020).

use vv_domain::error::{ErrorCode, ALL_ERROR_CODES};
use vv_persistence::classify::{self, blocked, Gone, JobHandling, SessionState, StateChange};
use vv_persistence::jobs::{self, CancelReason, JobKind, JobStatus, NewJob};
use vv_persistence::library::repo::{self, LibraryItem};
use vv_persistence::library::*;
use vv_persistence::{Database, OpenOptions};

use ErrorCode::*;
use JobHandling::*;

/// SPEC §19 "İş davranışı", one row per code.
const HANDLING: &[(ErrorCode, JobHandling)] = &[
    (InvalidUrl, Fail),
    (UrlBlocked, Fail),
    (DuplicateItem, Fail),
    (NetworkUnavailable, Retry),
    (RateLimited, Retry),
    (LoginRequired, Block(blocked::AUTH)),
    (SessionExpired, Block(blocked::AUTH)),
    (CookieAccessFailed, Block(blocked::AUTH)),
    (AccessDenied, Fail),
    (MediaUrlExpired, Retry),
    (ContentUnavailable, Fail),
    (RegionRestricted, Fail),
    (DrmUnsupported, Fail),
    (ExtractorUnsupported, Fail),
    (ExtractorFailed, Fail),
    (DiskFull, Block(blocked::DISK)),
    (QuotaExceeded, Block(blocked::DISK)),
    (PathNotWritable, Block(blocked::PATH)),
    (FileMissing, Fail),
    (MediaCorrupt, Fail),
    (CodecUnsupported, Fail),
    (ModelMissing, Block(blocked::MODEL)),
    (ResourceLimit, Block(blocked::MODEL)),
    (ChecksumMismatch, Fail),
    (SignatureInvalid, Fail),
    (ImportSchemaUnsupported, Fail),
    (MigrationFailed, Fail),
    (InputRejected, Fail),
    (StorageRootUnavailable, Block(blocked::STORAGE_ROOT)),
    (UrlHasCredentials, Fail),
    (PlaylistUnsupported, Fail),
    (OfflineMode, Block(blocked::OFFLINE)),
    (Timeout, Retry),
    (Canceled, Cancel(CancelReason::User)),
    (DependencyFailed, Cancel(CancelReason::DependencyFailed)),
    (ToolMissing, Block(blocked::TOOL)),
    (ToolBlocked, Block(blocked::TOOL)),
    (RevisionConflict, Fail),
    (InternalError, Fail),
];

#[test]
fn every_spec_19_code_exists_and_has_a_job_rule() {
    // The codes are read from SPEC.md itself, so a code added there and not
    // here fails this test.
    let spec =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../SPEC.md")).unwrap();
    let start = spec.find("## 19. Hata sınıflandırması").unwrap();
    let end = start + spec[start..].find("\n## 20.").unwrap();
    let mut in_spec = std::collections::BTreeSet::new();
    for line in spec[start..end].lines().filter(|l| l.starts_with("| `")) {
        let first = line.split('|').nth(1).unwrap();
        for part in first.split('`').skip(1).step_by(2) {
            in_spec.insert(part.to_string());
        }
    }
    let ours: std::collections::BTreeSet<_> = ALL_ERROR_CODES
        .iter()
        .map(|c| c.as_str().to_string())
        .collect();
    assert_eq!(in_spec, ours, "SPEC §19 and ErrorCode differ");

    assert_eq!(HANDLING.len(), ALL_ERROR_CODES.len());
    for &(code, want) in HANDLING {
        assert_eq!(classify::job_handling(code), want, "{code:?}");
        // Only codes the job retries by itself are marked retryable to the UI.
        assert_eq!(code.retryable(), want == Retry, "{code:?} retryable");
    }
}

#[test]
fn offline_blocks_with_the_queue_reason() {
    assert_eq!(blocked::OFFLINE, jobs::OFFLINE);
}

type Row = (
    ErrorCode,
    Option<AccessState>,
    Option<RemoteAvailability>,
    Option<SessionState>,
);

/// SPEC §12.1 "Hata kodu → durum eşlemesi". Codes not listed change nothing.
const STATES: &[Row] = &[
    (LoginRequired, Some(AccessState::LoginRequired), None, None),
    (
        SessionExpired,
        Some(AccessState::LoginRequired),
        None,
        Some(SessionState::Expired),
    ),
    (
        CookieAccessFailed,
        None,
        None,
        Some(SessionState::NeedsVerification),
    ),
    (AccessDenied, Some(AccessState::AccessDenied), None, None),
    (MediaUrlExpired, None, None, None),
    (
        RegionRestricted,
        None,
        Some(RemoteAvailability::RegionRestricted),
        None,
    ),
    (DrmUnsupported, Some(AccessState::DrmProtected), None, None),
    (
        ExtractorUnsupported,
        Some(AccessState::Unsupported),
        None,
        None,
    ),
    (
        ExtractorFailed,
        Some(AccessState::ExtractionFailed),
        None,
        None,
    ),
    (NetworkUnavailable, None, None, None),
    (RateLimited, None, None, None),
    (Timeout, None, None, None),
];

#[test]
fn error_codes_map_to_item_and_profile_states() {
    for &(code, access, remote, session) in STATES {
        let got = classify::state_change(code, Gone::Unavailable);
        let want = StateChange {
            access_state: access,
            remote_availability: remote,
            session_state: session,
            session_verified: false,
        };
        assert_eq!(got, want, "{code:?}");
    }
    let listed: Vec<_> = STATES
        .iter()
        .map(|r| r.0)
        .chain([ContentUnavailable])
        .collect();
    for &code in ALL_ERROR_CODES.iter().filter(|c| !listed.contains(c)) {
        assert_eq!(
            classify::state_change(code, Gone::Deleted),
            StateChange::default(),
            "{code:?} must not touch states"
        );
    }
    // Deleted only with proof; otherwise just unavailable.
    assert_eq!(
        classify::state_change(ContentUnavailable, Gone::Deleted).remote_availability,
        Some(RemoteAvailability::Deleted)
    );
    assert_eq!(
        classify::state_change(ContentUnavailable, Gone::Unavailable).remote_availability,
        Some(RemoteAvailability::Unavailable)
    );
    assert_eq!(
        classify::state_change(ContentUnavailable, Gone::Deleted).access_state,
        None
    );
}

#[test]
fn a_session_is_verified_only_with_evidence() {
    let anon = classify::success_state_change(false);
    assert_eq!(anon.access_state, Some(AccessState::PublicOk));
    assert_eq!(
        anon.remote_availability,
        Some(RemoteAvailability::Available)
    );
    assert_eq!(anon.session_state, None);
    assert!(!anon.session_verified);
    let auth = classify::success_state_change(true);
    assert_eq!(auth.access_state, Some(AccessState::Authorized));
    assert_eq!(auth.session_state, Some(SessionState::SessionAvailable));
    assert!(auth.session_verified);
    // No error ever claims an expired session or denied access without the
    // provider saying so.
    for &code in ALL_ERROR_CODES {
        let s = classify::state_change(code, Gone::Unavailable);
        if s.session_state == Some(SessionState::Expired) {
            assert_eq!(code, SessionExpired);
        }
        if s.access_state == Some(AccessState::AccessDenied) {
            assert_eq!(code, AccessDenied);
        }
    }
}

fn open(dir: &std::path::Path) -> Database {
    Database::open(&dir.join("library.sqlite"), &OpenOptions::default()).unwrap()
}

#[test]
fn errors_drive_the_job_through_the_queue() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let cases: &[(ErrorCode, JobStatus, Option<&str>)] = &[
        (NetworkUnavailable, JobStatus::RetryWait, None),
        (RateLimited, JobStatus::RetryWait, None),
        (DiskFull, JobStatus::Blocked, Some(blocked::DISK)),
        (LoginRequired, JobStatus::Blocked, Some(blocked::AUTH)),
        (OfflineMode, JobStatus::Blocked, Some(blocked::OFFLINE)),
        (DrmUnsupported, JobStatus::Failed, None),
        (ExtractorFailed, JobStatus::Failed, None),
        (Canceled, JobStatus::Canceled, None),
    ];
    for (i, &(code, want, reason)) in cases.iter().enumerate() {
        let id = jobs::enqueue(c, &NewJob::new(JobKind::Download, format!("k{i}")), 0)
            .unwrap()
            .id;
        let l = jobs::claim(c, "w", &[JobKind::Download], 1, false, 1)
            .unwrap()
            .unwrap();
        assert_eq!(l.job.id, id);
        let st = classify::apply_to_job(
            c,
            &id,
            l.generation,
            code,
            Some("redacted"),
            Some(60_000),
            2,
        )
        .unwrap();
        assert_eq!(st, want, "{code:?}");
        let j = jobs::get(c, &id).unwrap();
        assert_eq!(j.status, want);
        assert_eq!(j.blocked_reason.as_deref(), reason, "{code:?}");
        match want {
            JobStatus::RetryWait => {
                assert_eq!(j.error_code.as_deref(), Some(code.as_str()));
                assert!(
                    j.next_retry_at.unwrap() >= 2 + 60_000,
                    "Retry-After is honoured"
                );
            }
            JobStatus::Failed => assert_eq!(j.error_code.as_deref(), Some(code.as_str())),
            JobStatus::Blocked => assert_eq!(j.attempts, 0, "blocking does not use an attempt"),
            JobStatus::Canceled => assert_eq!(j.cancel_reason, Some(CancelReason::User)),
            _ => unreachable!(),
        }
        // Move the job out of the way for the next case.
        if !want.is_terminal() {
            jobs::cancel(c, &id, CancelReason::User, 3).unwrap();
        }
    }
}

#[test]
fn a_user_cancel_already_in_flight_is_confirmed_once() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let id = jobs::enqueue(c, &NewJob::new(JobKind::Download, "k"), 0)
        .unwrap()
        .id;
    let l = jobs::claim(c, "w", &[JobKind::Download], 1, false, 1)
        .unwrap()
        .unwrap();
    jobs::cancel(c, &id, CancelReason::Shutdown, 2).unwrap();
    // The tool then exits with "interrupted by user": the original reason stays.
    assert_eq!(
        classify::apply_to_job(c, &id, l.generation, Canceled, None, None, 3).unwrap(),
        JobStatus::Canceled
    );
    assert_eq!(
        jobs::get(c, &id).unwrap().cancel_reason,
        Some(CancelReason::Shutdown)
    );
}

#[test]
fn item_states_are_written_and_untouched_fields_kept() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let mut it = LibraryItem::new(ItemKind::Bookmark, SourcePlatform::Generic, 1);
    it.canonical_url = Some("https://example.com/v/1".into());
    repo::insert_item(c, &it).unwrap();
    let read = |c: &rusqlite::Connection| -> (String, String) {
        c.query_row(
            "SELECT access_state, remote_availability FROM library_item WHERE id = ?1",
            [&it.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    };
    classify::apply_to_item(c, &it.id, &classify::success_state_change(false), 2).unwrap();
    assert_eq!(read(c), ("public_ok".into(), "available".into()));
    classify::apply_to_item(
        c,
        &it.id,
        &classify::state_change(RegionRestricted, Gone::Unavailable),
        3,
    )
    .unwrap();
    assert_eq!(read(c), ("public_ok".into(), "region_restricted".into()));
    assert_eq!(
        classify::apply_to_item(
            c,
            &it.id,
            &classify::state_change(Timeout, Gone::Unavailable),
            4
        )
        .unwrap(),
        0
    );
    classify::apply_to_item(
        c,
        &it.id,
        &classify::state_change(DrmUnsupported, Gone::Unavailable),
        5,
    )
    .unwrap();
    assert_eq!(
        read(c),
        ("drm_protected".into(), "region_restricted".into())
    );
}
