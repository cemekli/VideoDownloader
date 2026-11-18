//! The search box with search by meaning (STATE.md F7.2; SPEC SEA-07,
//! §16.3, AC-40; ADR-0036).
//!
//! `list_items` answers the word search at once. When search by meaning
//! applies (a text, the Relevance sort) the answer says whether it can run
//! (`meaning: "available"`) or why not; the UI then asks again with
//! `meaning: true` and gets the fused result. The query is embedded by the
//! user's embedding API (cached, [`vv_engine::meaning::QueryEmbedder`]); no
//! library connection is held while it answers. A failure there never
//! fails the search: the word result comes back with `meaning: "failed"`.

use crate::library::LibraryState;
use crate::library_commands::{
    meaning_applies, page_dto, smart_rules, to_search_query_with, ItemPage, ListQuery,
};
use std::sync::Arc;
use tauri::{AppHandle, Manager, Runtime};
use vv_domain::error::DomainError;
use vv_engine::meaning::{self, QueryEmbedder};
use vv_persistence::library::hybrid::{self, MeaningQuery};
use vv_persistence::library::search;
use vv_persistence::library::{Clock, SystemClock};
use vv_persistence::rusqlite::Connection;

#[derive(Default)]
pub struct MeaningState {
    embedder: Arc<QueryEmbedder>,
}

pub fn embedder<R: Runtime>(app: &AppHandle<R>) -> Option<Arc<QueryEmbedder>> {
    app.try_state::<MeaningState>().map(|s| s.embedder.clone())
}

/// Forgets cached query vectors (the API settings changed).
pub fn clear<R: Runtime>(app: &AppHandle<R>) {
    if let Some(s) = app.try_state::<MeaningState>() {
        s.embedder.clear();
    }
}

/// One page of `list_items`, hybrid when asked and possible.
pub fn list(
    st: &LibraryState,
    q: &ListQuery,
    embedder: Option<Arc<QueryEmbedder>>,
) -> Result<ItemPage, DomainError> {
    let lib = st.get()?;
    let now = SystemClock.now_ms();
    let (sq, plan) = {
        let c = lib.reader()?;
        let sq = to_search_query_with(q, now, smart_rules(&c, q)?.as_ref())?;
        let words = |c: &Connection, m| {
            let page =
                search::search(c, &sq).map_err(|_| crate::library::internal("list-search"))?;
            page_dto(c, &sq, page, m)
        };
        if !meaning_applies(&sq) {
            return words(&c, None);
        }
        let offline = vv_persistence::settings::offline_mode(&c).unwrap_or(false);
        let secrets = crate::search_index_commands::secrets(st)?;
        match meaning::prepare(&c, secrets.as_ref(), offline) {
            Err(off) => return words(&c, Some(off.as_str())),
            Ok(_) if !q.meaning || embedder.is_none() => return words(&c, Some("available")),
            Ok(p) => (sq.clone(), p),
        }
    };
    let embedder = embedder.expect("checked above");
    let t = crate::diag::now_ms();
    let vector = embedder.embed(&plan, &sq.text);
    let c = lib.reader()?;
    match vector {
        Ok(v) => {
            crate::diag::log(&format!(
                "query embedded in {} ms",
                crate::diag::now_ms() - t
            ));
            let page = hybrid::search_hybrid(
                &c,
                &sq,
                Some(&MeaningQuery {
                    generation: &plan.generation,
                    vector: &v,
                }),
            )
            .map_err(|_| crate::library::internal("list-hybrid"))?;
            page_dto(&c, &sq, page, Some("used"))
        }
        Err(e) => {
            crate::diag::log(&format!(
                "query embedding failed: {:?}",
                vv_engine::embed::error_code(&e)
            ));
            let page =
                search::search(&c, &sq).map_err(|_| crate::library::internal("list-search"))?;
            page_dto(&c, &sq, page, Some("failed"))
        }
    }
}
