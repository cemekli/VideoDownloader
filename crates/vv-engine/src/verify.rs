//! `verify_session`: checking that a connected session really signs the
//! user in (STATE.md F5.4; SPEC §12.1, §12.2, AC-19, AC-54; ADR-0024).
//!
//! §12.1 accepts two kinds of evidence. (b), a platform check that passes
//! only while signed in, needs a request the app makes itself; every
//! request goes through yt-dlp behind the network guard, and yt-dlp has no
//! such check for any supported platform, so (b) is not available
//! (`docs/platform-support.md`). Verification is therefore evidence (a),
//! made on purpose: the newest item of the platform that needed a login is
//! read once without the profile and, if that is refused, once with it.
//!
//! - Refused anonymously, read with the profile: the session works. The
//!   profile becomes `session_available` with `last_verified_at`, the item
//!   `authorized`.
//! - Read anonymously: the item is public now and proves nothing; the
//!   profile is not touched (AC-54).
//! - Refused with the profile: the §12.1 table. A missing login or an
//!   unreadable store leaves the session `needs_verification`, an expired
//!   one `expired`; `ACCESS_DENIED` marks the item and leaves the session
//!   alone (AC-19).
//! - No item of the platform has needed a login yet: nothing to test with;
//!   the state is left as it is.

use crate::auth::{self, Attempt};
use crate::context::{Context, Store};
use crate::ytdlp::{self, Mode, RunResult};
use serde::Serialize;
use vv_domain::error::ErrorCode;
use vv_persistence::auth::{self as pauth, AuthProfile, Operation};
use vv_persistence::classify::{self, Gone};
use vv_persistence::library::repo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifyOutcome {
    /// Evidence found: the session signs the user in.
    Verified,
    /// The platform still asked for a login with the profile.
    NotSignedIn,
    /// The browser store or the stored cookies could not be read.
    CookieAccessFailed,
    /// The platform said the session expired.
    SessionExpired,
    /// Signed in, but this account may not see the test item (AC-19).
    AccessDenied,
    /// The test item opened without a login: it proves nothing.
    PublicItem,
    /// No item of this platform has needed a login yet.
    NoTestItem,
    /// The user did not allow "verify" for this session.
    NoConsent,
    /// Cleared, unsupported, or a method that cannot run here.
    NotRunnable,
    /// Something else failed (network, tools, timeout); `code` says what.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyResult {
    pub outcome: VerifyOutcome,
    /// The item used for the test, if any.
    pub item_id: Option<String>,
    /// The §19 code behind a failure.
    pub code: Option<ErrorCode>,
    pub profile: AuthProfile,
}

fn done(
    p: AuthProfile,
    outcome: VerifyOutcome,
    item: Option<&str>,
    code: Option<ErrorCode>,
) -> VerifyResult {
    VerifyResult {
        outcome,
        item_id: item.map(str::to_owned),
        code,
        profile: p,
    }
}

/// Runs the check for `profile_id`. Blocks for at most two metadata runs.
pub fn verify_session<S: Store>(
    cx: &Context<S>,
    profile_id: &str,
) -> Result<VerifyResult, pauth::AuthError> {
    let p = cx.store.read(|c| pauth::get(c, profile_id))?;
    if !p.allows(Operation::Verify) {
        return Ok(done(p, VerifyOutcome::NoConsent, None, None));
    }
    if !auth::runnable(&p)
        || matches!(
            p.session_state,
            pauth::ProfileState::NotConnected | pauth::ProfileState::Unsupported
        )
    {
        return Ok(done(p, VerifyOutcome::NotRunnable, None, None));
    }
    if cx.is_offline() {
        return Ok(done(
            p,
            VerifyOutcome::Failed,
            None,
            Some(ErrorCode::OfflineMode),
        ));
    }
    let tools = cx.tools();
    let Some(yt) = tools.ytdlp.clone() else {
        let code = tools
            .report
            .ready_for_downloads()
            .err()
            .unwrap_or(ErrorCode::ToolMissing);
        return Ok(done(p, VerifyOutcome::Failed, None, Some(code)));
    };
    let item = cx.store.read(|c| {
        pauth::verification_item(c, p.provider)
            .ok()
            .flatten()
            .and_then(|id| repo::get_item(c, &id).ok().flatten())
    });
    let Some(item) = item else {
        return Ok(done(p, VerifyOutcome::NoTestItem, None, None));
    };
    let Some(url) = item.canonical_url.clone() else {
        return Ok(done(p, VerifyOutcome::NoTestItem, None, None));
    };
    if let Err(code) = ytdlp::precheck(&cx.policy, &url) {
        return Ok(done(p, VerifyOutcome::Failed, Some(&item.id), Some(code)));
    }
    let exec = |attempt: &Attempt| -> RunResult {
        let prepared = match attempt.prepare(cx) {
            Ok(x) => x,
            Err(code) => {
                return RunResult::Failed {
                    code,
                    message: None,
                }
            }
        };
        ytdlp::run_as(
            &yt,
            cx.run_policy(),
            &url,
            &Mode::Metadata {
                thumbnail_dir: None,
            },
            &prepared.args,
            Some(ytdlp::METADATA_TIMEOUT),
            |_| {},
            || false,
        )
        .result
    };

    // (a): without the profile first.
    let code = match exec(&Attempt::anonymous()) {
        RunResult::Succeeded => {
            let now = cx.now();
            cx.store.write(|c| {
                let change = classify::success_state_change(false);
                let _ = classify::apply_to_item(c, &item.id, &change, now);
            });
            cx.events.item_changed(&item.id);
            let p = cx.store.read(|c| pauth::get(c, &p.id))?;
            return Ok(done(p, VerifyOutcome::PublicItem, Some(&item.id), None));
        }
        RunResult::Canceled => {
            return Ok(done(
                p,
                VerifyOutcome::Failed,
                Some(&item.id),
                Some(ErrorCode::Canceled),
            ))
        }
        RunResult::Failed { code, .. } if auth::refused(code) => code,
        RunResult::Failed { code, .. } => {
            return Ok(done(p, VerifyOutcome::Failed, Some(&item.id), Some(code)))
        }
    };
    let _ = code;

    // Refused anonymously: now with the profile.
    let attempt = Attempt {
        profile: Some(p.clone()),
        evidence: true,
        op: Some(Operation::Verify),
    };
    let result = exec(&attempt);
    let now = cx.now();
    let (outcome, code) = cx.store.write(|c| match &result {
        RunResult::Succeeded => {
            auth::on_success(c, &item.id, &attempt, now);
            (VerifyOutcome::Verified, None)
        }
        RunResult::Canceled => (VerifyOutcome::Failed, Some(ErrorCode::Canceled)),
        RunResult::Failed { code, .. } => {
            let code = *code;
            auth::after_failure(c, &item.id, &attempt, code, now);
            if code == ErrorCode::AccessDenied {
                let change = classify::state_change(code, Gone::Unavailable);
                let _ = classify::apply_to_item(c, &item.id, &change, now);
            }
            let outcome = match code {
                ErrorCode::LoginRequired => VerifyOutcome::NotSignedIn,
                ErrorCode::CookieAccessFailed => VerifyOutcome::CookieAccessFailed,
                ErrorCode::SessionExpired => VerifyOutcome::SessionExpired,
                ErrorCode::AccessDenied => VerifyOutcome::AccessDenied,
                _ => VerifyOutcome::Failed,
            };
            (outcome, Some(code))
        }
    });
    cx.events.item_changed(&item.id);
    let p = cx.store.read(|c| pauth::get(c, &p.id))?;
    Ok(done(p, outcome, Some(&item.id), code))
}
