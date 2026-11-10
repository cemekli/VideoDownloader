//! Authorized sessions: connect a browser profile or a cookie file to a
//! platform, change or withdraw consent, verify, reconnect, clear and
//! delete (STATE.md F5.2–F5.5, F5.7; SPEC §9.3, §12.1, §12.2, §20.3,
//! §21.1, §21.3; ADR-0022, ADR-0023, ADR-0024, ADR-0025).
//!
//! The webview sees a profile's platform, method, browser, browser profile
//! *name*, state and consent. It never sees the browser profile folder (a
//! private path), a cookie file's path or content, or a secret reference;
//! it picks a Firefox profile by name and the backend finds the folder
//! again, and the cookie file is chosen in a dialog the backend opens.

use crate::library::{internal, LibraryState};
use crate::library_commands::blocking;
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};
use vv_domain::error::{DomainError, ErrorCode};
use vv_domain::normalize::Platform;
use vv_engine::browsers::{self, BrowserProfile};
use vv_engine::cookiefile;
use vv_engine::verify::{self, VerifyOutcome};
use vv_engine::Store as _;
use vv_persistence::auth::{
    self, AuthError, AuthProfile, BrowserKind, Method, NewProfile, Operation,
};
use vv_persistence::library::{Clock, SystemClock};
use vv_secrets::{Secret, SecretStore};

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
    /// A cookie file's cookies are stored (only their presence is shown).
    pub has_cookies: bool,
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
        has_cookies: p.has_secret,
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
    /// Whether this system can keep a cookie file's cookies protected (the
    /// OS secret store; Windows only).
    pub cookie_file_supported: bool,
    /// Whether the in-app sign-in window can be used (it keeps the cookies
    /// the same way, so the same condition).
    pub app_sign_in_supported: bool,
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
        cookie_file_supported: store(st)?.available(),
        app_sign_in_supported: store(st)?.available(),
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

/// Stores `cookies` (already narrowed to `provider`) as a new profile of
/// `method` in the OS secret store and lets waiting jobs of the platform
/// run again. Nothing is left behind when it fails.
fn store_cookies(
    c: &vv_persistence::rusqlite::Connection,
    secrets: &dyn SecretStore,
    provider: Platform,
    method: Method,
    cookies: cookiefile::Filtered,
    ops: &[Operation],
    now_ms: i64,
) -> Result<SessionDto, DomainError> {
    let how = if method == Method::AppProfile {
        "in-app sign-in"
    } else {
        "cookie file"
    };
    let label = format!("{} · {how}", provider.display_name());
    let p = auth::create(
        c,
        &NewProfile {
            provider,
            label: &label,
            method,
            browser_kind: None,
            profile_locator: None,
            consent_scope: ops,
        },
        now_ms,
    )
    .map_err(auth_err)?;
    let secret = Secret::new(cookies.text.into_bytes());
    match auth::attach_secret(c, secrets, &p.id, &secret, now_ms) {
        Ok(p) => {
            auth::unblock_provider(c, provider, now_ms).map_err(auth_err)?;
            Ok(dto(&p))
        }
        Err(e) => {
            let _ = auth::delete(c, secrets, &p.id);
            Err(auth_err(e))
        }
    }
}

fn check_store_and_scope(secrets: &dyn SecretStore, ops: &[Operation]) -> Result<(), DomainError> {
    if !secrets.available() {
        return Err(DomainError::new(
            ErrorCode::CookieAccessFailed,
            "session-file-unsupported",
        ));
    }
    if ops.is_empty() {
        return Err(DomainError::new(ErrorCode::InputRejected, "session-scope"));
    }
    Ok(())
}

fn cookie_err(e: cookiefile::CookieFileError) -> DomainError {
    DomainError::new(ErrorCode::InputRejected, e.reason()).with_detail("reason", e.reason())
}

/// Connects `provider` to the cookies of a Netscape cookie file at `path`
/// (§21.1): only the platform's own cookies are kept, in the OS secret
/// store; the file is read, never written, never kept. Shared by the
/// command and the tests.
pub fn connect_cookie_file(
    c: &vv_persistence::rusqlite::Connection,
    secrets: &dyn SecretStore,
    provider: Platform,
    path: &std::path::Path,
    ops: &[Operation],
    now_ms: i64,
) -> Result<SessionDto, DomainError> {
    check_store_and_scope(secrets, ops)?;
    let filtered =
        cookiefile::read_and_filter(path, provider, now_ms / 1000).map_err(cookie_err)?;
    store_cookies(
        c,
        secrets,
        provider,
        Method::CookieFile,
        filtered,
        ops,
        now_ms,
    )
}

/// Connects `provider` to the cookies the in-app sign-in window holds
/// (§21.3, F5.7): only the platform's own cookies are kept, the same way
/// as a cookie file's. Shared by the command and the tests.
pub fn connect_app_cookies(
    c: &vv_persistence::rusqlite::Connection,
    secrets: &dyn SecretStore,
    provider: Platform,
    cookies: &[cookiefile::WebCookie],
    ops: &[Operation],
    now_ms: i64,
) -> Result<SessionDto, DomainError> {
    check_store_and_scope(secrets, ops)?;
    let filtered = cookiefile::from_webview(cookies, provider, now_ms / 1000).map_err(|e| {
        let reason = match e {
            cookiefile::CookieFileError::NoCookiesForSite => "session-app-not-signed-in",
            other => other.reason(),
        };
        DomainError::new(ErrorCode::InputRejected, reason).with_detail("reason", reason)
    })?;
    store_cookies(
        c,
        secrets,
        provider,
        Method::AppProfile,
        filtered,
        ops,
        now_ms,
    )
}

/// Connects `provider` to a cookie file the user picks in a dialog. `None`
/// when the dialog was closed. The user's file is not changed; the app
/// suggests deleting it afterwards.
#[tauri::command]
pub async fn connect_session_file<R: Runtime>(
    app: AppHandle<R>,
    provider: String,
    scope: Vec<String>,
) -> Result<Option<SessionDto>, DomainError> {
    use tauri_plugin_dialog::DialogExt;
    let a = app.clone();
    let r = blocking(app.clone(), move |st| {
        let platform = auth::provider(&provider)
            .ok_or_else(|| DomainError::new(ErrorCode::InputRejected, "session-provider"))?;
        let ops = parse_ops(&scope)?;
        let s = store(st)?;
        if !s.available() {
            return Err(DomainError::new(
                ErrorCode::CookieAccessFailed,
                "session-file-unsupported",
            ));
        }
        let mut dialog = app.dialog().file().add_filter("Cookie file", &["txt"]);
        if let Some(w) = app.get_webview_window("main") {
            dialog = dialog.set_parent(&w);
        }
        let Some(path) = dialog.blocking_pick_file().and_then(|p| p.into_path().ok()) else {
            return Ok(None);
        };
        st.get()?
            .write(|c| connect_cookie_file(c, s.as_ref(), platform, &path, &ops, now()))
            .map(Some)
    })
    .await;
    wake(&a);
    r
}

/// §12.2 Reconnect of a cleared browser session: consent again, state
/// `needs_verification`. Waiting jobs of the platform go back to the queue.
#[tauri::command]
pub async fn reconnect_session<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    scope: Vec<String>,
) -> Result<SessionDto, DomainError> {
    let a = app.clone();
    let r = blocking(app, move |st| {
        let ops = parse_ops(&scope)?;
        st.get()?.write(|c| {
            let p = auth::reconnect(c, &id, &ops, now()).map_err(auth_err)?;
            auth::unblock_provider(c, p.provider, now()).map_err(auth_err)?;
            Ok(dto(&p))
        })
    })
    .await;
    wake(&a);
    r
}

/// What `verify_session` found (no URL, no path, no cookie).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyDto {
    pub outcome: VerifyOutcome,
    pub session: SessionDto,
    /// The item used for the test.
    pub item_id: Option<String>,
    /// The §19 code behind a failure.
    pub code: Option<ErrorCode>,
}

/// Checks the session with an item of its platform that needed a login
/// (§12.1 evidence (a), F5.4). Runs at most two short metadata reads.
#[tauri::command]
pub async fn verify_session<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<VerifyDto, DomainError> {
    let r = tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<crate::engine::EngineState>();
        let engine = st
            .get()
            .ok_or_else(|| DomainError::new(ErrorCode::ToolMissing, "engine-not-ready"))?;
        let r = verify::verify_session(engine.context(), &id).map_err(auth_err)?;
        if r.outcome == VerifyOutcome::Verified {
            // A proven session: jobs waiting on it may run.
            let provider = r.profile.provider;
            engine.context().store.write(|c| {
                let _ = auth::unblock_provider(c, provider, now());
            });
            st.wake();
        }
        Ok(VerifyDto {
            outcome: r.outcome,
            session: dto(&r.profile),
            item_id: r.item_id,
            code: r.code,
        })
    })
    .await
    .map_err(|_| internal("task-join"))?;
    r
}

/// The page where the user signs in, in their own browser (§9.3 "güvenli
/// yönlendirme"). A fixed https address per platform; nothing from the
/// webview reaches the URL.
pub fn sign_in_url(p: Platform) -> Option<&'static str> {
    Some(match p {
        Platform::Reddit => "https://www.reddit.com/login/",
        Platform::Vk => "https://vk.com/",
        Platform::Youtube => "https://accounts.google.com/ServiceLogin?service=youtube",
        Platform::Facebook => "https://www.facebook.com/login/",
        Platform::Instagram => "https://www.instagram.com/accounts/login/",
        Platform::Tiktok => "https://www.tiktok.com/login",
        Platform::X => "https://x.com/i/flow/login",
        Platform::Dailymotion => "https://www.dailymotion.com/signin",
        Platform::Twitch => "https://www.twitch.tv/login",
        _ => return None,
    })
}

/// Opens the platform's sign-in page in the user's default browser.
/// Refused in Offline Mode.
#[tauri::command]
pub async fn open_sign_in<R: Runtime>(
    app: AppHandle<R>,
    provider: String,
) -> Result<(), DomainError> {
    blocking(app, move |st| {
        crate::data_commands::ensure_online(&st.get()?.reader()?)?;
        let url = auth::provider(&provider)
            .and_then(sign_in_url)
            .ok_or_else(|| DomainError::new(ErrorCode::InputRejected, "session-provider"))?;
        tauri_plugin_opener::open_url(url, None::<&str>).map_err(|_| internal("open-sign-in"))
    })
    .await
}

/// The in-app sign-in window's label. One at a time.
pub const SIGN_IN_WINDOW: &str = "sign-in";

/// Opens the in-app sign-in window on `provider`'s sign-in page (§21.3,
/// F5.7, ADR-0025). The window is a private (InPrivate) webview: nothing
/// it holds is written to disk, and it is gone when the window closes. It
/// has no access to the app's commands (the capability names only the main
/// window), and it only follows https links. Refused in Offline Mode and
/// where cookies cannot be kept protected.
#[tauri::command]
pub async fn start_app_sign_in<R: Runtime>(
    app: AppHandle<R>,
    provider: String,
) -> Result<(), DomainError> {
    let platform = auth::provider(&provider)
        .ok_or_else(|| DomainError::new(ErrorCode::InputRejected, "session-provider"))?;
    let url = sign_in_url(platform)
        .ok_or_else(|| DomainError::new(ErrorCode::InputRejected, "session-provider"))?;
    blocking(app.clone(), move |st| {
        crate::data_commands::ensure_online(&st.get()?.reader()?)?;
        check_store_and_scope(store(st)?.as_ref(), &[Operation::Download])
    })
    .await?;
    if let Some(w) = app.get_webview_window(SIGN_IN_WINDOW) {
        // A window left open for another site starts over.
        let _ = w.destroy();
    }
    let url: tauri::Url = url.parse().map_err(|_| internal("sign-in-url"))?;
    let mut b =
        tauri::WebviewWindowBuilder::new(&app, SIGN_IN_WINDOW, tauri::WebviewUrl::External(url))
            .title(format!(
                "Sign in to {} · VideoVault",
                platform.display_name()
            ))
            .inner_size(520.0, 760.0)
            .incognito(true)
            // https pages, plus the blank and blob frames sign-in and
            // captcha pages use while they load. Never file, http or an
            // app's own scheme.
            .on_navigation(|u| matches!(u.scheme(), "https" | "about" | "blob"))
            // "Sign in with Google/Apple/..." and some captcha steps open a
            // popup and hand the result back to this page through
            // `window.opener`. An https popup is allowed as WebView2's own
            // window: it shares this window's private store (the cookies stay
            // in one place) and keeps the opener link. Anything else is
            // refused. The popup has no access to the app either.
            .on_new_window(|u, _| {
                if u.scheme() == "https" {
                    tauri::webview::NewWindowResponse::Allow
                } else {
                    tauri::webview::NewWindowResponse::Deny
                }
            });
    if let Some(main) = app.get_webview_window("main") {
        b = b.parent(&main).map_err(|_| internal("sign-in-window"))?;
    }
    b.build().map_err(|_| internal("sign-in-window"))?;
    Ok(())
}

/// Takes `provider`'s cookies from the sign-in window, keeps them as an
/// `app_profile` session and closes the window. `session-app-not-signed-in`
/// when the window holds none of the site's cookies yet (the user has not
/// signed in), `session-app-closed` when the window was closed.
#[tauri::command]
pub async fn finish_app_sign_in<R: Runtime>(
    app: AppHandle<R>,
    provider: String,
    scope: Vec<String>,
) -> Result<SessionDto, DomainError> {
    let platform = auth::provider(&provider)
        .ok_or_else(|| DomainError::new(ErrorCode::InputRejected, "session-provider"))?;
    let ops = parse_ops(&scope)?;
    let w = app.get_webview_window(SIGN_IN_WINDOW).ok_or_else(|| {
        DomainError::new(ErrorCode::InputRejected, "session-app-closed")
            .with_detail("reason", "session-app-closed")
    })?;
    // WebView2 deadlocks when cookies are read on the main thread or in a
    // synchronous command (tauri-apps/wry#583): this command is async and
    // reads them on a worker thread.
    let all = tauri::async_runtime::spawn_blocking(move || w.cookies())
        .await
        .map_err(|_| internal("sign-in-cookies"))?
        .map_err(|_| DomainError::new(ErrorCode::CookieAccessFailed, "sign-in-cookies"))?;
    let cookies: Vec<cookiefile::WebCookie> = all
        .iter()
        .filter_map(|c| {
            let domain = c.domain()?.to_owned();
            Some(cookiefile::WebCookie {
                host_only: !domain.starts_with('.'),
                domain,
                path: c.path().unwrap_or("/").to_owned(),
                secure: c.secure().unwrap_or(false),
                http_only: c.http_only().unwrap_or(false),
                expires: c.expires_datetime().map_or(0, |t| t.unix_timestamp()),
                name: c.name().to_owned(),
                value: c.value().to_owned(),
            })
        })
        .collect();
    drop(all);
    let a = app.clone();
    let r = blocking(app.clone(), move |st| {
        let s = store(st)?;
        st.get()?
            .write(|c| connect_app_cookies(c, s.as_ref(), platform, &cookies, &ops, now()))
    })
    .await;
    if r.is_ok() {
        if let Some(w) = app.get_webview_window(SIGN_IN_WINDOW) {
            let _ = w.destroy();
        }
    }
    wake(&a);
    r
}

/// Closes the sign-in window without keeping anything.
#[tauri::command]
pub async fn cancel_app_sign_in<R: Runtime>(app: AppHandle<R>) -> Result<(), DomainError> {
    if let Some(w) = app.get_webview_window(SIGN_IN_WINDOW) {
        let _ = w.destroy();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vv_persistence::{Database, OpenOptions};

    const FILE: &str = "# Netscape HTTP Cookie File\n\
.vk.com\tTRUE\t/\tTRUE\t0\tremixsid\tVK-CANARY-91\n\
.reddit.com\tTRUE\t/\tTRUE\t0\treddit_session\tOTHER-SITE\n";

    #[test]
    fn cookie_file_keeps_only_the_sites_cookies_and_never_shows_them() {
        let d = tempfile::tempdir().unwrap();
        let mut db = Database::open(&d.path().join("l.sqlite"), &OpenOptions::default()).unwrap();
        let store = vv_secrets::MemoryStore::new();
        let file = d.path().join("cookies.txt");
        std::fs::write(&file, FILE).unwrap();
        let c = db.writer();
        let ops = [Operation::Metadata, Operation::Download, Operation::Verify];
        let s = connect_cookie_file(c, &store, Platform::Vk, &file, &ops, 1_000).unwrap();
        assert_eq!(s.method, Method::CookieFile);
        assert!(s.has_cookies);
        assert_eq!(s.browser_profile, None);
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            !json.contains("VK-CANARY")
                && !json.contains("cookies.txt")
                && !json.contains("vvsec:"),
            "{json}"
        );
        // The file is unchanged; only VK's cookie was stored.
        assert_eq!(std::fs::read_to_string(&file).unwrap(), FILE);
        let kept = auth::read_secret(c, &store, &s.id, Operation::Download).unwrap();
        let kept = String::from_utf8(kept.expose().to_vec()).unwrap();
        assert!(kept.contains("VK-CANARY-91") && !kept.contains("OTHER-SITE"));

        // A file without the site's cookies, no consent, no secure store:
        // refused, and nothing is left behind.
        let e = connect_cookie_file(c, &store, Platform::Twitch, &file, &ops, 2_000).unwrap_err();
        assert_eq!(e.code, ErrorCode::InputRejected);
        assert!(connect_cookie_file(c, &store, Platform::Vk, &file, &[], 3_000).is_err());
        let e = connect_cookie_file(
            c,
            &vv_secrets::UnavailableStore,
            Platform::Vk,
            &file,
            &ops,
            4_000,
        )
        .unwrap_err();
        assert_eq!(e.code, ErrorCode::CookieAccessFailed);
        assert_eq!(auth::list(c).unwrap().len(), 1);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn app_sign_in_keeps_only_the_sites_cookies_and_never_shows_them() {
        let d = tempfile::tempdir().unwrap();
        let mut db = Database::open(&d.path().join("l.sqlite"), &OpenOptions::default()).unwrap();
        let store = vv_secrets::MemoryStore::new();
        let c = db.writer();
        let ops = [Operation::Metadata, Operation::Download, Operation::Verify];
        let cookie = |domain: &str, value: &str| cookiefile::WebCookie {
            domain: domain.into(),
            host_only: false,
            path: "/".into(),
            secure: true,
            http_only: true,
            expires: 0,
            name: "remixsid".into(),
            value: value.into(),
        };
        let jar = [
            cookie(".vk.com", "VK-CANARY-77"),
            cookie(".google.com", "OTHER-SITE"),
        ];
        let s = connect_app_cookies(c, &store, Platform::Vk, &jar, &ops, 1_000).unwrap();
        assert_eq!(s.method, Method::AppProfile);
        assert_eq!(s.label, "VK · in-app sign-in");
        assert!(s.has_cookies);
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            !json.contains("VK-CANARY") && !json.contains("vvsec:"),
            "{json}"
        );
        let kept = auth::read_secret(c, &store, &s.id, Operation::Download).unwrap();
        let kept = String::from_utf8(kept.expose().to_vec()).unwrap();
        assert!(kept.contains("VK-CANARY-77") && !kept.contains("OTHER-SITE"));
        let p = auth::get(c, &s.id).unwrap();
        assert!(vv_engine::auth::runnable(&p));

        // Not signed in yet (no cookie of the site), no consent, no secure
        // store: refused, and nothing is left behind.
        let e = connect_app_cookies(c, &store, Platform::Reddit, &jar, &ops, 2_000).unwrap_err();
        assert_eq!(e.code, ErrorCode::InputRejected);
        assert_eq!(
            e.safe_details.get("reason").map(String::as_str),
            Some("session-app-not-signed-in")
        );
        assert!(connect_app_cookies(c, &store, Platform::Vk, &jar, &[], 3_000).is_err());
        let e = connect_app_cookies(
            c,
            &vv_secrets::UnavailableStore,
            Platform::Vk,
            &jar,
            &ops,
            4_000,
        )
        .unwrap_err();
        assert_eq!(e.code, ErrorCode::CookieAccessFailed);
        assert_eq!(auth::list(c).unwrap().len(), 1);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn sign_in_pages_are_fixed_https_addresses() {
        for p in PROVIDERS {
            let u = sign_in_url(*p).unwrap();
            assert!(u.starts_with("https://"), "{u}");
            vv_domain::url_policy::validate_capture_url(u).unwrap();
        }
        assert_eq!(sign_in_url(Platform::Generic), None);
    }
}
