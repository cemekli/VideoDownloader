//! Authorized sessions: connect a browser profile to a platform, change or
//! withdraw consent, clear and delete (STATE.md F5.2; SPEC §9.3, §12.2,
//! §20.3, §21.1; ADR-0022, ADR-0023).
//!
//! The webview sees a profile's platform, browser, browser profile *name*,
//! state and consent. It never sees the browser profile folder (a private
//! path) or a secret reference; it picks a Firefox profile by name and the
//! backend finds the folder again.

use crate::library::{internal, LibraryState};
use crate::library_commands::blocking;
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};
use vv_domain::error::{DomainError, ErrorCode};
use vv_domain::normalize::Platform;
use vv_engine::browsers::{self, BrowserProfile};
use vv_persistence::auth::{
    self, AuthError, AuthProfile, BrowserKind, Method, NewProfile, Operation,
};
use vv_persistence::library::{Clock, SystemClock};
use vv_secrets::SecretStore;

fn now() -> i64 {
    SystemClock.now_ms()
}

fn auth_err(e: AuthError) -> DomainError {
    match e {
        AuthError::NotFound => DomainError::new(ErrorCode::InputRejected, "session-not-found"),
        AuthError::Invalid(_) => DomainError::new(ErrorCode::InputRejected, "session-invalid"),
        AuthError::Secret(_) => internal("session-secret-store"),
        AuthError::Sqlite(_) => internal("session-db"),
    }
}

fn wake<R: Runtime>(app: &AppHandle<R>) {
    if let Some(e) = app.try_state::<crate::engine::EngineState>() {
        e.wake();
    }
}

fn store(st: &LibraryState) -> Result<Box<dyn SecretStore>, DomainError> {
    Ok(vv_secrets::os_store(&st.get()?.data_dir().join("secrets")))
}

/// Platforms a session can be connected for (every known one; `generic`
/// never).
pub const PROVIDERS: &[Platform] = &[
    Platform::Reddit,
    Platform::Vk,
    Platform::Youtube,
    Platform::Facebook,
    Platform::Instagram,
    Platform::Tiktok,
    Platform::X,
    Platform::Dailymotion,
    Platform::Twitch,
];

/// What the Sessions list shows of a profile.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDto {
    pub id: String,
    pub provider: Platform,
    pub label: String,
    pub method: Method,
    pub browser: Option<BrowserKind>,
    /// The browser profile's name (e.g. `default-release`), not its folder.
    pub browser_profile: Option<String>,
    /// §12.2: `not_connected`, `needs_verification`, `session_available`,
    /// `expired`, `unsupported`.
    pub state: String,
    pub consent: Vec<Operation>,
    pub consent_at: Option<i64>,
    pub last_verified_at: Option<i64>,
}

fn profile_name(p: &AuthProfile) -> Option<String> {
    // The folder's last part is `<salt>.<name>`; the name is enough to
    // recognise it and does not carry the user name.
    let dir = p.profile_locator.as_deref()?;
    let last = std::path::Path::new(dir)
        .file_name()?
        .to_string_lossy()
        .into_owned();
    Some(
        last.split_once('.')
            .map_or(last.clone(), |(_, n)| n.to_owned()),
    )
}

fn dto(p: &AuthProfile) -> SessionDto {
    SessionDto {
        id: p.id.clone(),
        provider: p.provider,
        label: p.label.clone(),
        method: p.method,
        browser: p.browser_kind,
        browser_profile: profile_name(p),
        state: p.session_state.as_str().to_owned(),
        consent: p.consent_scope.clone(),
        consent_at: p.consent_at,
        last_verified_at: p.last_verified_at,
    }
}

/// A Firefox profile the user can pick. No folder.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProfileDto {
    pub browser: BrowserKind,
    pub name: String,
    pub is_default: bool,
    /// Firefox has a cookie store there (it has been used).
    pub has_cookies: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionOverview {
    pub sessions: Vec<SessionDto>,
    pub browser_profiles: Vec<BrowserProfileDto>,
    /// Platforms a session can be connected for.
    pub providers: Vec<Platform>,
}

fn firefox() -> Vec<BrowserProfile> {
    browsers::firefox_root()
        .map(|r| browsers::firefox_profiles(&r))
        .unwrap_or_default()
}

fn overview(st: &LibraryState) -> Result<SessionOverview, DomainError> {
    let c = st.get()?.reader()?;
    let sessions = auth::list(&c).map_err(auth_err)?;
    Ok(SessionOverview {
        sessions: sessions.iter().map(dto).collect(),
        browser_profiles: firefox()
            .into_iter()
            .map(|p| BrowserProfileDto {
                browser: BrowserKind::Firefox,
                name: p.name,
                is_default: p.is_default,
                has_cookies: p.has_cookies,
            })
            .collect(),
        providers: PROVIDERS.to_vec(),
    })
}

#[tauri::command]
pub async fn list_sessions<R: Runtime>(app: AppHandle<R>) -> Result<SessionOverview, DomainError> {
    blocking(app, overview).await
}

fn parse_ops(scope: &[String]) -> Result<Vec<Operation>, DomainError> {
    scope
        .iter()
        .map(|s| {
            Operation::parse(s)
                .ok_or_else(|| DomainError::new(ErrorCode::InputRejected, "session-scope"))
        })
        .collect()
}

/// Connects `provider` to a Firefox profile with the operations the user
/// allowed (`metadata`, `download`). Jobs waiting for a login on that
/// platform go back to the queue.
#[tauri::command]
pub async fn connect_session<R: Runtime>(
    app: AppHandle<R>,
    provider: String,
    browser: String,
    browser_profile: String,
    scope: Vec<String>,
) -> Result<SessionDto, DomainError> {
    let a = app.clone();
    let r = blocking(app, move |st| {
        let platform = auth::provider(&provider)
            .ok_or_else(|| DomainError::new(ErrorCode::InputRejected, "session-provider"))?;
        if BrowserKind::parse(&browser) != Some(BrowserKind::Firefox) {
            // Chrome and Edge on Windows: app-bound encryption (ADR-0023).
            return Err(DomainError::new(
                ErrorCode::InputRejected,
                "session-browser",
            ));
        }
        let ops = parse_ops(&scope)?;
        let found = firefox()
            .into_iter()
            .find(|p| p.name == browser_profile)
            .ok_or_else(|| {
                DomainError::new(ErrorCode::CookieAccessFailed, "session-browser-profile")
            })?;
        let locator = found
            .path
            .to_str()
            .ok_or_else(|| {
                DomainError::new(ErrorCode::CookieAccessFailed, "session-browser-profile")
            })?
            .to_owned();
        let label = format!("{} · Firefox ({})", platform.display_name(), found.name);
        st.get()?.write(|c| {
            let p = auth::create(
                c,
                &NewProfile {
                    provider: platform,
                    label: &label,
                    method: Method::BrowserCookies,
                    browser_kind: Some(BrowserKind::Firefox),
                    profile_locator: Some(&locator),
                    consent_scope: &ops,
                },
                now(),
            )
            .map_err(auth_err)?;
            auth::unblock_provider(c, platform, now()).map_err(auth_err)?;
            Ok(dto(&p))
        })
    })
    .await;
    wake(&a);
    r
}

/// Changes what a session may be used for; an empty scope withdraws
/// consent.
#[tauri::command]
pub async fn set_session_consent<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    scope: Vec<String>,
) -> Result<SessionDto, DomainError> {
    let a = app.clone();
    let r = blocking(app, move |st| {
        let ops = parse_ops(&scope)?;
        st.get()?.write(|c| {
            let p = auth::set_consent(c, &id, &ops, now()).map_err(auth_err)?;
            if !ops.is_empty() {
                auth::unblock_provider(c, p.provider, now()).map_err(auth_err)?;
            }
            Ok(dto(&p))
        })
    })
    .await;
    wake(&a);
    r
}

/// §12.2 Clear: the app's secret and the consent go; the browser's own
/// cookies and the account are not touched.
#[tauri::command]
pub async fn clear_session<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<SessionDto, DomainError> {
    blocking(app, move |st| {
        let s = store(st)?;
        st.get()?.write(|c| {
            auth::clear(c, s.as_ref(), &id, now())
                .map(|p| dto(&p))
                .map_err(auth_err)
        })
    })
    .await
}

#[tauri::command]
pub async fn delete_session<R: Runtime>(app: AppHandle<R>, id: String) -> Result<(), DomainError> {
    blocking(app, move |st| {
        let s = store(st)?;
        st.get()?
            .write(|c| auth::delete(c, s.as_ref(), &id).map_err(auth_err))
    })
    .await
}
