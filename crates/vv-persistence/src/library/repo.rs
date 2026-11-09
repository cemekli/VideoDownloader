//! Row types and repository functions for the 0002 tables.

use super::*;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

/// `user_overrides` (§11.2): versioned, typed; a set field wins over the
/// platform value, clearing it falls back to the platform value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserOverrides {
    pub v: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<PublishedOverride>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedOverride {
    /// UTC ms as text for `datetime`, `YYYY-MM-DD` for `date`.
    pub value: String,
    pub precision: String,
}

pub const USER_OVERRIDES_VERSION: u32 = 1;

impl UserOverrides {
    pub fn empty() -> Self {
        UserOverrides {
            v: USER_OVERRIDES_VERSION,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryItem {
    pub id: String,
    pub kind: ItemKind,
    pub canonical_url: Option<String>,
    pub original_url: Option<String>,
    pub normalization_version: u32,
    pub normalization_conflict: bool,
    pub identity_conflict: bool,
    pub source_platform: SourcePlatform,
    pub source_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub creator: Option<String>,
    pub user_overrides: UserOverrides,
    pub published_at: Option<i64>,
    pub published_date: Option<String>,
    pub published_precision: Option<PublishedPrecision>,
    pub added_at: i64,
    pub updated_at: i64,
    pub duration_ms: Option<i64>,
    pub language: Option<String>,
    pub visibility: Visibility,
    pub remote_availability: RemoteAvailability,
    pub access_state: AccessState,
    pub access_profile_id: Option<String>,
    pub access_checked_at: Option<i64>,
    pub metadata_status: MetadataStatus,
    pub thumbnail_asset_id: Option<String>,
    pub user_notes: String,
    pub is_favorite: bool,
    pub archived_at: Option<i64>,
    pub removed_at: Option<i64>,
    pub last_checked_at: Option<i64>,
    pub metadata_provenance: Option<String>,
    pub revision: i64,
}

impl LibraryItem {
    /// A fresh item with every optional field unknown.
    pub fn new(kind: ItemKind, source_platform: SourcePlatform, now_ms: i64) -> Self {
        LibraryItem {
            id: new_id(),
            kind,
            canonical_url: None,
            original_url: None,
            normalization_version: 0,
            normalization_conflict: false,
            identity_conflict: false,
            source_platform,
            source_id: None,
            title: None,
            description: None,
            creator: None,
            user_overrides: UserOverrides::empty(),
            published_at: None,
            published_date: None,
            published_precision: None,
            added_at: now_ms,
            updated_at: now_ms,
            duration_ms: None,
            language: None,
            visibility: Visibility::Unknown,
            remote_availability: RemoteAvailability::Unknown,
            access_state: AccessState::Unknown,
            access_profile_id: None,
            access_checked_at: None,
            metadata_status: MetadataStatus::Pending,
            thumbnail_asset_id: None,
            user_notes: String::new(),
            is_favorite: false,
            archived_at: None,
            removed_at: None,
            last_checked_at: None,
            metadata_provenance: None,
            revision: 1,
        }
    }

    /// What the UI shows as the title: override, then platform value, then a
    /// title derived from the link alone (CAP-04). The derived one is never
    /// stored as if it were platform metadata.
    pub fn display_title(&self) -> String {
        if let Some(t) = self.user_overrides.title.as_ref().or(self.title.as_ref()) {
            return t.clone();
        }
        self.canonical_url
            .as_deref()
            .and_then(|u| vv_domain::url_policy::validate_capture_url(u).ok())
            .and_then(|u| vv_domain::normalize::normalize(&u).ok())
            .map(|n| vv_domain::normalize::derived_title(&n))
            .unwrap_or_default()
    }

    fn from_row(r: &Row<'_>) -> rusqlite::Result<Self> {
        let overrides: String = r.get("user_overrides")?;
        let user_overrides = serde_json::from_str(&overrides).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
        })?;
        Ok(LibraryItem {
            id: r.get("id")?,
            kind: r.get("kind")?,
            canonical_url: r.get("canonical_url")?,
            original_url: r.get("original_url")?,
            normalization_version: r.get("normalization_version")?,
            normalization_conflict: r.get("normalization_conflict")?,
            identity_conflict: r.get("identity_conflict")?,
            source_platform: r.get("source_platform")?,
            source_id: r.get("source_id")?,
            title: r.get("title")?,
            description: r.get("description")?,
            creator: r.get("creator")?,
            user_overrides,
            published_at: r.get("published_at")?,
            published_date: r.get("published_date")?,
            published_precision: r.get("published_precision")?,
            added_at: r.get("added_at")?,
            updated_at: r.get("updated_at")?,
            duration_ms: r.get("duration_ms")?,
            language: r.get("language")?,
            visibility: r.get("visibility")?,
            remote_availability: r.get("remote_availability")?,
            access_state: r.get("access_state")?,
            access_profile_id: r.get("access_profile_id")?,
            access_checked_at: r.get("access_checked_at")?,
            metadata_status: r.get("metadata_status")?,
            thumbnail_asset_id: r.get("thumbnail_asset_id")?,
            user_notes: r.get("user_notes")?,
            is_favorite: r.get("is_favorite")?,
            archived_at: r.get("archived_at")?,
            removed_at: r.get("removed_at")?,
            last_checked_at: r.get("last_checked_at")?,
            metadata_provenance: r.get("metadata_provenance")?,
            revision: r.get("revision")?,
        })
    }
}

const ITEM_COLUMNS: &str = "id, kind, canonical_url, original_url, normalization_version, \
    normalization_conflict, identity_conflict, source_platform, source_id, title, description, \
    creator, user_overrides, published_at, published_date, published_precision, added_at, \
    updated_at, duration_ms, language, visibility, remote_availability, access_state, \
    access_profile_id, access_checked_at, metadata_status, thumbnail_asset_id, user_notes, \
    is_favorite, archived_at, removed_at, last_checked_at, metadata_provenance, revision";

/// Inserts the item and its search document. Call inside a transaction so
/// both land together (CAP-03).
pub fn insert_item(c: &Connection, it: &LibraryItem) -> Result<()> {
    let overrides = serde_json::to_string(&it.user_overrides).expect("overrides serialize");
    c.execute(
        &format!(
            "INSERT INTO library_item ({ITEM_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, \
             ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, \
             ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34)"
        ),
        params![
            it.id,
            it.kind,
            it.canonical_url,
            it.original_url,
            it.normalization_version,
            it.normalization_conflict,
            it.identity_conflict,
            it.source_platform,
            it.source_id,
            it.title,
            it.description,
            it.creator,
            overrides,
            it.published_at,
            it.published_date,
            it.published_precision,
            it.added_at,
            it.updated_at,
            it.duration_ms,
            it.language,
            it.visibility,
            it.remote_availability,
            it.access_state,
            it.access_profile_id,
            it.access_checked_at,
            it.metadata_status,
            it.thumbnail_asset_id,
            it.user_notes,
            it.is_favorite,
            it.archived_at,
            it.removed_at,
            it.last_checked_at,
            it.metadata_provenance,
            it.revision,
        ],
    )?;
    write_search_document(c, it)
}

fn select_item(
    c: &Connection,
    where_sql: &str,
    p: impl rusqlite::Params,
) -> Result<Option<LibraryItem>> {
    Ok(c.query_row(
        &format!("SELECT {ITEM_COLUMNS} FROM library_item WHERE {where_sql}"),
        p,
        LibraryItem::from_row,
    )
    .optional()?)
}

pub fn get_item(c: &Connection, id: &str) -> Result<Option<LibraryItem>> {
    select_item(c, "id = ?1", [id])
}

pub fn item_by_canonical_url(c: &Connection, url: &str) -> Result<Option<LibraryItem>> {
    select_item(c, "canonical_url = ?1", [url])
}

pub fn item_by_source(
    c: &Connection,
    platform: SourcePlatform,
    source_id: &str,
) -> Result<Option<LibraryItem>> {
    select_item(
        c,
        "source_platform = ?1 AND source_id = ?2",
        params![platform, source_id],
    )
}

pub fn item_by_alias(c: &Connection, url: &str) -> Result<Option<LibraryItem>> {
    select_item(
        c,
        "id = (SELECT item_id FROM url_alias WHERE url = ?1)",
        [url],
    )
}

pub fn count_items(c: &Connection) -> Result<i64> {
    Ok(c.query_row("SELECT count(*) FROM library_item", [], |r| r.get(0))?)
}

/// Applies `set_sql` (which must not touch `revision`/`updated_at`) when the
/// stored revision equals `expected`, bumping both. Returns the new row.
fn update_checked(
    c: &Connection,
    id: &str,
    expected: i64,
    now_ms: i64,
    set_sql: &str,
    value: &dyn rusqlite::ToSql,
) -> Result<LibraryItem> {
    let n = c.execute(
        &format!(
            "UPDATE library_item SET {set_sql}, revision = revision + 1, updated_at = ?2 \
             WHERE id = ?3 AND revision = ?4"
        ),
        params![value, now_ms, id, expected],
    )?;
    if n == 0 {
        let current: Option<i64> = c
            .query_row(
                "SELECT revision FROM library_item WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        return Err(match current {
            Some(current) => LibraryError::RevisionConflict { current },
            None => LibraryError::NotFound,
        });
    }
    let it = get_item(c, id)?.ok_or(LibraryError::NotFound)?;
    write_search_document(c, &it)?;
    Ok(it)
}

pub fn set_notes(
    c: &Connection,
    id: &str,
    expected_revision: i64,
    notes: &str,
    now_ms: i64,
) -> Result<LibraryItem> {
    update_checked(c, id, expected_revision, now_ms, "user_notes = ?1", &notes)
}

pub fn set_favorite(
    c: &Connection,
    id: &str,
    expected_revision: i64,
    favorite: bool,
    now_ms: i64,
) -> Result<LibraryItem> {
    update_checked(
        c,
        id,
        expected_revision,
        now_ms,
        "is_favorite = ?1",
        &favorite,
    )
}

/// Soft delete (`Some(at)`) or restore (`None`), §7.3.
pub fn set_removed(
    c: &Connection,
    id: &str,
    expected_revision: i64,
    removed_at: Option<i64>,
    now_ms: i64,
) -> Result<LibraryItem> {
    update_checked(
        c,
        id,
        expected_revision,
        now_ms,
        "removed_at = ?1",
        &removed_at,
    )
}

pub fn set_user_overrides(
    c: &Connection,
    id: &str,
    expected_revision: i64,
    o: &UserOverrides,
    now_ms: i64,
) -> Result<LibraryItem> {
    if o.v != USER_OVERRIDES_VERSION {
        return Err(LibraryError::Invalid("unknown user_overrides version"));
    }
    let json = serde_json::to_string(o).expect("overrides serialize");
    update_checked(
        c,
        id,
        expected_revision,
        now_ms,
        "user_overrides = ?1",
        &json,
    )
}

// --- SearchDocument (§11.5) ------------------------------------------------

/// Text of the search document for an item, as stored in `search_document`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchText {
    pub title: String,
    pub description: String,
    pub creator: String,
    pub notes: String,
    pub tags: String,
}

pub fn search_text(it: &LibraryItem) -> SearchText {
    let o = &it.user_overrides;
    SearchText {
        title: it.display_title(),
        description: o
            .description
            .clone()
            .or(it.description.clone())
            .unwrap_or_default(),
        creator: o.creator.clone().or(it.creator.clone()).unwrap_or_default(),
        notes: it.user_notes.clone(),
        // Filled from `library_item_tag` by `write_search_document`.
        tags: String::new(),
    }
}

/// Replaces the item's search document. Same transaction as the item write.
/// Stored text goes through [`super::search::index_text`] (ADR-0012).
pub fn write_search_document(c: &Connection, it: &LibraryItem) -> Result<()> {
    let seq: i64 = c.query_row(
        "SELECT seq FROM library_item WHERE id = ?1",
        [&it.id],
        |r| r.get(0),
    )?;
    let mut t = search_text(it);
    t.tags = tag_text(c, &it.id)?;
    let f = super::search::index_text;
    c.execute("DELETE FROM search_document WHERE rowid = ?1", [seq])?;
    c.execute(
        "INSERT INTO search_document (rowid, title, description, creator, notes, tags) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            seq,
            f(&t.title),
            f(&t.description),
            f(&t.creator),
            f(&t.notes),
            f(&t.tags)
        ],
    )?;
    Ok(())
}

/// The item's tag names, space separated, in name order (search document).
pub fn tag_text(c: &Connection, item_id: &str) -> Result<String> {
    let mut s = c.prepare_cached(
        "SELECT t.name FROM library_item_tag lt JOIN tag t ON t.id = lt.tag_id \
         WHERE lt.item_id = ?1 ORDER BY t.normalized_name",
    )?;
    let names = s
        .query_map([item_id], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(names.join(" "))
}

/// Rewrites the search documents of `ids` (after tag changes, ORG-09).
pub fn reindex_items(c: &Connection, ids: &[String]) -> Result<()> {
    for id in ids {
        if let Some(it) = get_item(c, id)? {
            write_search_document(c, &it)?;
        }
    }
    Ok(())
}

/// Rebuilds every search document from the authoritative rows (§11.7: the
/// index is derived data). Returns how many were written.
pub fn rebuild_search_index(c: &Connection) -> Result<usize> {
    c.execute("DELETE FROM search_document", [])?;
    let ids: Vec<String> = {
        let mut s = c.prepare("SELECT id FROM library_item ORDER BY seq")?;
        let v = s
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        v
    };
    for id in &ids {
        let it = get_item(c, id)?.ok_or(LibraryError::NotFound)?;
        write_search_document(c, &it)?;
    }
    Ok(ids.len())
}

pub fn get_search_document(c: &Connection, item_id: &str) -> Result<Option<SearchText>> {
    Ok(c.query_row(
        "SELECT d.title, d.description, d.creator, d.notes, d.tags FROM search_document d \
         JOIN library_item i ON i.seq = d.rowid WHERE i.id = ?1",
        [item_id],
        |r| {
            Ok(SearchText {
                title: r.get(0)?,
                description: r.get(1)?,
                creator: r.get(2)?,
                notes: r.get(3)?,
                tags: r.get(4)?,
            })
        },
    )
    .optional()?)
}

/// Raw FTS match over visible items, ids in rank order. The safe query
/// builder and ranking weights are F1.6; this exists for tests and is fed a
/// literal phrase only.
pub fn search_phrase_ids(c: &Connection, phrase: &str) -> Result<Vec<String>> {
    let q = format!(
        "\"{}\"",
        super::search::index_text(phrase).replace('"', "\"\"")
    );
    let mut s = c.prepare(
        "SELECT i.id FROM search_document d JOIN library_item i ON i.seq = d.rowid \
         WHERE search_document MATCH ?1 AND i.removed_at IS NULL ORDER BY bm25(search_document), i.seq",
    )?;
    let ids = s
        .query_map([q], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(ids)
}

// --- StorageRoot (§11.6) ----------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageRoot {
    pub id: String,
    pub kind: StorageRootKind,
    pub label: String,
    pub path: String,
    pub volume_id: Option<String>,
    pub status: StorageRootStatus,
    pub created_at: i64,
    pub last_verified_at: Option<i64>,
}

pub fn insert_storage_root(c: &Connection, r: &StorageRoot) -> Result<()> {
    c.execute(
        "INSERT INTO storage_root (id, kind, label, path, volume_id, status, created_at, last_verified_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![r.id, r.kind, r.label, r.path, r.volume_id, r.status, r.created_at, r.last_verified_at],
    )?;
    Ok(())
}

pub fn get_storage_root(c: &Connection, id: &str) -> Result<Option<StorageRoot>> {
    Ok(c.query_row(
        "SELECT id, kind, label, path, volume_id, status, created_at, last_verified_at \
         FROM storage_root WHERE id = ?1",
        [id],
        |r| {
            Ok(StorageRoot {
                id: r.get(0)?,
                kind: r.get(1)?,
                label: r.get(2)?,
                path: r.get(3)?,
                volume_id: r.get(4)?,
                status: r.get(5)?,
                created_at: r.get(6)?,
                last_verified_at: r.get(7)?,
            })
        },
    )
    .optional()?)
}

pub fn set_storage_root_status(
    c: &Connection,
    id: &str,
    status: StorageRootStatus,
    verified_at: Option<i64>,
) -> Result<()> {
    let n = c.execute(
        "UPDATE storage_root SET status = ?1, last_verified_at = COALESCE(?2, last_verified_at) WHERE id = ?3",
        params![status, verified_at, id],
    )?;
    if n == 0 {
        return Err(LibraryError::NotFound);
    }
    Ok(())
}

// --- MediaAsset (§11.3) -----------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaAsset {
    pub id: String,
    pub library_item_id: String,
    pub role: AssetRole,
    pub media_kind: MediaKind,
    pub storage_kind: StorageKind,
    pub storage_root_id: String,
    /// Relative to the root; never absolute (the schema refuses it).
    pub storage_ref: String,
    pub content_hash: Option<String>,
    pub hash_algorithm: Option<String>,
    pub size_bytes: Option<i64>,
    pub mime_type: Option<String>,
    pub container: Option<String>,
    pub video_codec: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub audio_tracks: Option<String>,
    pub duration_ms: Option<i64>,
    pub acquired_at: Option<i64>,
    pub acquisition_status: AcquisitionStatus,
    pub integrity_state: IntegrityState,
    pub source_metadata: Option<String>,
    pub source_fingerprint: Option<String>,
    pub source_asset_id: Option<String>,
    pub derivation: Option<String>,
    pub created_at: i64,
}

impl MediaAsset {
    pub fn new(
        item_id: &str,
        role: AssetRole,
        media_kind: MediaKind,
        storage_kind: StorageKind,
        root_id: &str,
        storage_ref: &str,
        now_ms: i64,
    ) -> Self {
        MediaAsset {
            id: new_id(),
            library_item_id: item_id.to_owned(),
            role,
            media_kind,
            storage_kind,
            storage_root_id: root_id.to_owned(),
            storage_ref: storage_ref.to_owned(),
            content_hash: None,
            hash_algorithm: None,
            size_bytes: None,
            mime_type: None,
            container: None,
            video_codec: None,
            width: None,
            height: None,
            audio_tracks: None,
            duration_ms: None,
            acquired_at: None,
            acquisition_status: AcquisitionStatus::Pending,
            integrity_state: IntegrityState::Unknown,
            source_metadata: None,
            source_fingerprint: None,
            source_asset_id: None,
            derivation: None,
            created_at: now_ms,
        }
    }
}

const ASSET_COLUMNS: &str = "id, library_item_id, role, media_kind, storage_kind, storage_root_id, \
    storage_ref, content_hash, hash_algorithm, size_bytes, mime_type, container, video_codec, width, \
    height, audio_tracks, duration_ms, acquired_at, acquisition_status, integrity_state, \
    source_metadata, source_fingerprint, source_asset_id, derivation, created_at";

pub fn insert_media_asset(c: &Connection, a: &MediaAsset) -> Result<()> {
    c.execute(
        &format!(
            "INSERT INTO media_asset ({ASSET_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, \
             ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)"
        ),
        params![
            a.id,
            a.library_item_id,
            a.role,
            a.media_kind,
            a.storage_kind,
            a.storage_root_id,
            a.storage_ref,
            a.content_hash,
            a.hash_algorithm,
            a.size_bytes,
            a.mime_type,
            a.container,
            a.video_codec,
            a.width,
            a.height,
            a.audio_tracks,
            a.duration_ms,
            a.acquired_at,
            a.acquisition_status,
            a.integrity_state,
            a.source_metadata,
            a.source_fingerprint,
            a.source_asset_id,
            a.derivation,
            a.created_at,
        ],
    )?;
    Ok(())
}

fn asset_from_row(r: &Row<'_>) -> rusqlite::Result<MediaAsset> {
    Ok(MediaAsset {
        id: r.get(0)?,
        library_item_id: r.get(1)?,
        role: r.get(2)?,
        media_kind: r.get(3)?,
        storage_kind: r.get(4)?,
        storage_root_id: r.get(5)?,
        storage_ref: r.get(6)?,
        content_hash: r.get(7)?,
        hash_algorithm: r.get(8)?,
        size_bytes: r.get(9)?,
        mime_type: r.get(10)?,
        container: r.get(11)?,
        video_codec: r.get(12)?,
        width: r.get(13)?,
        height: r.get(14)?,
        audio_tracks: r.get(15)?,
        duration_ms: r.get(16)?,
        acquired_at: r.get(17)?,
        acquisition_status: r.get(18)?,
        integrity_state: r.get(19)?,
        source_metadata: r.get(20)?,
        source_fingerprint: r.get(21)?,
        source_asset_id: r.get(22)?,
        derivation: r.get(23)?,
        created_at: r.get(24)?,
    })
}

pub fn get_media_asset(c: &Connection, id: &str) -> Result<Option<MediaAsset>> {
    Ok(c.query_row(
        &format!("SELECT {ASSET_COLUMNS} FROM media_asset WHERE id = ?1"),
        [id],
        asset_from_row,
    )
    .optional()?)
}

pub fn assets_for_item(c: &Connection, item_id: &str) -> Result<Vec<MediaAsset>> {
    let mut s = c.prepare(&format!(
        "SELECT {ASSET_COLUMNS} FROM media_asset WHERE library_item_id = ?1 ORDER BY created_at, id"
    ))?;
    let v = s
        .query_map([item_id], asset_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

// --- UrlAlias (§11.4) -------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlAlias {
    pub id: String,
    pub item_id: String,
    pub url: String,
    pub normalization_version: u32,
    pub created_at: i64,
}

pub fn insert_url_alias(c: &Connection, a: &UrlAlias) -> Result<()> {
    c.execute(
        "INSERT INTO url_alias (id, item_id, url, normalization_version, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![a.id, a.item_id, a.url, a.normalization_version, a.created_at],
    )?;
    Ok(())
}

pub fn aliases_for_item(c: &Connection, item_id: &str) -> Result<Vec<UrlAlias>> {
    let mut s = c.prepare(
        "SELECT id, item_id, url, normalization_version, created_at FROM url_alias WHERE item_id = ?1 ORDER BY created_at, id",
    )?;
    let v = s
        .query_map([item_id], |r| {
            Ok(UrlAlias {
                id: r.get(0)?,
                item_id: r.get(1)?,
                url: r.get(2)?,
                normalization_version: r.get(3)?,
                created_at: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

// --- DuplicateCandidate (§11.4) --------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateCandidate {
    pub id: String,
    pub item_a_id: String,
    pub item_b_id: String,
    pub reason: DuplicateReason,
    pub status: DuplicateStatus,
    pub created_at: i64,
    pub decided_at: Option<i64>,
}

/// Records an open candidate for the pair, or returns the one already open
/// for the same unordered pair and reason (idempotent).
pub fn open_duplicate_candidate(
    c: &Connection,
    a: &str,
    b: &str,
    reason: DuplicateReason,
    now_ms: i64,
) -> Result<DuplicateCandidate> {
    if a == b {
        return Err(LibraryError::Invalid("duplicate candidate needs two items"));
    }
    c.execute(
        "INSERT OR IGNORE INTO duplicate_candidate (id, item_a_id, item_b_id, reason, status, created_at) \
         VALUES (?1, ?2, ?3, ?4, 'open', ?5)",
        params![new_id(), a, b, reason, now_ms],
    )?;
    let d = c.query_row(
        "SELECT id, item_a_id, item_b_id, reason, status, created_at, decided_at FROM duplicate_candidate \
         WHERE min(item_a_id, item_b_id) = min(?1, ?2) AND max(item_a_id, item_b_id) = max(?1, ?2) \
         AND reason = ?3 AND status = 'open'",
        params![a, b, reason],
        dup_from_row,
    )?;
    Ok(d)
}

fn dup_from_row(r: &Row<'_>) -> rusqlite::Result<DuplicateCandidate> {
    Ok(DuplicateCandidate {
        id: r.get(0)?,
        item_a_id: r.get(1)?,
        item_b_id: r.get(2)?,
        reason: r.get(3)?,
        status: r.get(4)?,
        created_at: r.get(5)?,
        decided_at: r.get(6)?,
    })
}

pub fn open_duplicates_for_item(c: &Connection, item_id: &str) -> Result<Vec<DuplicateCandidate>> {
    let mut s = c.prepare(
        "SELECT id, item_a_id, item_b_id, reason, status, created_at, decided_at FROM duplicate_candidate \
         WHERE status = 'open' AND (item_a_id = ?1 OR item_b_id = ?1) ORDER BY created_at, id",
    )?;
    let v = s
        .query_map([item_id], dup_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn decide_duplicate_candidate(
    c: &Connection,
    id: &str,
    status: DuplicateStatus,
    now_ms: i64,
) -> Result<()> {
    if status == DuplicateStatus::Open {
        return Err(LibraryError::Invalid(
            "a decision cannot reopen a candidate",
        ));
    }
    let n = c.execute(
        "UPDATE duplicate_candidate SET status = ?1, decided_at = ?2 WHERE id = ?3 AND status = 'open'",
        params![status, now_ms, id],
    )?;
    if n == 0 {
        return Err(LibraryError::NotFound);
    }
    Ok(())
}

// --- Local media queries (STATE.md F1.4/F1.5) ------------------------------

fn root_from_row(r: &Row<'_>) -> rusqlite::Result<StorageRoot> {
    Ok(StorageRoot {
        id: r.get(0)?,
        kind: r.get(1)?,
        label: r.get(2)?,
        path: r.get(3)?,
        volume_id: r.get(4)?,
        status: r.get(5)?,
        created_at: r.get(6)?,
        last_verified_at: r.get(7)?,
    })
}

pub fn list_storage_roots(c: &Connection) -> Result<Vec<StorageRoot>> {
    let mut s = c.prepare(
        "SELECT id, kind, label, path, volume_id, status, created_at, last_verified_at \
         FROM storage_root ORDER BY created_at, id",
    )?;
    let v = s
        .query_map([], root_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn asset_at(c: &Connection, root_id: &str, storage_ref: &str) -> Result<Option<MediaAsset>> {
    Ok(c.query_row(
        &format!("SELECT {ASSET_COLUMNS} FROM media_asset WHERE storage_root_id = ?1 AND storage_ref = ?2 ORDER BY created_at LIMIT 1"),
        params![root_id, storage_ref],
        asset_from_row,
    )
    .optional()?)
}

pub fn assets_in_root(c: &Connection, root_id: &str) -> Result<Vec<MediaAsset>> {
    let mut s = c.prepare(&format!(
        "SELECT {ASSET_COLUMNS} FROM media_asset WHERE storage_root_id = ?1 ORDER BY created_at, id"
    ))?;
    let v = s
        .query_map([root_id], asset_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

/// Original assets of other items with the same content hash (IMP-05).
pub fn originals_with_hash(
    c: &Connection,
    hash: &str,
    except_item: &str,
) -> Result<Vec<MediaAsset>> {
    let mut s = c.prepare(&format!(
        "SELECT {ASSET_COLUMNS} FROM media_asset WHERE content_hash = ?1 AND role = 'original' \
         AND library_item_id <> ?2 ORDER BY created_at, id"
    ))?;
    let v = s
        .query_map(params![hash, except_item], asset_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

/// Sets an asset's integrity state. DATA-09: `missing` is only written while
/// the asset's root is `available`; on a disconnected or inaccessible root
/// the call changes nothing and returns `false`.
pub fn set_asset_integrity(c: &Connection, asset_id: &str, state: IntegrityState) -> Result<bool> {
    let n = c.execute(
        "UPDATE media_asset SET integrity_state = ?1 WHERE id = ?2 AND (?1 <> 'missing' OR \
         (SELECT status FROM storage_root WHERE id = media_asset.storage_root_id) = 'available')",
        params![state, asset_id],
    )?;
    if n == 0 && get_media_asset(c, asset_id)?.is_none() {
        return Err(LibraryError::NotFound);
    }
    Ok(n == 1)
}

/// Points an asset at a new location (relink, IMP-07). The item, its notes
/// and everything attached to it stay as they are.
pub fn relocate_asset(
    c: &Connection,
    asset_id: &str,
    root_id: &str,
    storage_ref: &str,
    size_bytes: Option<i64>,
) -> Result<()> {
    let n = c.execute(
        "UPDATE media_asset SET storage_root_id = ?1, storage_ref = ?2, size_bytes = COALESCE(?3, size_bytes), \
         integrity_state = 'verified' WHERE id = ?4",
        params![root_id, storage_ref, size_bytes, asset_id],
    )?;
    if n == 0 {
        return Err(LibraryError::NotFound);
    }
    Ok(())
}

/// Links (or clears) the item's thumbnail. A write like any other: bumps
/// `revision` so a stale editor sees the change.
pub fn set_thumbnail(
    c: &Connection,
    item_id: &str,
    asset_id: Option<&str>,
    now_ms: i64,
) -> Result<()> {
    let n = c.execute(
        "UPDATE library_item SET thumbnail_asset_id = ?1, revision = revision + 1, updated_at = ?2 WHERE id = ?3",
        params![asset_id, now_ms, item_id],
    )?;
    if n == 0 {
        return Err(LibraryError::NotFound);
    }
    Ok(())
}

/// Local items whose original video has no thumbnail yet (IMP-06 backlog),
/// newest first: the grid opens on the newest items.
pub fn items_missing_thumbnail(c: &Connection, limit: u32) -> Result<Vec<String>> {
    let mut s = c.prepare(
        "SELECT i.id FROM library_item i WHERE i.thumbnail_asset_id IS NULL AND i.removed_at IS NULL \
         AND EXISTS (SELECT 1 FROM media_asset a WHERE a.library_item_id = i.id AND a.role = 'original' \
         AND a.media_kind = 'video' AND a.integrity_state IN ('verified', 'unknown')) \
         ORDER BY i.added_at DESC, i.seq DESC LIMIT ?1",
    )?;
    let v = s
        .query_map([limit], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(v)
}
