//! Error classification and state mapping (SPEC §12.1, §19; STATE.md F3.7,
//! ADR-0020).
//!
//! One place answers, for every §19 error code, what the job does next
//! ([`job_handling`]) and which item and profile states change
//! ([`state_change`]). Workers turn a tool outcome into an [`ErrorCode`]
//! (e.g. `vv_ytdlp::outcome::classify`) and then call [`apply_to_job`]; they
//! never pick a job transition themselves.

use crate::jobs::{self, CancelReason, Failure, JobStatus};
use crate::library::{AccessState, RemoteAvailability};
use rusqlite::Connection;
use vv_domain::error::ErrorCode;

/// Why a job waits for the user or the environment (`job.blocked_reason`).
/// `OFFLINE` is the same string the queue uses for Offline Mode.
pub mod blocked {
    pub const OFFLINE: &str = crate::jobs::OFFLINE;
    pub const DISK: &str = "disk";
    pub const AUTH: &str = "auth";
    pub const TOOL: &str = "tool";
    pub const STORAGE_ROOT: &str = "storage_root";
    pub const PATH: &str = "path";
    pub const MODEL: &str = "model";
}

/// What a job does after an error (SPEC §19 "İş davranışı", §13.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobHandling {
    /// Transient: back off and try again while attempts remain.
    Retry,
    /// The user or the environment must act; the attempt is not counted.
    Block(&'static str),
    /// Permanent: never retried automatically; the user may retry.
    Fail,
    /// Not an error to show: the job ends `canceled` (`CANCELED`) or is
    /// cancelled by its parent (`DEPENDENCY_FAILED`).
    Cancel(CancelReason),
}

/// SPEC §19 per code. Exhaustive on purpose: a new code does not compile
/// until it has a rule.
pub fn job_handling(code: ErrorCode) -> JobHandling {
    use ErrorCode::*;
    use JobHandling::*;
    match code {
        NetworkUnavailable | RateLimited | Timeout | MediaUrlExpired => Retry,
        OfflineMode => Block(blocked::OFFLINE),
        DiskFull | QuotaExceeded => Block(blocked::DISK),
        LoginRequired | SessionExpired | CookieAccessFailed => Block(blocked::AUTH),
        ToolMissing | ToolBlocked => Block(blocked::TOOL),
        StorageRootUnavailable => Block(blocked::STORAGE_ROOT),
        PathNotWritable => Block(blocked::PATH),
        ModelMissing | ResourceLimit => Block(blocked::MODEL),
        Canceled => Cancel(CancelReason::User),
        DependencyFailed => Cancel(CancelReason::DependencyFailed),
        InvalidUrl
        | UrlBlocked
        | UrlHasCredentials
        | InternalError
        | AccessDenied
        | ContentUnavailable
        | RegionRestricted
        | DrmUnsupported
        | ExtractorUnsupported
        | ExtractorFailed
        | PlaylistUnsupported
        | ChecksumMismatch
        | SignatureInvalid
        | InputRejected
        | MediaCorrupt
        | DuplicateItem
        | RevisionConflict
        | FileMissing
        | CodecUnsupported
        | ImportSchemaUnsupported
        | MigrationFailed => Fail,
    }
}

/// §12.2 profile layer. Only the values an error or success can set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    NeedsVerification,
    SessionAvailable,
    Expired,
}

impl SessionState {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionState::NeedsVerification => "needs_verification",
            SessionState::SessionAvailable => "session_available",
            SessionState::Expired => "expired",
        }
    }
}

/// Fields to change after an attempt; `None` leaves a field as it is
/// ("–" in the §12.1 table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StateChange {
    pub access_state: Option<AccessState>,
    pub remote_availability: Option<RemoteAvailability>,
    pub session_state: Option<SessionState>,
    /// Update the profile's `last_verified_at` (only with proof of an
    /// authenticated access).
    pub session_verified: bool,
}

/// What a `CONTENT_UNAVAILABLE` error showed about the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gone {
    /// The source says the content was removed (e.g. HTTP 410, "has been
    /// removed").
    Deleted,
    /// Not reachable now, with no proof that it was removed.
    Unavailable,
}

/// §12.1 "Hata kodu → durum eşlemesi" for a failed attempt. Codes that are
/// not about the content (disk, tools, network…) change nothing.
pub fn state_change(code: ErrorCode, gone: Gone) -> StateChange {
    use ErrorCode::*;
    let access = |a| StateChange {
        access_state: Some(a),
        ..StateChange::default()
    };
    match code {
        LoginRequired => access(AccessState::LoginRequired),
        SessionExpired => StateChange {
            access_state: Some(AccessState::LoginRequired),
            session_state: Some(SessionState::Expired),
            ..StateChange::default()
        },
        // The session was not proven invalid (§12.1).
        CookieAccessFailed => StateChange {
            session_state: Some(SessionState::NeedsVerification),
            ..StateChange::default()
        },
        // The session may still be valid.
        AccessDenied => access(AccessState::AccessDenied),
        ContentUnavailable => StateChange {
            remote_availability: Some(match gone {
                Gone::Deleted => RemoteAvailability::Deleted,
                Gone::Unavailable => RemoteAvailability::Unavailable,
            }),
            ..StateChange::default()
        },
        RegionRestricted => StateChange {
            remote_availability: Some(RemoteAvailability::RegionRestricted),
            ..StateChange::default()
        },
        DrmUnsupported => access(AccessState::DrmProtected),
        ExtractorUnsupported => access(AccessState::Unsupported),
        ExtractorFailed => access(AccessState::ExtractionFailed),
        // An expired media address is not an expired session.
        _ => StateChange::default(),
    }
}

/// §12.1 for a successful attempt. A request made with a profile is proof
/// of authenticated access only per the §12.1 evidence rule; without it the
/// content counts as public and the profile is not touched.
pub fn success_state_change(authenticated_evidence: bool) -> StateChange {
    if authenticated_evidence {
        StateChange {
            access_state: Some(AccessState::Authorized),
            remote_availability: Some(RemoteAvailability::Available),
            session_state: Some(SessionState::SessionAvailable),
            session_verified: true,
        }
    } else {
        StateChange {
            access_state: Some(AccessState::PublicOk),
            remote_availability: Some(RemoteAvailability::Available),
            ..StateChange::default()
        }
    }
}

/// Writes the item fields of a [`StateChange`]. The profile fields are
/// applied by the auth layer (M3).
pub fn apply_to_item(
    c: &Connection,
    item_id: &str,
    change: &StateChange,
    now: i64,
) -> rusqlite::Result<usize> {
    if change.access_state.is_none() && change.remote_availability.is_none() {
        return Ok(0);
    }
    c.execute(
        "UPDATE library_item SET access_state = coalesce(?2, access_state), remote_availability = coalesce(?3, remote_availability), updated_at = ?4 WHERE id = ?1",
        rusqlite::params![item_id, change.access_state.map(|a| a.as_str()), change.remote_availability.map(|r| r.as_str()), now],
    )
}

/// Ends a running attempt with `code` through the queue: retry with backoff,
/// block, fail or cancel, as [`job_handling`] says. `message` must already
/// be redacted. Returns the job's new status.
pub fn apply_to_job(
    c: &mut Connection,
    job_id: &str,
    generation: i64,
    code: ErrorCode,
    message: Option<&str>,
    retry_after_ms: Option<i64>,
    now: i64,
) -> jobs::Result<JobStatus> {
    let wire = code.as_str();
    match job_handling(code) {
        JobHandling::Retry => jobs::fail(
            c,
            job_id,
            generation,
            &Failure::Transient { retry_after_ms },
            wire,
            message,
            now,
        ),
        JobHandling::Fail => jobs::fail(
            c,
            job_id,
            generation,
            &Failure::Permanent,
            wire,
            message,
            now,
        ),
        JobHandling::Block(reason) => {
            jobs::block(c, job_id, Some(generation), reason, now)?;
            Ok(JobStatus::Blocked)
        }
        JobHandling::Cancel(reason) => {
            // The worker stopped on its own (e.g. the tool saw an interrupt).
            if jobs::get(c, job_id)?.status != JobStatus::Canceling {
                jobs::cancel(c, job_id, reason, now)?;
            }
            jobs::confirm_canceled(c, job_id, generation, now)?;
            Ok(JobStatus::Canceled)
        }
    }
}
