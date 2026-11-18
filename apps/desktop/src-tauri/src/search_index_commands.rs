//! Search by meaning: the user's embedding API and the index as Settings and
//! AI Processing show them; setting the API, building, rebuilding and
//! canceling the index (STATE.md F7.1; SPEC §16.3, AI-C07, AC-24;
//! ADR-0035).
//!
//! VideoVault hosts no model: `set_embedding_api` checks the address with
//! one request and saves it only when vectors came back. The API key goes
//! to the OS credential store; the webview never sees it again, nor a
//! vector. A host other than this PC needs `allowRemote` (the screen asks
//! first: transcript text goes there).

use crate::download_commands::{job_dto, JobDto};
use crate::engine::EngineState;
use crate::library::LibraryState;
use crate::library_commands::blocking;
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};
use vv_domain::error::{DomainError, ErrorCode};
use vv_engine::embed::{self, Api, EmbedError, IndexError, UrlError};
use vv_engine::Dirs;
use vv_persistence::jobs::{self, JobKind};
use vv_persistence::library::{Clock, SystemClock};
use vv_persistence::rusqlite::Connection;
use vv_persistence::settings::{self, EmbeddingApi};
use vv_persistence::vectors;
use vv_secrets::{Secret, SecretRef, SecretStore};

fn now() -> i64 {
    SystemClock.now_ms()
}

/// One index generation (§16.3) as the screen shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexDto {
    /// The model name the API was asked for.
    pub model_id: String,
    /// The API address its vectors came from.
    pub url: String,
    pub dims: usize,
    /// Chunks with a vector / transcript chunks in the library.
    pub done: i64,
    pub total: i64,
    pub created_at: i64,
    pub activated_at: Option<i64>,
}

/// The embedding API in the settings (never the key itself).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiDto {
    pub url: String,
    pub model: String,
    pub dims: usize,
    pub has_key: bool,
    /// The address is this PC: text does not leave it, works offline.
    pub local: bool,
    pub checked_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchIndexDto {
    /// `null` until the user sets and checks one.
    pub api: Option<ApiDto>,
    /// The index searches use, if any.
    pub active: Option<IndexDto>,
    /// A build under way beside it (AC-24).
    pub building: Option<IndexDto>,
    /// The open or last index job.
    pub job: Option<JobDto>,
    /// The active index was made with the API and model in the settings,
    /// so searches by meaning can run.
    pub searchable: bool,
}

fn index_dto(c: &Connection, g: &vectors::Generation) -> IndexDto {
    let (done, total) = vectors::coverage(c, g.seq).unwrap_or((0, 0));
    IndexDto {
        model_id: g.producer.model_id.clone(),
        url: g.producer.model_version.clone(),
        dims: g.producer.dims,
        done,
        total,
        created_at: g.created_at,
        activated_at: g.activated_at,
    }
}

fn last_index_job(c: &Connection, staging: &std::path::Path) -> Option<JobDto> {
    jobs::list(c, &[], 500)
        .ok()?
        .into_iter()
        .filter(|j| matches!(j.kind, JobKind::Embedding | JobKind::Reindex))
        .max_by_key(|j| (!j.status.is_terminal(), j.created_at, j.id.clone()))
        .map(|j| job_dto(c, staging, &j))
}

pub fn search_index(c: &Connection, dirs: &Dirs) -> SearchIndexDto {
    let api = embed::configured(c);
    let active = vectors::active(c).ok().flatten();
    let searchable = match (&api, &active) {
        (Some(a), Some(g)) => embed::producer(a) == g.producer,
        _ => false,
    };
    SearchIndexDto {
        api: api.map(|a| ApiDto {
            local: embed::is_loopback(&a.url),
            has_key: a.key_ref.is_some(),
            url: a.url,
            model: a.model,
            dims: a.dims,
            checked_at: a.checked_at,
        }),
        active: active.as_ref().map(|g| index_dto(c, g)),
        building: vectors::building(c)
            .ok()
            .flatten()
            .map(|g| index_dto(c, &g)),
        job: last_index_job(c, &dirs.staging),
        searchable,
    }
}

fn dirs(st: &LibraryState) -> Result<Dirs, DomainError> {
    Ok(Dirs::under(st.get()?.data_dir()))
}

pub(crate) fn secrets(st: &LibraryState) -> Result<Box<dyn SecretStore>, DomainError> {
    Ok(vv_secrets::os_store(&st.get()?.data_dir().join("secrets")))
}

fn wake<R: Runtime>(app: &AppHandle<R>) {
    if let Some(e) = app.try_state::<EngineState>() {
        e.wake();
    }
}

#[tauri::command]
pub async fn get_search_index<R: Runtime>(
    app: AppHandle<R>,
) -> Result<SearchIndexDto, DomainError> {
    blocking(app, move |st| {
        let d = dirs(st)?;
        Ok(search_index(&st.get()?.reader()?, &d))
    })
    .await
}

fn embed_error(e: &EmbedError) -> DomainError {
    match e {
        EmbedError::Stopped => DomainError::new(ErrorCode::Canceled, "embedding-check-stopped"),
        EmbedError::Failed(code, _) => DomainError::new(*code, "embedding-api-check"),
    }
}

/// Checks `url` + `model` with one request and saves them with the vector
/// length that came back. `key`: `null` keeps the saved key, `""` removes
/// it, anything else replaces it. A host other than this PC is refused
/// with `URL_BLOCKED` unless `allowRemote` (AI-C07), and in Offline Mode
/// with `OFFLINE_MODE` (DATA-05). The index is not touched: a new API or
/// model needs "Build index", and its index is built beside the old one.
#[tauri::command]
pub async fn set_embedding_api<R: Runtime>(
    app: AppHandle<R>,
    url: String,
    model: String,
    key: Option<String>,
    allow_remote: bool,
) -> Result<SearchIndexDto, DomainError> {
    let a = app.clone();
    let r = blocking(app, move |st| {
        let d = dirs(st)?;
        let store = secrets(st)?;
        let base = embed::normalize_url(&url).map_err(|e| match e {
            UrlError::Invalid => DomainError::new(ErrorCode::InvalidUrl, "embedding-url-invalid"),
            UrlError::Credentials => {
                DomainError::new(ErrorCode::UrlHasCredentials, "embedding-url-credentials")
            }
        })?;
        let model = model.trim().to_owned();
        let (old, offline) = {
            let c = st.get()?.reader()?;
            (
                embed::configured(&c),
                settings::offline_mode(&c).unwrap_or(false),
            )
        };
        let old_key = old.as_ref().and_then(|o| o.key_ref.clone());
        let key_text: Option<String> = match &key {
            Some(k) if k.is_empty() => None,
            Some(k) => Some(k.clone()),
            None => match &old_key {
                Some(r) => SecretRef::parse(r)
                    .and_then(|r| store.get(&r).ok())
                    .map(|s| String::from_utf8_lossy(s.expose()).into_owned()),
                None => None,
            },
        };
        let remote_ok = allow_remote || embed::is_loopback(&base);
        let api = Api::new(&base, &model, key_text.clone(), remote_ok, offline)
            .map_err(|e| embed_error(&e))?;
        // Network, with no library connection held.
        let dims = embed::check(&api).map_err(|e| embed_error(&e))?;
        // The key: a new one replaces the old in the credential store.
        let key_ref = match &key {
            None => old_key.clone(),
            Some(k) if k.is_empty() => None,
            Some(k) => Some(
                store
                    .put(&Secret::new(k.as_bytes().to_vec()))
                    .map_err(|_| {
                        DomainError::new(ErrorCode::CookieAccessFailed, "embedding-key-store")
                    })?
                    .as_str()
                    .to_owned(),
            ),
        };
        if key.is_some() {
            if let Some(r) = old_key.as_deref().and_then(SecretRef::parse) {
                let _ = store.delete(&r);
            }
        }
        let cfg = EmbeddingApi {
            url: base,
            model,
            dims,
            key_ref,
            remote_allowed: remote_ok,
            checked_at: now(),
        };
        st.get()?.write(|c| {
            settings::set_embedding_api(c, Some(&cfg), now())
                .map_err(|_| crate::library::internal("embedding-api-save"))?;
            Ok(search_index(c, &d))
        })
    })
    .await;
    crate::meaning::clear(&a);
    r
}

/// Forgets the embedding API and its key. Search by meaning goes off; the
/// index stays until another API is set and built (§16.3: lexical search
/// keeps working, AC-40).
#[tauri::command]
pub async fn clear_embedding_api<R: Runtime>(
    app: AppHandle<R>,
) -> Result<SearchIndexDto, DomainError> {
    let a = app.clone();
    let r = blocking(app, move |st| {
        let d = dirs(st)?;
        let store = secrets(st)?;
        st.get()?.write(|c| {
            if let Some(r) = embed::configured(c)
                .and_then(|o| o.key_ref)
                .as_deref()
                .and_then(SecretRef::parse)
            {
                let _ = store.delete(&r);
            }
            embed::cancel_build(c, now());
            settings::set_embedding_api(c, None, now())
                .map_err(|_| crate::library::internal("embedding-api-save"))?;
            Ok(search_index(c, &d))
        })
    })
    .await;
    crate::meaning::clear(&a);
    wake(&a);
    r
}

/// Builds the index for the API in the settings, or adds what it misses;
/// `rebuild` starts a new one beside the active index.
#[tauri::command]
pub async fn build_search_index<R: Runtime>(
    app: AppHandle<R>,
    rebuild: bool,
) -> Result<SearchIndexDto, DomainError> {
    let a = app.clone();
    let r = blocking(app, move |st| {
        let d = dirs(st)?;
        st.get()?.write(|c| {
            embed::request_index(c, rebuild, now()).map_err(|e| match e {
                IndexError::NoApi => {
                    DomainError::new(ErrorCode::ModelMissing, "embedding-api-missing")
                }
                IndexError::Db => crate::library::internal("index-db"),
            })?;
            Ok(search_index(c, &d))
        })
    })
    .await;
    wake(&a);
    r
}

/// Cancels the build under way; the active index stays as it was (AC-24).
#[tauri::command]
pub async fn cancel_search_index<R: Runtime>(
    app: AppHandle<R>,
) -> Result<SearchIndexDto, DomainError> {
    let a = app.clone();
    let r = blocking(app, move |st| {
        let d = dirs(st)?;
        st.get()?.write(|c| {
            embed::cancel_build(c, now());
            Ok(search_index(c, &d))
        })
    })
    .await;
    wake(&a);
    r
}
