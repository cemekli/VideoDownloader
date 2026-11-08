//! Library screen commands (STATE.md F1.7; SPEC §9.2, §9.3, §16.1, §18).
//!
//! `list_items` is the one query behind the grid, the list, the sidebar
//! views, the filter chips and the search box. `get_item` feeds the
//! inspector. `import_media` opens the native file dialog in Rust, so no path
//! ever comes from the webview (ADR-0013).

use crate::library::{internal, ImportSummary, LibraryState};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};
use vv_domain::error::{DomainError, ErrorCode};
use vv_persistence::library::repo::{self, LibraryItem};
use vv_persistence::library::search::{
    self, ArchiveFilter, Field, Filters, QueryNotice, SearchQuery, Snippet, Sort, SortField,
};
use vv_persistence::library::view::{self, AssetView, LocalState};
use vv_persistence::library::{Clock, ItemKind, SourcePlatform, SystemClock};
use vv_persistence::rusqlite::Connection;

/// Window of the "Recently Added" view.
pub const RECENT_DAYS: i64 = 30;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// Thumbnail URL scheme (see `thumb_protocol`).
pub const THUMB_SCHEME: &str = "vvthumb";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LibraryView {
    #[default]
    All,
    Favorites,
    Recent,
    Archive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SortKey {
    Relevance,
    #[default]
    Added,
    Title,
    Duration,
    Published,
}

/// What the UI asks for. Unknown enum values are refused by deserialization.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListQuery {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub view: LibraryView,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub kinds: Vec<String>,
    #[serde(default)]
    pub sort: SortKey,
    #[serde(default = "yes")]
    pub descending: bool,
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "page")]
    pub limit: u32,
}

fn yes() -> bool {
    true
}
fn page() -> u32 {
    100
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetPartDto {
    pub text: String,
    pub highlight: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetDto {
    pub field: &'static str,
    pub parts: Vec<SnippetPartDto>,
    pub cut_start: bool,
    pub cut_end: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryItemDto {
    pub id: String,
    pub title: String,
    pub platform: &'static str,
    pub kind: &'static str,
    pub local_state: &'static str,
    /// Unknown is `null`, never 0 (SPEC §11.1).
    pub duration_ms: Option<i64>,
    pub tags: Vec<String>,
    pub thumbnail_url: Option<String>,
    pub added_at: i64,
    pub is_favorite: bool,
    pub snippet: Option<SnippetDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemPage {
    pub total: u64,
    pub offset: u32,
    pub items: Vec<LibraryItemDto>,
    /// `no_searchable_terms` when the text had nothing to search for (SEA-04).
    pub notice: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetDto {
    pub id: String,
    pub file_name: String,
    pub root_label: String,
    pub root_status: &'static str,
    pub integrity_state: &'static str,
    pub size_bytes: Option<i64>,
    pub container: Option<String>,
    pub video_codec: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDetail {
    pub item: LibraryItemDto,
    pub canonical_url: Option<String>,
    pub creator: Option<String>,
    pub description: Option<String>,
    pub notes: String,
    pub published_at: Option<i64>,
    pub published_date: Option<String>,
    pub metadata_status: &'static str,
    pub updated_at: i64,
    pub archived: bool,
    pub assets: Vec<AssetDto>,
}

fn field_name(f: Field) -> &'static str {
    match f {
        Field::Title => "title",
        Field::Description => "description",
        Field::Creator => "creator",
        Field::Notes => "notes",
        Field::Tags => "tags",
    }
}

fn snippet_dto(s: &Snippet) -> SnippetDto {
    SnippetDto {
        field: field_name(s.field),
        parts: s
            .parts
            .iter()
            .map(|p| SnippetPartDto {
                text: p.text.clone(),
                highlight: p.highlight,
            })
            .collect(),
        cut_start: s.cut_start,
        cut_end: s.cut_end,
    }
}

/// `http://vvthumb.localhost/<id>` on Windows, `vvthumb://localhost/<id>`
/// elsewhere (how Tauri exposes custom schemes per platform).
pub fn thumbnail_url(asset_id: &str) -> String {
    if cfg!(windows) {
        format!("http://{THUMB_SCHEME}.localhost/{asset_id}")
    } else {
        format!("{THUMB_SCHEME}://localhost/{asset_id}")
    }
}

fn item_dto(it: &LibraryItem, state: LocalState, snippet: Option<SnippetDto>) -> LibraryItemDto {
    LibraryItemDto {
        id: it.id.clone(),
        title: it.display_title(),
        platform: it.source_platform.as_str(),
        kind: it.kind.as_str(),
        local_state: state.as_str(),
        duration_ms: it.duration_ms,
        tags: Vec::new(),
        thumbnail_url: it.thumbnail_asset_id.as_deref().map(thumbnail_url),
        added_at: it.added_at,
        is_favorite: it.is_favorite,
        snippet,
    }
}

fn parse_all<T: Copy>(
    values: &[String],
    parse: fn(&str) -> Option<T>,
    what: &str,
) -> Result<Vec<T>, DomainError> {
    values
        .iter()
        .map(|v| parse(v).ok_or_else(|| DomainError::new(ErrorCode::InternalError, what)))
        .collect()
}

pub fn to_search_query(q: &ListQuery, now_ms: i64) -> Result<SearchQuery, DomainError> {
    let mut filters = Filters {
        platforms: parse_all(&q.platforms, SourcePlatform::parse, "list-bad-platform")?,
        kinds: parse_all(&q.kinds, ItemKind::parse, "list-bad-kind")?,
        ..Default::default()
    };
    match q.view {
        LibraryView::All => {}
        LibraryView::Favorites => filters.favorite_only = true,
        LibraryView::Recent => filters.added_since = Some(now_ms - RECENT_DAYS * DAY_MS),
        LibraryView::Archive => filters.archived = ArchiveFilter::Only,
    }
    let field = match q.sort {
        SortKey::Relevance => SortField::Relevance,
        SortKey::Added => SortField::AddedAt,
        SortKey::Title => SortField::Title,
        SortKey::Duration => SortField::Duration,
        SortKey::Published => SortField::PublishedAt,
    };
    Ok(SearchQuery {
        text: q.text.clone(),
        prefix_last: true,
        filters,
        sort: Sort {
            field,
            descending: q.descending,
        },
        limit: q.limit.clamp(1, search::MAX_PAGE),
        offset: q.offset,
    })
}

pub fn list_page(c: &Connection, q: &ListQuery, now_ms: i64) -> Result<ItemPage, DomainError> {
    let sq = to_search_query(q, now_ms)?;
    let page = search::search(c, &sq).map_err(|_| internal("list-search"))?;
    let ids: Vec<&str> = page.hits.iter().map(|h| h.item.id.as_str()).collect();
    let states = view::local_states(c, &ids).map_err(|_| internal("list-states"))?;
    let items = page
        .hits
        .iter()
        .map(|h| {
            item_dto(
                &h.item,
                states
                    .get(&h.item.id)
                    .copied()
                    .unwrap_or(LocalState::BookmarkOnly),
                h.snippet.as_ref().map(snippet_dto),
            )
        })
        .collect();
    Ok(ItemPage {
        total: page.total,
        offset: sq.offset,
        items,
        notice: page.notice.map(|n| match n {
            QueryNotice::NoSearchableTerms => "no_searchable_terms",
        }),
    })
}

fn asset_dto(a: AssetView) -> AssetDto {
    AssetDto {
        id: a.id,
        file_name: a.file_name,
        root_label: a.root_label,
        root_status: a.root_status.as_str(),
        integrity_state: a.integrity_state.as_str(),
        size_bytes: a.size_bytes,
        container: a.container,
        video_codec: a.video_codec,
        width: a.width,
        height: a.height,
        duration_ms: a.duration_ms,
    }
}

pub fn item_detail(c: &Connection, id: &str) -> Result<ItemDetail, DomainError> {
    let it = repo::get_item(c, id)
        .map_err(|_| internal("item-read"))?
        .filter(|it| it.removed_at.is_none())
        .ok_or_else(|| DomainError::new(ErrorCode::InternalError, "item-not-found"))?;
    let state = view::local_states(c, &[id])
        .map_err(|_| internal("item-state"))?
        .remove(id)
        .unwrap_or(LocalState::BookmarkOnly);
    let assets = view::asset_views(c, id).map_err(|_| internal("item-assets"))?;
    let o = &it.user_overrides;
    Ok(ItemDetail {
        item: item_dto(&it, state, None),
        canonical_url: it.canonical_url.clone(),
        creator: o.creator.clone().or_else(|| it.creator.clone()),
        description: o.description.clone().or_else(|| it.description.clone()),
        notes: it.user_notes.clone(),
        published_at: it.published_at,
        published_date: it.published_date.clone(),
        metadata_status: it.metadata_status.as_str(),
        updated_at: it.updated_at,
        archived: it.archived_at.is_some(),
        assets: assets.into_iter().map(asset_dto).collect(),
    })
}

async fn blocking<R: Runtime, T: Send + 'static>(
    app: AppHandle<R>,
    f: impl FnOnce(&LibraryState) -> Result<T, DomainError> + Send + 'static,
) -> Result<T, DomainError> {
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<LibraryState>()))
        .await
        .map_err(|_| internal("task-join"))?
}

/// One page of the library for the current view, filters, sort and search.
#[tauri::command]
pub async fn list_items<R: Runtime>(
    app: AppHandle<R>,
    query: ListQuery,
) -> Result<ItemPage, DomainError> {
    blocking(app, move |st| {
        let c = st.get()?.reader()?;
        list_page(&c, &query, SystemClock.now_ms())
    })
    .await
}

/// Everything the inspector shows about one item.
#[tauri::command]
pub async fn get_item<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<ItemDetail, DomainError> {
    blocking(app, move |st| {
        let c = st.get()?.reader()?;
        item_detail(&c, &id)
    })
    .await
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportSource {
    Files,
    Folder,
}

/// Opens the native dialog and imports what the user picked (IMP-01).
#[tauri::command]
pub async fn import_media<R: Runtime>(
    app: AppHandle<R>,
    source: ImportSource,
) -> Result<ImportSummary, DomainError> {
    use tauri_plugin_dialog::DialogExt;
    let lib_ready = app.state::<LibraryState>().get()?.has_tools();
    if !lib_ready {
        return Err(DomainError::new(ErrorCode::ToolMissing, "import-no-tools"));
    }
    blocking(app.clone(), move |st| {
        let mut dialog = app.dialog().file();
        if let Some(w) = app.get_webview_window("main") {
            dialog = dialog.set_parent(&w);
        }
        let picked: Vec<std::path::PathBuf> = match source {
            ImportSource::Files => dialog
                .add_filter("Video and audio", vv_import::fsutil::MEDIA_EXTENSIONS)
                .blocking_pick_files()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|p| p.into_path().ok())
                .collect(),
            ImportSource::Folder => dialog
                .blocking_pick_folder()
                .and_then(|p| p.into_path().ok())
                .into_iter()
                .collect(),
        };
        if picked.is_empty() {
            return Ok(ImportSummary::default());
        }
        st.get()?
            .import(&picked, true, crate::library::cancel_flag(st))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use vv_persistence::library::repo::{MediaAsset, StorageRoot};
    use vv_persistence::library::*;
    use vv_persistence::{Database, OpenOptions};

    fn db(dir: &std::path::Path) -> Database {
        Database::open(&dir.join("library.sqlite"), &OpenOptions::default()).unwrap()
    }

    fn query(v: serde_json::Value) -> ListQuery {
        serde_json::from_value(v).unwrap()
    }

    fn local(c: &Connection, title: &str, added: i64, status: StorageRootStatus) -> LibraryItem {
        let mut it = LibraryItem::new(ItemKind::LocalVideo, SourcePlatform::Local, added);
        it.title = Some(title.into());
        repo::insert_item(c, &it).unwrap();
        let root = StorageRoot {
            id: new_id(),
            kind: StorageRootKind::ExternalImport,
            label: "Videos".into(),
            path: format!("/vv/{}", new_id()),
            volume_id: None,
            status,
            created_at: 1,
            last_verified_at: None,
        };
        repo::insert_storage_root(c, &root).unwrap();
        let mut a = MediaAsset::new(
            &it.id,
            AssetRole::Original,
            MediaKind::Video,
            StorageKind::ExternalReference,
            &root.id,
            &format!("sub/{title}.mp4"),
            added,
        );
        a.acquisition_status = AcquisitionStatus::Ready;
        a.integrity_state = IntegrityState::Verified;
        a.video_codec = Some("h264".into());
        repo::insert_media_asset(c, &a).unwrap();
        it
    }

    #[test]
    fn defaults_are_newest_first_page_of_100() {
        let q = query(serde_json::json!({}));
        let sq = to_search_query(&q, 0).unwrap();
        assert_eq!(sq.limit, 100);
        assert_eq!(sq.sort, Sort::default());
        assert_eq!(sq.filters, Filters::default());
        let big = to_search_query(&query(serde_json::json!({ "limit": 100000 })), 0).unwrap();
        assert_eq!(big.limit, search::MAX_PAGE);
    }

    #[test]
    fn views_map_to_filters() {
        let now = 100 * DAY_MS;
        let f = |v: &str| {
            to_search_query(&query(serde_json::json!({ "view": v })), now)
                .unwrap()
                .filters
        };
        assert!(f("favorites").favorite_only);
        assert_eq!(f("recent").added_since, Some(now - RECENT_DAYS * DAY_MS));
        assert_eq!(f("archive").archived, ArchiveFilter::Only);
        assert_eq!(f("all"), Filters::default());
    }

    #[test]
    fn page_carries_local_state_snippet_and_notice() {
        let d = tempfile::tempdir().unwrap();
        let mut db = db(d.path());
        let c = db.writer();
        let ready = local(c, "Deniz", 10, StorageRootStatus::Available);
        let away = local(c, "Dağ", 20, StorageRootStatus::Disconnected);
        let page = list_page(c, &query(serde_json::json!({})), 0).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.items[0].id, away.id);
        assert_eq!(page.items[0].local_state, "disconnected");
        assert_eq!(page.items[1].local_state, "local_ready");
        assert!(page.items.iter().all(|i| i.snippet.is_none()));

        let hit = list_page(c, &query(serde_json::json!({ "text": "deniz" })), 0).unwrap();
        assert_eq!(hit.items.len(), 1);
        assert_eq!(hit.items[0].id, ready.id);
        assert_eq!(hit.items[0].snippet.as_ref().unwrap().field, "title");

        let none = list_page(c, &query(serde_json::json!({ "text": "!!" })), 0).unwrap();
        assert_eq!(none.notice, Some("no_searchable_terms"));
        assert_eq!(none.total, 2);

        let only_local =
            list_page(c, &query(serde_json::json!({ "kinds": ["bookmark"] })), 0).unwrap();
        assert_eq!(only_local.total, 0);
    }

    #[test]
    fn detail_lists_media_without_paths() {
        let d = tempfile::tempdir().unwrap();
        let mut db = db(d.path());
        let c = db.writer();
        let it = local(c, "Deniz", 10, StorageRootStatus::Available);
        let det = item_detail(c, &it.id).unwrap();
        assert_eq!(det.item.local_state, "local_ready");
        assert_eq!(det.assets.len(), 1);
        assert_eq!(det.assets[0].file_name, "Deniz.mp4");
        assert_eq!(det.assets[0].video_codec.as_deref(), Some("h264"));
        let json = serde_json::to_string(&det).unwrap();
        assert!(!json.contains("/vv/"), "detail leaks a root path: {json}");

        repo::set_removed(c, &it.id, it.revision, Some(11), 11).unwrap();
        assert!(item_detail(c, &it.id).is_err());
    }

    #[test]
    fn thumbnail_urls_use_the_platform_form() {
        let u = thumbnail_url("abc");
        if cfg!(windows) {
            assert_eq!(u, "http://vvthumb.localhost/abc");
        } else {
            assert_eq!(u, "vvthumb://localhost/abc");
        }
    }
}
