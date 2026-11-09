//! Running a job as the user (STATE.md F5.2; SPEC §12.1, §20.3, §21.1;
//! ADR-0022, ADR-0023).
//!
//! A metadata or download job runs without a profile first. Only when that
//! attempt says `LOGIN_REQUIRED`, and the user connected a profile for the
//! item's platform with consent for this operation, does the same attempt
//! run again with the profile's browser cookies. That pair is the §12.1
//! evidence (a): the content needed a login and the profile got in. An item
//! that already needed a login (`login_required`, `authorized`) goes to the
//! profile at once; its state came from such an anonymous refusal.
//!
//! Nothing secret passes through here: the browser method keeps only the
//! browser and the profile folder, and yt-dlp reads the cookies itself.

use crate::context::{Context, Store};
use std::ffi::OsString;
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
}

impl Attempt {
    pub fn anonymous() -> Self {
        Attempt::default()
    }

    /// `--cookies-from-browser` value for this attempt, if any.
    pub fn cookies(&self) -> Option<OsString> {
        self.profile.as_ref().and_then(cookies_source)
    }
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

fn usable(c: &Connection, item: &LibraryItem, op: Operation) -> Option<AuthProfile> {
    let provider = auth::provider(item.source_platform.as_str())?;
    auth::usable_for(c, provider, op)
        .ok()?
        .into_iter()
        .find(|p| cookies_source(p).is_some())
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
        Some(p) => Attempt {
            profile: Some(p),
            evidence: true,
        },
        None => Attempt::anonymous(),
    }
}

/// After an anonymous `LOGIN_REQUIRED`: the item needs a login (written
/// now, whatever happens next) and, if the user connected a profile, the
/// attempt to make with it.
pub fn after_login_required<S: Store>(
    cx: &Context<S>,
    item: &LibraryItem,
    op: Operation,
) -> Option<Attempt> {
    let now = cx.now();
    cx.store.write(|c| {
        let _ = c.execute(
            "UPDATE library_item SET access_state = 'login_required', updated_at = ?2 WHERE id = ?1",
            vv_persistence::rusqlite::params![item.id, now],
        );
        usable(c, item, op).map(|p| Attempt {
            profile: Some(p),
            evidence: true,
        })
    })
}

/// A failed attempt made with a profile: what it says about the session.
/// `COOKIE_ACCESS_FAILED` and a refused login both leave the session
/// unproven (`needs_verification`); only `SESSION_EXPIRED` says `expired`.
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
