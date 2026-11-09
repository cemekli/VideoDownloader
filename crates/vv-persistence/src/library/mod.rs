//! Library tables and their repositories (SPEC §11, STATE.md F1.1–F1.3,
//! ADR-0011).
//!
//! Repositories take a `&Connection` (a `Transaction` derefs to one), so a
//! caller composes several writes into one transaction. Every write that
//! changes a user-visible field bumps `revision`; update commands carry the
//! revision they read and fail with `REVISION_CONFLICT` when it moved.

pub mod bookmark;
pub mod merge;
pub mod organize;
pub mod playback;
pub mod removal;
pub mod renormalize;
pub mod repo;
pub mod search;
pub mod undo;
pub mod view;

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use std::sync::atomic::{AtomicI64, Ordering};
use vv_domain::error::ErrorCode;

/// Source of "now" in UTC Unix milliseconds. Injected so tests control time.
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> i64;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64)
    }
}

/// A clock that only moves when told to.
pub struct FixedClock(AtomicI64);

impl FixedClock {
    pub fn new(ms: i64) -> Self {
        FixedClock(AtomicI64::new(ms))
    }
    pub fn advance(&self, ms: i64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// Random UUID v4 in its text form (SPEC §11.1).
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[derive(Debug)]
pub enum LibraryError {
    Sqlite(rusqlite::Error),
    NotFound,
    /// The row's revision is not the one the caller read.
    RevisionConflict {
        current: i64,
    },
    /// A value broke a rule the schema cannot express.
    Invalid(&'static str),
}

impl LibraryError {
    pub fn code(&self) -> ErrorCode {
        match self {
            LibraryError::RevisionConflict { .. } => ErrorCode::RevisionConflict,
            LibraryError::Sqlite(_) | LibraryError::NotFound | LibraryError::Invalid(_) => {
                ErrorCode::InternalError
            }
        }
    }

    /// True for a UNIQUE (or PRIMARY KEY) constraint failure.
    pub fn is_unique_violation(&self) -> bool {
        matches!(self, LibraryError::Sqlite(e) if is_unique_violation(e))
    }
}

pub(crate) fn is_unique_violation(e: &rusqlite::Error) -> bool {
    matches!(
        e.sqlite_error().map(|e| e.extended_code),
        Some(rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE | rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY)
    )
}

impl std::fmt::Display for LibraryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LibraryError::Sqlite(e) => write!(f, "sqlite: {e}"),
            LibraryError::NotFound => write!(f, "not found"),
            LibraryError::RevisionConflict { current } => {
                write!(f, "revision conflict (current {current})")
            }
            LibraryError::Invalid(m) => write!(f, "invalid: {m}"),
        }
    }
}

impl std::error::Error for LibraryError {}

impl From<rusqlite::Error> for LibraryError {
    fn from(e: rusqlite::Error) -> Self {
        LibraryError::Sqlite(e)
    }
}

pub type Result<T> = std::result::Result<T, LibraryError>;

/// Closed text enumerations stored as their wire name. The SQL CHECK lists in
/// `0002_library.up.sql` must hold exactly `ALL`; `tests/library_schema.rs`
/// enforces that both ways.
macro_rules! text_enum {
    ($(#[$m:meta])* $name:ident { $($v:ident = $s:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name { $($v),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$v),+];
            pub fn as_str(self) -> &'static str {
                match self { $($name::$v => $s),+ }
            }
            pub fn parse(s: &str) -> Option<Self> {
                match s { $($s => Some($name::$v),)+ _ => None }
            }
        }

        impl ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                Ok(ToSqlOutput::from(self.as_str()))
            }
        }

        impl FromSql for $name {
            fn column_result(v: ValueRef<'_>) -> FromSqlResult<Self> {
                let s = v.as_str()?;
                $name::parse(s).ok_or_else(|| FromSqlError::Other(
                    format!("unknown {} {s:?}", stringify!($name)).into(),
                ))
            }
        }
    };
}

text_enum!(ItemKind { Bookmark = "bookmark", RemoteVideo = "remote_video", LocalVideo = "local_video" });
text_enum!(SourcePlatform {
    Youtube = "youtube", Facebook = "facebook", Instagram = "instagram", Tiktok = "tiktok",
    X = "x", Vk = "vk", Dailymotion = "dailymotion", Twitch = "twitch", Generic = "generic", Local = "local",
});
text_enum!(Visibility { Unknown = "unknown", Public = "public", Unlisted = "unlisted", Private = "private" });
text_enum!(RemoteAvailability {
    Unknown = "unknown", Available = "available", Unavailable = "unavailable",
    Deleted = "deleted", RegionRestricted = "region_restricted",
});
text_enum!(
    /// §12.1 item layer only; session validity lives on the auth profile.
    AccessState {
        Unknown = "unknown", PublicOk = "public_ok", LoginRequired = "login_required",
        Authorized = "authorized", AccessDenied = "access_denied",
        ExtractionFailed = "extraction_failed", Unsupported = "unsupported",
        DrmProtected = "drm_protected",
    }
);
text_enum!(MetadataStatus { Pending = "pending", Fetching = "fetching", Ready = "ready", Partial = "partial", Failed = "failed" });
text_enum!(PublishedPrecision { Datetime = "datetime", Date = "date" });
text_enum!(StorageRootKind { ManagedMedia = "managed_media", Models = "models", Cache = "cache", ExternalImport = "external_import" });
text_enum!(StorageRootStatus { Unknown = "unknown", Available = "available", Disconnected = "disconnected", Inaccessible = "inaccessible" });
text_enum!(AssetRole {
    Original = "original", Thumbnail = "thumbnail", Preview = "preview", Waveform = "waveform",
    ExtractedAudio = "extracted_audio", SampledFrame = "sampled_frame",
});
text_enum!(MediaKind { Video = "video", Audio = "audio", Image = "image", Other = "other" });
text_enum!(StorageKind { Managed = "managed", ExternalReference = "external_reference" });
text_enum!(AcquisitionStatus { Pending = "pending", Acquiring = "acquiring", Ready = "ready", Failed = "failed", Canceled = "canceled" });
text_enum!(IntegrityState { Unknown = "unknown", Verified = "verified", Missing = "missing", Corrupt = "corrupt", Modified = "modified" });
text_enum!(DuplicateReason {
    NormalizationConflict = "normalization_conflict", SourceIdConflict = "source_id_conflict",
    ContentHash = "content_hash", SimilarUrl = "similar_url",
});
text_enum!(DuplicateStatus { Open = "open", Merged = "merged", Dismissed = "dismissed" });

impl From<vv_domain::normalize::Platform> for SourcePlatform {
    fn from(p: vv_domain::normalize::Platform) -> Self {
        // Same wire names by construction; `local` has no URL platform.
        SourcePlatform::parse(p.as_str()).expect("domain platform names are a subset")
    }
}
