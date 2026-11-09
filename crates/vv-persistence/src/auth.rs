//! Authorized session profiles (SPEC §11.6 `AuthProfile`, §12.1, §12.2,
//! §20.3; STATE.md F5.1; ADR-0022).
//!
//! A profile says *how* the app may reach one platform as the user (read a
//! browser's cookies, use a filtered cookie file, use an app-owned browser
//! profile) and *what for* (the consent scope). It never holds the secret:
//! [`attach_secret`] puts the value in the OS-protected store and keeps only
//! the opaque reference.
//!
//! Rules kept here:
//! - No profile without consent; consent names the operations.
//! - A profile is used only for its own platform and consented operations
//!   ([`usable_for`]).
//! - `session_available` is written only with authenticated-access evidence
//!   ([`apply_state_change`] with `session_verified`); the database refuses
//!   it without a verification time.
//! - [`clear`] removes the app's secret and the consent; it does not touch
//!   the user's browser or account (§12.2).

use crate::classify::{self, StateChange};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use vv_domain::normalize::Platform;
use vv_secrets::{Secret, SecretError, SecretRef, SecretStore};

#[derive(Debug)]
pub enum AuthError {
    Sqlite(rusqlite::Error),
    Secret(SecretError),
    NotFound,
    /// Input the rules refuse (no consent, generic platform, wrong method
    /// fields…). The message names the rule, never a value.
    Invalid(&'static str),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::Sqlite(e) => write!(f, "sqlite: {e}"),
            AuthError::Secret(e) => write!(f, "{e}"),
            AuthError::NotFound => write!(f, "profile not found"),
            AuthError::Invalid(m) => write!(f, "invalid profile: {m}"),
        }
    }
}

impl std::error::Error for AuthError {}

impl From<rusqlite::Error> for AuthError {
    fn from(e: rusqlite::Error) -> Self {
        AuthError::Sqlite(e)
    }
}

impl From<SecretError> for AuthError {
    fn from(e: SecretError) -> Self {
        AuthError::Secret(e)
    }
}

pub type Result<T> = std::result::Result<T, AuthError>;

macro_rules! wire_enum {
    ($name:ident { $($v:ident = $s:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
        #[serde(rename_all = "snake_case")]
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
    };
}

wire_enum!(Method {
    BrowserCookies = "browser_cookies",
    CookieFile = "cookie_file",
    AppProfile = "app_profile",
});

wire_enum!(BrowserKind {
    Firefox = "firefox",
    Chrome = "chrome",
    Edge = "edge",
    Brave = "brave",
    Chromium = "chromium",
    Opera = "opera",
    Vivaldi = "vivaldi",
});

wire_enum!(Operation {
    Metadata = "metadata",
    Download = "download",
    Verify = "verify",
});

// §12.2, all five persisted states.
wire_enum!(ProfileState {
    NotConnected = "not_connected",
    NeedsVerification = "needs_verification",
    SessionAvailable = "session_available",
    Expired = "expired",
    Unsupported = "unsupported",
});

impl From<classify::SessionState> for ProfileState {
    fn from(s: classify::SessionState) -> Self {
        match s {
            classify::SessionState::NeedsVerification => ProfileState::NeedsVerification,
            classify::SessionState::SessionAvailable => ProfileState::SessionAvailable,
            classify::SessionState::Expired => ProfileState::Expired,
        }
    }
}

/// A stored profile. Holds the secret's reference, never the secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthProfile {
    pub id: String,
    pub provider: Platform,
    pub label: String,
    pub method: Method,
    pub browser_kind: Option<BrowserKind>,
    /// Browser profile name or folder. A private path: do not log it.
    #[serde(skip)]
    pub profile_locator: Option<String>,
    /// Opaque; not sent to the UI.
    #[serde(skip)]
    pub secure_local_ref: Option<String>,
    pub has_secret: bool,
    pub session_state: ProfileState,
    pub consent_scope: Vec<Operation>,
    pub consent_at: Option<i64>,
    pub last_verified_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl AuthProfile {
    pub fn allows(&self, op: Operation) -> bool {
        self.consent_at.is_some() && self.consent_scope.contains(&op)
    }
}

/// What the user chose in the consent step.
#[derive(Debug, Clone)]
pub struct NewProfile<'a> {
    pub provider: Platform,
    pub label: &'a str,
    pub method: Method,
    pub browser_kind: Option<BrowserKind>,
    pub profile_locator: Option<&'a str>,
    /// Must not be empty: there is no profile without consent.
    pub consent_scope: &'a [Operation],
}

const COLUMNS: &str =
    "id, provider, label, method, browser_kind, profile_locator, secure_local_ref, \
    session_state, consent_scope, consent_at, last_verified_at, created_at, updated_at";

/// The platform a profile can be for (`generic` is never one).
pub fn provider(s: &str) -> Option<Platform> {
    platform(s)
}

fn platform(s: &str) -> Option<Platform> {
    use Platform::*;
    [
        Youtube,
        Facebook,
        Instagram,
        Tiktok,
        X,
        Vk,
        Dailymotion,
        Twitch,
        Reddit,
    ]
    .into_iter()
    .find(|p| p.as_str() == s)
}

fn bad(col: usize) -> rusqlite::Error {
    rusqlite::Error::InvalidColumnType(col, "auth_profile".into(), rusqlite::types::Type::Text)
}

fn from_row(r: &Row) -> rusqlite::Result<AuthProfile> {
    let provider: String = r.get(1)?;
    let method: String = r.get(3)?;
    let browser: Option<String> = r.get(4)?;
    let state: String = r.get(7)?;
    let scope: String = r.get(8)?;
    let secure_local_ref: Option<String> = r.get(6)?;
    let ops: Vec<String> = serde_json::from_str(&scope).map_err(|_| bad(8))?;
    Ok(AuthProfile {
        id: r.get(0)?,
        provider: platform(&provider).ok_or_else(|| bad(1))?,
        label: r.get(2)?,
        method: Method::parse(&method).ok_or_else(|| bad(3))?,
        browser_kind: match browser {
            Some(b) => Some(BrowserKind::parse(&b).ok_or_else(|| bad(4))?),
            None => None,
        },
        profile_locator: r.get(5)?,
        has_secret: secure_local_ref.is_some(),
        secure_local_ref,
        session_state: ProfileState::parse(&state).ok_or_else(|| bad(7))?,
        consent_scope: ops.iter().filter_map(|o| Operation::parse(o)).collect(),
        consent_at: r.get(9)?,
        last_verified_at: r.get(10)?,
        created_at: r.get(11)?,
        updated_at: r.get(12)?,
    })
}

fn scope_json(ops: &[Operation]) -> String {
    let mut v: Vec<&str> = Vec::new();
    for o in Operation::ALL {
        if ops.contains(o) {
            v.push(o.as_str());
        }
    }
    serde_json::to_string(&v).expect("string list")
}

/// Creates a profile with the user's consent. It starts as
/// `needs_verification`: a stored reference alone is not a working session
/// (§12.2).
pub fn create(c: &Connection, p: &NewProfile, now: i64) -> Result<AuthProfile> {
    if p.provider == Platform::Generic {
        return Err(AuthError::Invalid("profile must name one known platform"));
    }
    if p.consent_scope.is_empty() {
        return Err(AuthError::Invalid("consent is required"));
    }
    let label = p.label.trim();
    if label.is_empty() || label.chars().count() > 200 {
        return Err(AuthError::Invalid("label must be 1-200 characters"));
    }
    if (p.method == Method::BrowserCookies) != p.browser_kind.is_some() {
        return Err(AuthError::Invalid(
            "browser is required exactly for browser_cookies",
        ));
    }
    if let Some(l) = p.profile_locator {
        if l.is_empty() || l.len() > 1024 || l.contains('\0') {
            return Err(AuthError::Invalid("browser profile must be 1-1024 bytes"));
        }
    }
    let id = crate::library::new_id();
    c.execute(
        "INSERT INTO auth_profile (id, provider, label, method, browser_kind, profile_locator, session_state, consent_scope, consent_at, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'needs_verification', ?7, ?8, ?8, ?8)",
        params![
            id,
            p.provider.as_str(),
            label,
            p.method.as_str(),
            p.browser_kind.map(|b| b.as_str()),
            p.profile_locator,
            scope_json(p.consent_scope),
            now
        ],
    )?;
    get(c, &id)
}

pub fn get(c: &Connection, id: &str) -> Result<AuthProfile> {
    c.query_row(
        &format!("SELECT {COLUMNS} FROM auth_profile WHERE id = ?1"),
        [id],
        from_row,
    )
    .optional()?
    .ok_or(AuthError::NotFound)
}

pub fn list(c: &Connection) -> Result<Vec<AuthProfile>> {
    let mut st = c.prepare(&format!(
        "SELECT {COLUMNS} FROM auth_profile ORDER BY provider, created_at, id"
    ))?;
    let rows = st.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Profiles that may be used for `op` on `provider`: consent covers the
/// operation and the profile is not cleared or unsupported. Verified
/// sessions come first, then the newest.
pub fn usable_for(c: &Connection, provider: Platform, op: Operation) -> Result<Vec<AuthProfile>> {
    let mut st = c.prepare(&format!(
        "SELECT {COLUMNS} FROM auth_profile WHERE provider = ?1 AND consent_at IS NOT NULL \
           AND session_state NOT IN ('not_connected', 'unsupported') \
           AND EXISTS (SELECT 1 FROM json_each(consent_scope) WHERE value = ?2) \
         ORDER BY session_state = 'session_available' DESC, created_at DESC, id"
    ))?;
    let rows = st.query_map(params![provider.as_str(), op.as_str()], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Changes what the user allows. An empty scope withdraws consent; the
/// profile then cannot be used until consent is given again.
pub fn set_consent(c: &Connection, id: &str, ops: &[Operation], now: i64) -> Result<AuthProfile> {
    let n = c.execute(
        "UPDATE auth_profile SET consent_scope = ?2, consent_at = ?3, updated_at = ?4 WHERE id = ?1",
        params![
            id,
            scope_json(ops),
            (!ops.is_empty()).then_some(now),
            now
        ],
    )?;
    if n == 0 {
        return Err(AuthError::NotFound);
    }
    get(c, id)
}

/// Stores `secret` in the OS store and keeps its reference on the profile,
/// replacing (and deleting) an earlier one. If the database write fails the
/// new secret is removed again.
pub fn attach_secret(
    c: &Connection,
    store: &dyn SecretStore,
    id: &str,
    secret: &Secret,
    now: i64,
) -> Result<AuthProfile> {
    let old = get(c, id)?;
    if old.consent_at.is_none() {
        return Err(AuthError::Invalid("consent is required"));
    }
    let r = store.put(secret)?;
    let res = c.execute(
        "UPDATE auth_profile SET secure_local_ref = ?2, session_state = 'needs_verification', last_verified_at = NULL, updated_at = ?3 WHERE id = ?1",
        params![id, r.as_str(), now],
    );
    if let Err(e) = res {
        let _ = store.delete(&r);
        return Err(e.into());
    }
    if let Some(prev) = old.secure_local_ref.as_deref().and_then(SecretRef::parse) {
        store.delete(&prev)?;
    }
    get(c, id)
}

/// Reads the profile's secret for one job. Refuses when consent does not
/// cover `op`.
pub fn read_secret(
    c: &Connection,
    store: &dyn SecretStore,
    id: &str,
    op: Operation,
) -> Result<Secret> {
    let p = get(c, id)?;
    if !p.allows(op) {
        return Err(AuthError::Invalid("consent does not cover this operation"));
    }
    let r = p
        .secure_local_ref
        .as_deref()
        .and_then(SecretRef::parse)
        .ok_or(AuthError::Secret(SecretError::NotFound))?;
    Ok(store.get(&r)?)
}

/// §12.2 "Clear": deletes the app's secret, withdraws consent and marks the
/// profile `not_connected`. The user's browser cookies and the account
/// itself are not touched.
pub fn clear(c: &Connection, store: &dyn SecretStore, id: &str, now: i64) -> Result<AuthProfile> {
    let p = get(c, id)?;
    c.execute(
        "UPDATE auth_profile SET secure_local_ref = NULL, consent_scope = '[]', consent_at = NULL, \
           session_state = 'not_connected', last_verified_at = NULL, updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    if let Some(r) = p.secure_local_ref.as_deref().and_then(SecretRef::parse) {
        store.delete(&r)?;
    }
    get(c, id)
}

/// Removes the profile and its secret. Items that named it as their last
/// authorized profile forget it (trigger in migration 0012).
pub fn delete(c: &Connection, store: &dyn SecretStore, id: &str) -> Result<()> {
    let p = get(c, id)?;
    c.execute("DELETE FROM auth_profile WHERE id = ?1", [id])?;
    if let Some(r) = p.secure_local_ref.as_deref().and_then(SecretRef::parse) {
        store.delete(&r)?;
    }
    Ok(())
}

/// Marks a profile `unsupported` (the method does not work in this
/// environment, e.g. a browser store the tool cannot read).
pub fn mark_unsupported(c: &Connection, id: &str, now: i64) -> Result<AuthProfile> {
    let n = c.execute(
        "UPDATE auth_profile SET session_state = 'unsupported', last_verified_at = NULL, updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    if n == 0 {
        return Err(AuthError::NotFound);
    }
    get(c, id)
}

/// Applies the profile half of a §12.1 state change after an attempt made
/// with this profile. `session_available` and `last_verified_at` are written
/// only when the change carries evidence (`session_verified`); a cleared
/// profile is left alone. Returns whether anything changed.
pub fn apply_state_change(
    c: &Connection,
    id: &str,
    change: &StateChange,
    now: i64,
) -> Result<bool> {
    let Some(state) = change.session_state else {
        return Ok(false);
    };
    let state = ProfileState::from(state);
    if state == ProfileState::SessionAvailable && !change.session_verified {
        return Err(AuthError::Invalid("session_available needs evidence"));
    }
    let n = c.execute(
        "UPDATE auth_profile SET session_state = ?2, \
           last_verified_at = CASE WHEN ?3 THEN ?4 ELSE last_verified_at END, updated_at = ?4 \
         WHERE id = ?1 AND consent_at IS NOT NULL AND session_state <> 'not_connected'",
        params![id, state.as_str(), change.session_verified, now],
    )?;
    Ok(n > 0)
}

/// Jobs waiting for a login (`blocked`/`auth`) on `provider`'s items go
/// back to the queue, e.g. after the user connected a profile. Returns how
/// many.
pub fn unblock_provider(c: &Connection, provider: Platform, now: i64) -> Result<usize> {
    Ok(c.execute(
        "UPDATE job SET status = 'queued', blocked_reason = NULL, updated_at = ?2 \
         WHERE status = 'blocked' AND blocked_reason = 'auth' \
           AND item_id IN (SELECT id FROM library_item WHERE source_platform = ?1)",
        params![provider.as_str(), now],
    )?)
}

/// Records on the item that `profile_id` reached it with proof of
/// authenticated access (§12.1 `authorized`, `access_profile_id`,
/// `access_checked_at`).
pub fn mark_item_authorized(
    c: &Connection,
    item_id: &str,
    profile_id: &str,
    now: i64,
) -> Result<()> {
    c.execute(
        "UPDATE library_item SET access_state = 'authorized', remote_availability = 'available', \
           access_profile_id = ?2, access_checked_at = ?3, updated_at = ?3 WHERE id = ?1",
        params![item_id, profile_id, now],
    )?;
    Ok(())
}

/// §12.2 "Reconnect" of a cleared browser profile: consent is given again
/// and the session starts unproven (`needs_verification`). A cookie-file
/// profile cannot be reconnected this way: its cookies were deleted with
/// Clear, so the user picks a file again.
pub fn reconnect(c: &Connection, id: &str, ops: &[Operation], now: i64) -> Result<AuthProfile> {
    let p = get(c, id)?;
    if ops.is_empty() {
        return Err(AuthError::Invalid("consent is required"));
    }
    if p.method != Method::BrowserCookies {
        return Err(AuthError::Invalid(
            "only a browser profile reconnects in place",
        ));
    }
    c.execute(
        "UPDATE auth_profile SET consent_scope = ?2, consent_at = ?3, session_state = 'needs_verification', \
           last_verified_at = NULL, updated_at = ?3 WHERE id = ?1",
        params![id, scope_json(ops), now],
    )?;
    get(c, id)
}

/// An item `verify_session` can test the profile with (§12.1 evidence (a)):
/// the newest live item of `provider` that needed a login. `None` when
/// there is none yet.
pub fn verification_item(c: &Connection, provider: Platform) -> Result<Option<String>> {
    Ok(c
        .query_row(
            "SELECT id FROM library_item WHERE source_platform = ?1 AND removed_at IS NULL \
               AND canonical_url IS NOT NULL AND access_state IN ('login_required', 'authorized', 'access_denied') \
             ORDER BY access_state = 'authorized' DESC, coalesce(access_checked_at, 0) DESC, added_at DESC, id LIMIT 1",
            [provider.as_str()],
            |r| r.get(0),
        )
        .optional()?)
}

/// Start-up reconciliation (ADR-0022 "orphan secret"): deletes stored
/// secrets no profile points at (a delete whose store step failed, or a
/// crash between the two writes). Returns how many.
pub fn reconcile_secrets(c: &Connection, store: &dyn SecretStore) -> Result<usize> {
    let known: std::collections::HashSet<String> = {
        let mut st = c.prepare(
            "SELECT secure_local_ref FROM auth_profile WHERE secure_local_ref IS NOT NULL",
        )?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut n = 0;
    for r in store.refs()? {
        if !known.contains(r.as_str()) {
            store.delete(&r)?;
            n += 1;
        }
    }
    Ok(n)
}
