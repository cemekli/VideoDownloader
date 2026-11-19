//! Tag suggestions (STATE.md F7.3; SPEC AI-B01…B04, AI-C07; ADR-0037): the
//! user's chat API in AI Processing, "Suggest tags" for one item, "Suggest
//! for all" across the library, and accepting or rejecting a suggestion.
//!
//! VideoVault hosts no model: `set_suggest_api` checks the address with one
//! short question and saves it only when a tag list came back. The key
//! goes to the OS credential store and never comes back to the webview.
//! A suggestion never changes tags by itself: `decide_ai_proposal` with
//! `accept` adds the tag (as an undoable "add tag") and marks the relation
//! `ai_accepted`; a rejected value is not suggested for that item again.
//!
//! "Suggest for all" runs on one background thread, one item at a time,
//! and skips items whose text was already asked with this model and API
//! (AI-B04). It is not a queue job: it holds no files, is cheap to start
//! again, and an interrupted run simply continues where it stopped.

use crate::engine::EngineState;
use crate::library::{internal, LibraryState};
use crate::library_commands::blocking;
use crate::organize_commands::{ops, OpResult};
use crate::search_index_commands::secrets;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, Runtime};
use vv_domain::error::{DomainError, ErrorCode};
use vv_engine::embed::{self, EmbedError, UrlError};
use vv_engine::suggest::{self, ChatApi, Outcome};
use vv_engine::Store;
use vv_persistence::library::organize;
use vv_persistence::library::{Clock, SystemClock};
use vv_persistence::proposals::{self, Proposal};
use vv_persistence::rusqlite::{Connection, TransactionBehavior};
use vv_persistence::settings::{self, SuggestApi};
use vv_secrets::{Secret, SecretRef};

fn now() -> i64 {
    SystemClock.now_ms()
}

/// The chat API in the settings (never the key itself).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestApiDto {
    pub url: String,
    pub model: String,
    pub has_key: bool,
    /// The address is this PC: text does not leave it, works offline.
    pub local: bool,
    pub checked_at: i64,
}

/// "Suggest for all": the run under way or the last one of this app run.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestRunDto {
    pub running: bool,
    /// Items looked at / all live items.
    pub done: u32,
    pub total: u32,
    /// Items the model was asked about (the rest were current or had no text).
    pub asked: u32,
    /// Suggestions those answers added.
    pub suggested: u32,
    pub failed: u32,
    /// Why the run stopped early, when it did.
    pub error_code: Option<ErrorCode>,
    pub canceled: bool,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagSuggestDto {
    /// `null` until the user sets and checks one.
    pub api: Option<SuggestApiDto>,
    pub run: Option<SuggestRunDto>,
    /// Pending suggestions in the library.
    pub pending: i64,
}

/// One pending suggestion as the details panel shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalDto {
    pub id: String,
    pub item_id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub value: String,
    /// The model that suggested it.
    pub model_id: String,
}

fn proposal_dto(p: &Proposal) -> ProposalDto {
    ProposalDto {
        id: p.id.clone(),
        item_id: p.item_id.clone(),
        kind: p.kind.clone(),
        value: p.value.clone(),
        model_id: p.model_id.clone(),
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestOutcome {
    /// The model was asked.
    Asked,
    /// Nothing changed since the last suggestions (AI-B04).
    Current,
    /// The item has no title, description or transcript.
    NoText,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemSuggestions {
    pub outcome: SuggestOutcome,
    pub proposals: Vec<ProposalDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecideResult {
    /// The item's pending suggestions after the decision.
    pub proposals: Vec<ProposalDto>,
    /// The "add tag" an accept made (Undo reverts it).
    pub op: Option<OpResult>,
}

/// The "Suggest for all" run, shared by the commands and its thread.
#[derive(Default)]
pub struct SuggestState {
    run: Mutex<Option<SuggestRunDto>>,
    stop: Arc<AtomicBool>,
}

impl SuggestState {
    fn snapshot(&self) -> Option<SuggestRunDto> {
        self.run.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn update(&self, f: impl FnOnce(&mut SuggestRunDto)) {
        if let Some(r) = self.run.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            f(r);
        }
    }

    fn running(&self) -> bool {
        self.snapshot().is_some_and(|r| r.running)
    }
}

pub fn view(c: &Connection, run: Option<SuggestRunDto>) -> TagSuggestDto {
    TagSuggestDto {
        api: suggest::configured(c).map(|a| SuggestApiDto {
            local: embed::is_loopback(&a.url),
            has_key: a.key_ref.is_some(),
            url: a.url,
            model: a.model,
            checked_at: a.checked_at,
        }),
        run,
        pending: proposals::pending_count(c).unwrap_or(0),
    }
}

fn run_of<R: Runtime>(app: &AppHandle<R>) -> Option<SuggestRunDto> {
    app.try_state::<SuggestState>().and_then(|s| s.snapshot())
}

fn api_error(e: &EmbedError) -> DomainError {
    match e {
        EmbedError::Stopped => DomainError::new(ErrorCode::Canceled, "suggest-stopped"),
        EmbedError::Failed(code, _) => DomainError::new(*code, "suggest-api"),
    }
}

#[tauri::command]
pub async fn get_tag_suggest<R: Runtime>(app: AppHandle<R>) -> Result<TagSuggestDto, DomainError> {
    let run = run_of(&app);
    blocking(app, move |st| Ok(view(&st.get()?.reader()?, run))).await
}

/// Checks `url` + `model` with one short question and saves them. `key`:
/// `null` keeps the saved key, `""` removes it, anything else replaces it.
/// A host other than this PC is refused with `URL_BLOCKED` unless
/// `allowRemote` (AI-C07), and in Offline Mode with `OFFLINE_MODE`.
#[tauri::command]
pub async fn set_suggest_api<R: Runtime>(
    app: AppHandle<R>,
    url: String,
    model: String,
    key: Option<String>,
    allow_remote: bool,
) -> Result<TagSuggestDto, DomainError> {
    let run = run_of(&app);
    blocking(app, move |st| {
        let store = secrets(st)?;
        let base = embed::normalize_url(&url).map_err(|e| match e {
            UrlError::Invalid => DomainError::new(ErrorCode::InvalidUrl, "suggest-url-invalid"),
            UrlError::Credentials => {
                DomainError::new(ErrorCode::UrlHasCredentials, "suggest-url-credentials")
            }
        })?;
        let model = model.trim().to_owned();
        let (old, offline) = {
            let c = st.get()?.reader()?;
            (
                suggest::configured(&c),
                settings::offline_mode(&c).unwrap_or(false),
            )
        };
        let old_key = old.as_ref().and_then(|o| o.key_ref.clone());
        let key_text: Option<String> = match &key {
            Some(k) if k.is_empty() => None,
            Some(k) => Some(k.clone()),
            None => old_key
                .as_deref()
                .and_then(SecretRef::parse)
                .and_then(|r| store.get(&r).ok())
                .map(|s| String::from_utf8_lossy(s.expose()).into_owned()),
        };
        let remote_ok = allow_remote || embed::is_loopback(&base);
        let api =
            ChatApi::new(&base, &model, key_text, remote_ok, offline).map_err(|e| api_error(&e))?;
        // Network, with no library connection held.
        suggest::check(&api).map_err(|e| api_error(&e))?;
        let key_ref = match &key {
            None => old_key.clone(),
            Some(k) if k.is_empty() => None,
            Some(k) => Some(
                store
                    .put(&Secret::new(k.as_bytes().to_vec()))
                    .map_err(|_| {
                        DomainError::new(ErrorCode::CookieAccessFailed, "suggest-key-store")
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
        let cfg = SuggestApi {
            url: base,
            model,
            key_ref,
            remote_allowed: remote_ok,
            checked_at: now(),
        };
        st.get()?.write(|c| {
            settings::set_suggest_api(c, Some(&cfg), now())
                .map_err(|_| internal("suggest-api-save"))?;
            Ok(view(c, run))
        })
    })
    .await
}

/// Forgets the chat API and its key and stops "Suggest for all".
/// Suggestions already made stay.
#[tauri::command]
pub async fn clear_suggest_api<R: Runtime>(
    app: AppHandle<R>,
) -> Result<TagSuggestDto, DomainError> {
    if let Some(s) = app.try_state::<SuggestState>() {
        s.stop.store(true, Ordering::SeqCst);
    }
    let run = run_of(&app);
    blocking(app, move |st| {
        let store = secrets(st)?;
        st.get()?.write(|c| {
            if let Some(r) = suggest::configured(c)
                .and_then(|o| o.key_ref)
                .as_deref()
                .and_then(SecretRef::parse)
            {
                let _ = store.delete(&r);
            }
            settings::set_suggest_api(c, None, now()).map_err(|_| internal("suggest-api-save"))?;
            Ok(view(c, run))
        })
    })
    .await
}

/// The item's pending suggestions (SPEC §19 `list_ai_proposals`).
#[tauri::command]
pub async fn list_ai_proposals<R: Runtime>(
    app: AppHandle<R>,
    item_id: String,
) -> Result<Vec<ProposalDto>, DomainError> {
    blocking(app, move |st| {
        let c = st.get()?.reader()?;
        let ps = proposals::pending(&c, &item_id).map_err(|_| internal("proposals-list"))?;
        Ok(ps.iter().map(proposal_dto).collect())
    })
    .await
}

/// Asks the chat API for tags for one item now. `force` asks again even
/// when the item's text did not change since the last suggestions.
#[tauri::command]
pub async fn suggest_tags<R: Runtime>(
    app: AppHandle<R>,
    item_id: String,
    force: bool,
) -> Result<ItemSuggestions, DomainError> {
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<EngineState>();
        let engine = st
            .get()
            .ok_or_else(|| DomainError::new(ErrorCode::ToolMissing, "engine-not-ready"))?;
        let cx = engine.context();
        let outcome =
            suggest::suggest_item(cx, &item_id, force, || false).map_err(|e| api_error(&e))?;
        let (outcome, ps) = match outcome {
            Outcome::Asked(ps) => (SuggestOutcome::Asked, ps),
            Outcome::Current => (
                SuggestOutcome::Current,
                cx.store
                    .read(|c| proposals::pending(c, &item_id))
                    .map_err(|_| internal("proposals-list"))?,
            ),
            Outcome::NoText => (SuggestOutcome::NoText, Vec::new()),
        };
        Ok(ItemSuggestions {
            outcome,
            proposals: ps.iter().map(proposal_dto).collect(),
        })
    })
    .await
    .map_err(|_| internal("task-join"))?
}

/// Accepts or rejects one suggestion (SPEC §19 `decide_ai_proposal`). An
/// accept adds the tag, named `name` when the user edited it, as an
/// undoable "add tag", and marks it as coming from the suggestion.
pub fn decide(
    c: &Connection,
    session: &str,
    now: i64,
    id: &str,
    accept: bool,
    name: Option<&str>,
) -> Result<DecideResult, DomainError> {
    let p = proposals::get(c, id)
        .map_err(|_| internal("proposal-get"))?
        .ok_or_else(|| internal("proposal-not-found"))?;
    if p.status != proposals::Status::Pending {
        return Err(internal("proposal-decided"));
    }
    let op = if accept {
        let name = name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .unwrap_or(&p.value)
            .to_owned();
        let r = ops::tag_items(c, session, now, std::slice::from_ref(&p.item_id), &name)?;
        if let Some(t) = organize::tag_by_name(c, &name).map_err(|_| internal("tag-get"))? {
            proposals::link_tag(c, &p.item_id, &t.id, &p.id, now)
                .map_err(|_| internal("proposal-link"))?;
        }
        Some(r)
    } else {
        None
    };
    proposals::decide(c, id, accept, now).map_err(|_| internal("proposal-decide"))?;
    let ps = proposals::pending(c, &p.item_id).map_err(|_| internal("proposals-list"))?;
    Ok(DecideResult {
        proposals: ps.iter().map(proposal_dto).collect(),
        op,
    })
}

#[tauri::command]
pub async fn decide_ai_proposal<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    accept: bool,
    name: Option<String>,
) -> Result<DecideResult, DomainError> {
    blocking(app, move |st: &LibraryState| {
        let session = st.session().to_owned();
        st.get()?.write(|c| {
            let tx = c
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|_| internal("tx-begin"))?;
            let r = decide(&tx, &session, now(), &id, accept, name.as_deref())?;
            tx.commit().map_err(|_| internal("tx-commit"))?;
            Ok(r)
        })
    })
    .await
}

/// Pending tag suggestions across the library, grouped by value (the
/// review screen; ADR-0038).
#[tauri::command]
pub async fn list_ai_proposal_groups<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Vec<proposals::Group>, DomainError> {
    blocking(app, move |st| {
        let c = st.get()?.reader()?;
        proposals::pending_groups(&c).map_err(|_| internal("proposal-groups"))
    })
    .await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecideManyResult {
    /// Suggestions accepted or rejected.
    pub decided: u32,
    /// The one "add tag" an accept made for all the items (Undo reverts it).
    pub op: Option<OpResult>,
}

/// Accepts or rejects several pending suggestions at once (the review
/// screen). An accept adds one tag, `name` or the first suggestion's
/// value, to every item as a single undoable "add tag", and marks each
/// relation as coming from its suggestion. All or nothing: a suggestion
/// that is missing or already decided refuses the whole call.
pub fn decide_many(
    c: &Connection,
    session: &str,
    now: i64,
    ids: &[String],
    accept: bool,
    name: Option<&str>,
) -> Result<DecideManyResult, DomainError> {
    ops::check_bulk(ids)?;
    let mut ps: Vec<Proposal> = Vec::with_capacity(ids.len());
    let mut seen = std::collections::HashSet::new();
    for id in ids {
        if !seen.insert(id.as_str()) {
            continue;
        }
        let p = proposals::get(c, id)
            .map_err(|_| internal("proposal-get"))?
            .ok_or_else(|| internal("proposal-not-found"))?;
        if p.status != proposals::Status::Pending {
            return Err(internal("proposal-decided"));
        }
        ps.push(p);
    }
    let Some(first) = ps.first() else {
        return Ok(DecideManyResult {
            decided: 0,
            op: None,
        });
    };
    let op = if accept {
        let name = name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .unwrap_or(&first.value)
            .to_owned();
        let mut items: Vec<String> = Vec::with_capacity(ps.len());
        for p in &ps {
            if !items.contains(&p.item_id) {
                items.push(p.item_id.clone());
            }
        }
        let r = ops::tag_items(c, session, now, &items, &name)?;
        if let Some(t) = organize::tag_by_name(c, &name).map_err(|_| internal("tag-get"))? {
            for p in &ps {
                proposals::link_tag(c, &p.item_id, &t.id, &p.id, now)
                    .map_err(|_| internal("proposal-link"))?;
            }
        }
        Some(r)
    } else {
        None
    };
    for p in &ps {
        proposals::decide(c, &p.id, accept, now).map_err(|_| internal("proposal-decide"))?;
    }
    Ok(DecideManyResult {
        decided: ps.len() as u32,
        op,
    })
}

#[tauri::command]
pub async fn decide_ai_proposals<R: Runtime>(
    app: AppHandle<R>,
    ids: Vec<String>,
    accept: bool,
    name: Option<String>,
) -> Result<DecideManyResult, DomainError> {
    blocking(app, move |st: &LibraryState| {
        let session = st.session().to_owned();
        st.get()?.write(|c| {
            let tx = c
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|_| internal("tx-begin"))?;
            let r = decide_many(&tx, &session, now(), &ids, accept, name.as_deref())?;
            tx.commit().map_err(|_| internal("tx-commit"))?;
            Ok(r)
        })
    })
    .await
}

/// Starts "Suggest for all": every live item, newest first, skipping the
/// ones already current (all of them again with `force`).
#[tauri::command]
pub async fn suggest_all<R: Runtime>(
    app: AppHandle<R>,
    force: bool,
) -> Result<TagSuggestDto, DomainError> {
    let state = app
        .try_state::<SuggestState>()
        .ok_or_else(|| internal("suggest-state"))?;
    if !state.running() {
        let engine = app
            .try_state::<EngineState>()
            .ok_or_else(|| internal("engine-state"))?;
        let engine = engine
            .get()
            .ok_or_else(|| DomainError::new(ErrorCode::ToolMissing, "engine-not-ready"))?;
        let cx = engine.context();
        let cfg = cx
            .store
            .read(suggest::configured)
            .ok_or_else(|| DomainError::new(ErrorCode::ModelMissing, "suggest-api-missing"))?;
        let api = ChatApi::from_settings(&cfg, cx.secrets.as_ref(), cx.is_offline())
            .map_err(|e| api_error(&e))?;
        let items = cx
            .store
            .read(proposals::live_items)
            .map_err(|_| internal("suggest-items"))?;
        state.stop.store(false, Ordering::SeqCst);
        *state.run.lock().unwrap_or_else(|e| e.into_inner()) = Some(SuggestRunDto {
            running: true,
            total: items.len() as u32,
            started_at: now(),
            ..Default::default()
        });
        let a = app.clone();
        std::thread::spawn(move || run_all(&a, &api, &items, force));
    }
    get_tag_suggest(app).await
}

fn run_all<R: Runtime>(app: &AppHandle<R>, api: &ChatApi, items: &[String], force: bool) {
    let state = app.state::<SuggestState>();
    let engines = app.state::<EngineState>();
    let Some(engine) = engines.get() else {
        state.update(|r| {
            r.running = false;
            r.error_code = Some(ErrorCode::ToolMissing);
            r.finished_at = Some(now());
        });
        return;
    };
    let cx = engine.context();
    let stop = state.stop.clone();
    // Several failures in a row: the API is down, stop rather than go on.
    let mut failed_in_row = 0;
    for id in items {
        if stop.load(Ordering::SeqCst) {
            state.update(|r| r.canceled = true);
            break;
        }
        match suggest::suggest_with(cx, api, id, force, || stop.load(Ordering::SeqCst)) {
            Ok(Outcome::Asked(ps)) => {
                failed_in_row = 0;
                state.update(|r| {
                    r.asked += 1;
                    r.suggested += ps.len() as u32;
                });
            }
            Ok(_) => {}
            Err(EmbedError::Stopped) => {
                state.update(|r| r.canceled = true);
                break;
            }
            Err(EmbedError::Failed(code, _)) => {
                failed_in_row += 1;
                state.update(|r| {
                    r.failed += 1;
                    r.error_code = Some(code);
                });
                let fatal = matches!(
                    code,
                    ErrorCode::AccessDenied
                        | ErrorCode::ModelMissing
                        | ErrorCode::NetworkUnavailable
                        | ErrorCode::OfflineMode
                        | ErrorCode::UrlBlocked
                );
                if fatal || failed_in_row >= 3 {
                    break;
                }
            }
        }
        state.update(|r| r.done += 1);
    }
    state.update(|r| {
        r.running = false;
        r.finished_at = Some(now());
        if r.failed == 0 {
            r.error_code = None;
        }
    });
}

/// Stops "Suggest for all" after the item under way; what it made stays.
#[tauri::command]
pub async fn cancel_suggest_all<R: Runtime>(
    app: AppHandle<R>,
) -> Result<TagSuggestDto, DomainError> {
    if let Some(s) = app.try_state::<SuggestState>() {
        s.stop.store(true, Ordering::SeqCst);
    }
    get_tag_suggest(app).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use vv_persistence::library::bookmark;
    use vv_persistence::library::FixedClock;
    use vv_persistence::{Database, OpenOptions};

    #[test]
    fn accepting_adds_an_undoable_ai_tag_and_rejecting_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = Database::open(&dir.path().join("l.sqlite"), &OpenOptions::default()).unwrap();
        let clock = FixedClock::new(1_794_000_000_000);
        let now = clock.now_ms();
        {
            let c = db.writer();
            let item = match bookmark::save_bookmark(c, &clock, "https://example.com/v")
                .unwrap()
                .outcome
            {
                bookmark::SaveOutcome::Created(it) => it.id,
                bookmark::SaveOutcome::Existing { item, .. } => item.id,
            };
            let ps = proposals::save_run(
                c,
                &item,
                "m",
                "http://localhost/v1",
                &"0".repeat(32),
                &["uçak".into(), "jet".into()],
                now,
            )
            .unwrap();
            let r = decide(c, "s1", now, &ps[0].id, true, Some("Uçaklar")).unwrap();
            assert_eq!(r.proposals.len(), 1);
            assert_eq!(r.op.as_ref().unwrap().changed, 1);
            assert!(r.op.unwrap().undo.is_some());
            let origin: String = c
                .query_row(
                    "SELECT lt.origin FROM library_item_tag lt JOIN tag t ON t.id = lt.tag_id \
                     WHERE lt.item_id = ?1 AND t.name = 'Uçaklar'",
                    [&item],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(origin, "ai_accepted");
            let r = decide(c, "s1", now, &ps[1].id, false, None).unwrap();
            assert!(r.proposals.is_empty() && r.op.is_none());
            assert_eq!(organize::item_tags(c, &item).unwrap().len(), 1);
            // Decided twice: refused.
            assert!(decide(c, "s1", now, &ps[1].id, true, None).is_err());
        }
    }

    #[test]
    fn deciding_many_tags_every_item_once_and_is_all_or_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = Database::open(&dir.path().join("l.sqlite"), &OpenOptions::default()).unwrap();
        let clock = FixedClock::new(1_794_000_000_000);
        let now = clock.now_ms();
        let w = db.writer();
        let mut item = |url: &str| match bookmark::save_bookmark(w, &clock, url).unwrap().outcome {
            bookmark::SaveOutcome::Created(it) => it.id,
            bookmark::SaveOutcome::Existing { item, .. } => item.id,
        };
        let a = item("https://example.com/a");
        let b = item("https://example.com/b");
        let c: &Connection = w;
        let run = |id: &str, v: &[&str]| {
            proposals::save_run(
                c,
                id,
                "m",
                "http://localhost/v1",
                &"0".repeat(32),
                &v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
                now,
            )
            .unwrap()
        };
        let pa = run(&a, &["uçak", "deniz"]);
        let pb = run(&b, &["Uçak"]);
        let ids = vec![pa[0].id.clone(), pb[0].id.clone()];
        // One already decided: nothing changes.
        decide(c, "s1", now, &pa[1].id, false, None).unwrap();
        assert!(decide_many(
            c,
            "s1",
            now,
            &[ids[0].clone(), pa[1].id.clone()],
            true,
            None
        )
        .is_err());
        assert!(organize::item_tags(c, &a).unwrap().is_empty());

        let r = decide_many(c, "s1", now, &ids, true, Some("Uçaklar")).unwrap();
        assert_eq!(r.decided, 2);
        let op = r.op.unwrap();
        assert_eq!(op.changed, 2);
        assert!(op.undo.is_some());
        for it in [&a, &b] {
            let origin: String = c
                .query_row(
                    "SELECT lt.origin FROM library_item_tag lt JOIN tag t ON t.id = lt.tag_id \
                     WHERE lt.item_id = ?1 AND t.name = 'Uçaklar'",
                    [it],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(origin, "ai_accepted");
        }
        assert!(proposals::pending_groups(c).unwrap().is_empty());
        // Rejecting many changes no tag.
        let pc = run(&a, &["gemi"]);
        let r = decide_many(c, "s1", now, &[pc[0].id.clone()], false, None).unwrap();
        assert_eq!(r.decided, 1);
        assert!(r.op.is_none());
        assert_eq!(organize::item_tags(c, &a).unwrap().len(), 1);
    }
}
