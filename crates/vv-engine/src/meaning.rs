//! The search box's query by meaning (SPEC §16.3, SEA-07, AC-40; STATE.md
//! F7.2; ADR-0036).
//!
//! - A query must be embedded by the API and model the active index was
//!   made with ([`prepare`] checks it); otherwise search by meaning is off
//!   and the word search runs alone, with the reason ([`Off`]).
//! - [`QueryEmbedder`] keeps the last [`CACHE`] query vectors, so paging
//!   through a result or typing back to an earlier text does not ask the
//!   API again.
//! - The webview never sees a vector; it gets the fused result.

use crate::embed::{self, Api, EmbedError};
use std::collections::VecDeque;
use std::sync::Mutex;
use vv_persistence::rusqlite::Connection;
use vv_persistence::vectors::{self, Generation};
use vv_secrets::SecretStore;

/// Query vectors kept.
pub const CACHE: usize = 64;

/// Why the search ran without meaning (the screen says it in words).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Off {
    /// No embedding API is set.
    NoApi,
    /// No complete index yet (a first build may be under way).
    NoIndex,
    /// The index was made with another API or model than the settings name.
    OtherModel,
    /// Offline Mode, and the API is not on this PC.
    Offline,
    /// The API did not answer; the message is for the log only.
    Failed(String),
}

impl Off {
    pub fn as_str(&self) -> &'static str {
        match self {
            Off::NoApi => "no_api",
            Off::NoIndex => "no_index",
            Off::OtherModel => "other_model",
            Off::Offline => "offline",
            Off::Failed(_) => "failed",
        }
    }
}

/// What a query by meaning needs, read from the library in one go so no
/// connection is held while the API answers.
#[derive(Debug, Clone)]
pub struct Plan {
    pub generation: Generation,
    pub api: Api,
}

/// Can a query by meaning run now, and with what.
pub fn prepare(c: &Connection, secrets: &dyn SecretStore, offline: bool) -> Result<Plan, Off> {
    let cfg = embed::configured(c).ok_or(Off::NoApi)?;
    let generation = vectors::active(c).ok().flatten().ok_or(Off::NoIndex)?;
    if embed::producer(&cfg) != generation.producer {
        return Err(Off::OtherModel);
    }
    let api = embed::Api::from_settings(&cfg, secrets, offline).map_err(|e| match e {
        EmbedError::Failed(vv_domain::error::ErrorCode::OfflineMode, _) => Off::Offline,
        EmbedError::Failed(_, m) => Off::Failed(m),
        EmbedError::Stopped => Off::Failed(String::new()),
    })?;
    Ok(Plan { generation, api })
}

#[derive(Default)]
struct State {
    /// (generation id, text) → vector.
    cache: VecDeque<((String, String), Vec<f32>)>,
    requests: usize,
}

/// The query vector cache. Queries may run side by side.
#[derive(Default)]
pub struct QueryEmbedder {
    state: Mutex<State>,
}

impl QueryEmbedder {
    pub fn new() -> Self {
        Self::default()
    }

    /// The vector of `text` for `plan`'s index.
    pub fn embed(&self, plan: &Plan, text: &str) -> Result<Vec<f32>, EmbedError> {
        let key = (plan.generation.id.clone(), text.to_owned());
        {
            let mut st = self.state.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(i) = st.cache.iter().position(|(k, _)| *k == key) {
                let hit = st.cache.remove(i).expect("position is in range");
                let v = hit.1.clone();
                st.cache.push_front(hit);
                return Ok(v);
            }
            st.requests += 1;
        }
        let v = plan
            .api
            .embed(&[text], Some(plan.generation.producer.dims), || false)?
            .pop()
            .ok_or_else(|| {
                EmbedError::Failed(
                    vv_domain::error::ErrorCode::ExtractorFailed,
                    "no vector".into(),
                )
            })?;
        let mut st = self.state.lock().unwrap_or_else(|p| p.into_inner());
        st.cache.push_front((key, v.clone()));
        st.cache.truncate(CACHE);
        Ok(v)
    }

    /// Forgets the cached vectors (the API settings changed).
    pub fn clear(&self) {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .cache
            .clear();
    }

    /// Requests sent to the API so far (tests).
    pub fn requests(&self) -> usize {
        self.state.lock().map(|s| s.requests).unwrap_or(0)
    }
}
