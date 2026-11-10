//! Running a job as the user (STATE.md F5.2, F5.3; SPEC §12.1, §20.3,
//! §21.1; ADR-0022, ADR-0023, ADR-0024).
//!
//! A metadata or download job runs without a profile first. Only when that
//! attempt is refused (`LOGIN_REQUIRED` or `ACCESS_DENIED`), and the user
//! connected a profile for the item's platform with consent for this
//! operation, does the same attempt run again with the profile. That pair
//! is the §12.1 evidence (a): the content was refused anonymously and the
//! profile got in. An item that already needed a login (`login_required`,
//! `authorized`) goes to the profile at once; its state came from such an
//! anonymous refusal.
//!
//! Two methods:
//! - Browser cookies (F5.2): yt-dlp reads the browser's store itself; the
//!   profile keeps only the browser and the profile folder.
//! - Cookie file (F5.3): the platform's cookies, filtered when the user
//!   connected the file, sit in the OS secret store. For each run they are
//!   written to a new working copy in a folder only the current user can
//!   read; yt-dlp gets that copy (`--cookies` writes back to its file, so
//!   it never gets the user's own file) and the copy is deleted when the
//!   run ends, whatever the outcome. Copies a crash left behind go at the
//!   next start ([`clean_working_copies`]). The copy's path is never in the
//!   job row, its payload or a log line.

use crate::context::{Context, Dirs, Store};
use std::ffi::OsString;
use std::path::PathBuf;
use vv_domain::error::ErrorCode;
use vv_persistence::auth::{self, AuthProfile, Method, Operation};
use vv_persistence::classify::{SessionState, StateChange};
use vv_persistence::library::repo::LibraryItem;
use vv_persistence::library::AccessState;
use vv_persistence::rusqlite::Connection;

/// How one run is made.
#[derive(Debug, Clone, Default)]
pub struct Attempt {
    pub profile: Option<AuthProfile>,
    /// A success with this profile proves authenticated access (§12.1).
    pub evidence: bool,
    /// The operation the profile is used for (consent is checked again
    /// when a secret is read).
    pub op: Option<Operation>,
}

impl Attempt {
    pub fn anonymous() -> Self {
        Attempt::default()
    }

    fn with(p: AuthProfile, op: Operation) -> Self {
        Attempt {
            profile: Some(p),
            evidence: true,
            op: Some(op),
        }
    }

    /// The extra yt-dlp arguments for this attempt and, for a cookie file,
    /// the working copy they name (keep it alive for the whole run; it is
    /// deleted when dropped). `COOKIE_ACCESS_FAILED` when the stored
    /// cookies cannot be read or the copy cannot be written.
    pub fn prepare<S: Store>(&self, cx: &Context<S>) -> Result<Prepared, ErrorCode> {
        let Some(p) = &self.profile else {
            return Ok(Prepared::default());
        };
        match p.method {
            Method::BrowserCookies => {
                let src = cookies_source(p).ok_or(ErrorCode::CookieAccessFailed)?;
                Ok(Prepared {
                    args: vec!["--cookies-from-browser".into(), src],
                    copy: None,
                })
            }
            // Both keep the platform's cookies in the secret store: a
            // cookie file the user exported, or the in-app sign-in window's
            // cookies (F5.7, ADR-0025).
            Method::CookieFile | Method::AppProfile => {
                let op = self.op.ok_or(ErrorCode::CookieAccessFailed)?;
                let secret = cx
                    .store
                    .read(|c| auth::read_secret(c, cx.secrets.as_ref(), &p.id, op))
                    .map_err(|_| ErrorCode::CookieAccessFailed)?;
                let copy = WorkingCopy::create(&cx.dirs, secret.expose())
                    .map_err(|_| ErrorCode::CookieAccessFailed)?;
                Ok(Prepared {
                    args: vec!["--cookies".into(), copy.path.clone().into()],
                    copy: Some(copy),
                })
            }
        }
    }
}

/// Arguments for one run; holds the working copy until the run ends.
#[derive(Default)]
pub struct Prepared {
    pub args: Vec<OsString>,
    pub copy: Option<WorkingCopy>,
}

impl std::fmt::Debug for Prepared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The arguments name a private folder: never printed.
        write!(
            f,
            "Prepared {{ args: {}, copy: {} }}",
            self.args.len(),
            self.copy.is_some()
        )
    }
}

/// One run's copy of the platform's cookies. Deleted on drop.
pub struct WorkingCopy {
    path: PathBuf,
}

impl WorkingCopy {
    fn create(dirs: &Dirs, bytes: &[u8]) -> std::io::Result<WorkingCopy> {
        let dir = dirs.cookie_copies();
        vv_secrets::private_fs::create_dir(&dir)?;
        let path = dir.join(format!("{}.txt", vv_persistence::library::new_id()));
        vv_secrets::private_fs::write_new(&path, bytes)?;
        Ok(WorkingCopy { path })
    }
}

impl Drop for WorkingCopy {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Start-up reconciliation: working copies a crash left behind are deleted
/// (§21.1). Returns how many.
pub fn clean_working_copies(dirs: &Dirs) -> usize {
    let Ok(rd) = std::fs::read_dir(dirs.cookie_copies()) else {
        return 0;
    };
    let mut n = 0;
    for e in rd.flatten() {
        if std::fs::remove_file(e.path()).is_ok() {
            n += 1;
        }
    }
    n
}

/// `<browser>:<profile folder>`, or `None` when the profile is not a usable
/// browser profile.
pub fn cookies_source(p: &AuthProfile) -> Option<OsString> {
    if p.method != Method::BrowserCookies {
        return None;
    }
    let browser = p.browser_kind?;
    let dir = p.profile_locator.as_deref()?;
    let mut v = OsString::from(browser.as_str());
    v.push(":");
    v.push(dir);
    Some(v)
}

/// A profile a run can use: a browser profile with its folder, or a cookie
/// file or in-app sign-in whose cookies are stored.
pub fn runnable(p: &AuthProfile) -> bool {
    match p.method {
        Method::BrowserCookies => cookies_source(p).is_some(),
        Method::CookieFile | Method::AppProfile => p.has_secret,
    }
}

fn usable(c: &Connection, item: &LibraryItem, op: Operation) -> Option<AuthProfile> {
    let provider = auth::provider(item.source_platform.as_str())?;
    auth::usable_for(c, provider, op)
        .ok()?
        .into_iter()
        .find(runnable)
}

/// The first attempt: with the profile only when the item is known to need
/// a login.
pub fn first<S: Store>(cx: &Context<S>, item: &LibraryItem, op: Operation) -> Attempt {
    if !matches!(
        item.access_state,
        AccessState::LoginRequired | AccessState::Authorized
    ) {
        return Attempt::anonymous();
    }
    match cx.store.read(|c| usable(c, item, op)) {
        Some(p) => Attempt::with(p, op),
        None => Attempt::anonymous(),
    }
}

/// Whether an anonymous failure is a refusal a profile may get past.
pub fn refused(code: ErrorCode) -> bool {
    matches!(code, ErrorCode::LoginRequired | ErrorCode::AccessDenied)
}

/// After an anonymous refusal (`LOGIN_REQUIRED` or `ACCESS_DENIED`): the
/// item's state is written now, whatever happens next, and, if the user
/// connected a profile, the attempt to make with it.
pub fn after_refusal<S: Store>(
    cx: &Context<S>,
    item: &LibraryItem,
    op: Operation,
    code: ErrorCode,
) -> Option<Attempt> {
    let now = cx.now();
    let state = if code == ErrorCode::AccessDenied {
        "access_denied"
    } else {
        "login_required"
    };
    cx.store.write(|c| {
        let _ = c.execute(
            "UPDATE library_item SET access_state = ?2, updated_at = ?3 WHERE id = ?1",
            vv_persistence::rusqlite::params![item.id, state, now],
        );
        usable(c, item, op).map(|p| Attempt::with(p, op))
    })
}

/// A failed attempt made with a profile: what it says about the session.
/// `COOKIE_ACCESS_FAILED` and a refused login both leave the session
/// unproven (`needs_verification`); only `SESSION_EXPIRED` says `expired`.
/// `ACCESS_DENIED` says nothing about the session (AC-19).
pub fn after_failure(c: &Connection, item_id: &str, attempt: &Attempt, code: ErrorCode, now: i64) {
    if matches!(code, ErrorCode::LoginRequired | ErrorCode::SessionExpired) {
        let _ = c.execute(
            "UPDATE library_item SET access_state = 'login_required', updated_at = ?2 WHERE id = ?1",
            vv_persistence::rusqlite::params![item_id, now],
        );
    }
    let Some(p) = &attempt.profile else {
        return;
    };
    let state = match code {
        ErrorCode::CookieAccessFailed | ErrorCode::LoginRequired => SessionState::NeedsVerification,
        ErrorCode::SessionExpired => SessionState::Expired,
        _ => return,
    };
    let change = StateChange {
        session_state: Some(state),
        ..StateChange::default()
    };
    let _ = auth::apply_state_change(c, &p.id, &change, now);
}

/// A successful attempt, inside the job's success transaction. Returns
/// whether the item was marked `authorized` (the caller then does not
/// write `public_ok`).
pub fn on_success(c: &Connection, item_id: &str, attempt: &Attempt, now: i64) -> bool {
    let Some(p) = &attempt.profile else {
        return false;
    };
    if !attempt.evidence {
        return false;
    }
    let change = vv_persistence::classify::success_state_change(true);
    let _ = auth::apply_state_change(c, &p.id, &change, now);
    auth::mark_item_authorized(c, item_id, &p.id, now).is_ok()
}
