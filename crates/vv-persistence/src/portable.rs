//! Portable JSON export and import (SPEC §15.1, §15.2; STATE.md F2.5;
//! AC-15, AC-16, AC-30, AC-51, AC-57; ADR-0017).
//!
//! The export is a JSON document of the library's own rows. Media files are
//! not inside it: each original file is a reference (root, relative path,
//! size, hash). Machine paths are left out unless the user chooses them.
//!
//! Import never changes anything until the whole document has been checked:
//!
//! 1. size, JSON shape and `schema_version` (a newer version stops with
//!    IMPORT_SCHEMA_UNSUPPORTED and the library is untouched, AC-16);
//! 2. every URL passes the capture URL policy (unsafe items are skipped);
//! 3. every relative media path stays inside its root, also through a
//!    junction or symlink once the root is mapped; one bad path refuses the
//!    whole import (AC-30);
//! 4. one transaction applies new ids and relations, unions tags and
//!    collections, and rebuilds the search index (AC-15).
//!
//! An absolute file path is used only when the user approved original
//! locations; the file gets its own `external_import` root and is never
//! written to (AC-51). A root nobody mapped becomes a disconnected
//! placeholder, so the references stay and can be relinked later.

use crate::library::organize::{
    get_or_create_tag, SmartRules, MAX_COLLECTION_DEPTH, RULE_SCHEMA_VERSION,
};
use crate::library::{new_id, repo, LibraryError};
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Marks the file as ours (a different JSON file is refused, not guessed at).
pub const FORMAT: &str = "videovault-library";
/// The portable format this app writes and reads.
pub const SCHEMA_VERSION: u32 = 1;
/// Larger files are refused before parsing (§15.2 step 1).
pub const MAX_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_ROWS: usize = 2_000_000;
/// Placeholder roots have this path until they are relinked.
pub const UNMAPPED_PREFIX: &str = "unmapped:";

pub type Row = Map<String, Value>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Document {
    #[serde(default)]
    pub format: String,
    pub schema_version: u32,
    #[serde(default)]
    pub exported_at: String,
    #[serde(default)]
    pub app_version: String,
    #[serde(default)]
    pub items: Vec<Row>,
    #[serde(default)]
    pub collections: Vec<Row>,
    #[serde(default)]
    pub smart_collections: Vec<Row>,
    #[serde(default)]
    pub tags: Vec<Row>,
    #[serde(default)]
    pub item_tags: Vec<Row>,
    #[serde(default)]
    pub item_collections: Vec<Row>,
    #[serde(default)]
    pub media_references: Vec<Row>,
    /// Transcripts arrive with F5; the field is kept so the shape is stable.
    #[serde(default)]
    pub transcripts: Vec<Row>,
    #[serde(default)]
    pub playback_states: Vec<Row>,
    #[serde(default)]
    pub storage_roots: Vec<Row>,
    #[serde(default)]
    pub duplicate_candidates: Vec<Row>,
    #[serde(default)]
    pub url_aliases: Vec<Row>,
    /// Fields this app does not know (counted in the preview, not imported).
    #[serde(flatten)]
    pub unknown: Map<String, Value>,
}

#[derive(Debug)]
pub enum PortableError {
    TooLarge,
    /// Not JSON, or not shaped like an export.
    Malformed(String),
    /// JSON, but not a VideoVault export.
    NotAnExport,
    /// IMPORT_SCHEMA_UNSUPPORTED.
    UnsupportedSchema {
        found: u64,
        supported: u32,
    },
    /// A media reference leaves its root (AC-30).
    UnsafePath(String),
    Io(String),
    Library(LibraryError),
}

impl std::fmt::Display for PortableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PortableError::TooLarge => {
                write!(f, "file is larger than {} MiB", MAX_BYTES / 1024 / 1024)
            }
            PortableError::Malformed(m) => write!(f, "not a valid export: {m}"),
            PortableError::NotAnExport => write!(f, "not a VideoVault library export"),
            PortableError::UnsupportedSchema { found, supported } => write!(
                f,
                "export schema {found} is not supported (this app reads {supported})"
            ),
            PortableError::UnsafePath(p) => write!(f, "media reference leaves its folder: {p}"),
            PortableError::Io(e) => write!(f, "{e}"),
            PortableError::Library(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PortableError {}

impl From<LibraryError> for PortableError {
    fn from(e: LibraryError) -> Self {
        PortableError::Library(e)
    }
}

impl From<rusqlite::Error> for PortableError {
    fn from(e: rusqlite::Error) -> Self {
        PortableError::Library(LibraryError::Sqlite(e))
    }
}

type Result<T> = std::result::Result<T, PortableError>;

// ---------------------------------------------------------------- export

/// Columns written and read per table. Only these names ever reach SQL.
const ITEM_COLUMNS: &[&str] = &[
    "id",
    "kind",
    "canonical_url",
    "original_url",
    "normalization_version",
    "normalization_conflict",
    "identity_conflict",
    "source_platform",
    "source_id",
    "title",
    "description",
    "creator",
    "user_overrides",
    "published_at",
    "published_date",
    "published_precision",
    "added_at",
    "updated_at",
    "duration_ms",
    "language",
    "visibility",
    "remote_availability",
    "metadata_status",
    "user_notes",
    "is_favorite",
    "archived_at",
    "removed_at",
    "last_checked_at",
    "metadata_provenance",
    "revision",
];
const COLLECTION_COLUMNS: &[&str] = &[
    "id",
    "name",
    "parent_id",
    "sort_order",
    "created_at",
    "updated_at",
];
const SMART_COLUMNS: &[&str] = &[
    "id",
    "name",
    "rule_schema_version",
    "rules",
    "sort",
    "created_at",
    "updated_at",
];
const TAG_COLUMNS: &[&str] = &["id", "name", "normalized_name", "created_at"];
const ITEM_TAG_COLUMNS: &[&str] = &["item_id", "tag_id", "origin", "created_at"];
const ITEM_COLLECTION_COLUMNS: &[&str] = &["item_id", "collection_id", "added_at"];
const PLAYBACK_COLUMNS: &[&str] = &[
    "item_id",
    "asset_id",
    "position_ms",
    "duration_ms",
    "last_played_at",
    "completed_at",
];
const DUPLICATE_COLUMNS: &[&str] = &[
    "id",
    "item_a_id",
    "item_b_id",
    "reason",
    "status",
    "created_at",
    "decided_at",
];
const ALIAS_COLUMNS: &[&str] = &[
    "id",
    "item_id",
    "url",
    "normalization_version",
    "created_at",
];

fn json_of(v: SqlValue) -> Value {
    match v {
        SqlValue::Null => Value::Null,
        SqlValue::Integer(i) => Value::from(i),
        SqlValue::Real(f) => Value::from(f),
        SqlValue::Text(s) => Value::from(s),
        SqlValue::Blob(_) => Value::Null,
    }
}

fn sql_of(v: Option<&Value>) -> SqlValue {
    match v {
        None | Some(Value::Null) => SqlValue::Null,
        Some(Value::Bool(b)) => SqlValue::Integer(i64::from(*b)),
        Some(Value::Number(n)) => n
            .as_i64()
            .map(SqlValue::Integer)
            .or_else(|| n.as_f64().map(SqlValue::Real))
            .unwrap_or(SqlValue::Null),
        Some(Value::String(s)) => SqlValue::Text(s.clone()),
        Some(other) => SqlValue::Text(other.to_string()),
    }
}

fn rows(c: &Connection, table: &str, cols: &[&str], filter: &str) -> Result<Vec<Row>> {
    let sql = format!("SELECT {} FROM {table} {filter}", cols.join(", "));
    let mut stmt = c.prepare(&sql)?;
    let out = stmt
        .query_map([], |r| {
            let mut m = Row::new();
            for (i, col) in cols.iter().enumerate() {
                m.insert((*col).to_string(), json_of(r.get::<_, SqlValue>(i)?));
            }
            Ok(m)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(out)
}

fn s<'a>(r: &'a Row, k: &str) -> Option<&'a str> {
    r.get(k).and_then(Value::as_str)
}

fn i(r: &Row, k: &str) -> Option<i64> {
    r.get(k).and_then(Value::as_i64)
}

/// `2026-11-08T14:00:00Z` from Unix milliseconds (UTC).
pub fn iso_utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil-from-days (Howard Hinnant), valid for the proleptic Gregorian calendar.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExportOptions {
    /// Adds each root's path and each file's absolute path (§15.1: a visible
    /// choice; the default export has no machine-specific paths).
    pub include_absolute_paths: bool,
}

/// Builds the export of the whole library, Recently removed included.
pub fn export(
    c: &Connection,
    opts: ExportOptions,
    app_version: &str,
    now_ms: i64,
) -> Result<Document> {
    let items = rows(c, "library_item", ITEM_COLUMNS, "ORDER BY seq")?;
    let mut refs = Vec::new();
    let mut used_roots: HashSet<String> = HashSet::new();
    {
        let mut stmt = c.prepare(
            "SELECT a.id, a.library_item_id, a.storage_root_id, a.storage_ref, a.storage_kind, a.media_kind, a.size_bytes, a.content_hash, a.hash_algorithm, \
             a.mime_type, a.container, a.video_codec, a.width, a.height, a.duration_ms, a.audio_tracks, a.acquired_at, a.created_at, r.path \
             FROM media_asset a JOIN storage_root r ON r.id = a.storage_root_id WHERE a.role = 'original' ORDER BY a.created_at, a.id",
        )?;
        const NAMES: &[&str] = &[
            "asset_id",
            "item_id",
            "root_id",
            "relative_path",
            "storage_kind",
            "media_kind",
            "size_bytes",
            "content_hash",
            "hash_algorithm",
            "mime_type",
            "container",
            "video_codec",
            "width",
            "height",
            "duration_ms",
            "audio_tracks",
            "acquired_at",
            "created_at",
        ];
        let mut q = stmt.query([])?;
        while let Some(r) = q.next()? {
            let mut m = Row::new();
            for (n, name) in NAMES.iter().enumerate() {
                m.insert((*name).to_string(), json_of(r.get::<_, SqlValue>(n)?));
            }
            let root_path: String = r.get(18)?;
            if opts.include_absolute_paths && !root_path.starts_with(UNMAPPED_PREFIX) {
                let rel: String = r.get(3)?;
                let abs = Path::new(&root_path).join(rel.replace('\\', "/"));
                m.insert(
                    "absolute_path".into(),
                    Value::from(abs.to_string_lossy().into_owned()),
                );
            }
            used_roots.insert(r.get(2)?);
            refs.push(m);
        }
    }
    let mut roots = Vec::new();
    for id in {
        let mut v: Vec<_> = used_roots.into_iter().collect();
        v.sort();
        v
    } {
        let (kind, label, path): (String, String, String) = c.query_row(
            "SELECT kind, label, path FROM storage_root WHERE id = ?1",
            [&id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let mut m = Row::new();
        m.insert("id".into(), Value::from(id));
        m.insert("kind".into(), Value::from(kind));
        m.insert("label".into(), Value::from(label));
        if opts.include_absolute_paths && !path.starts_with(UNMAPPED_PREFIX) {
            m.insert("path".into(), Value::from(path));
        }
        roots.push(m);
    }
    Ok(Document {
        format: FORMAT.into(),
        schema_version: SCHEMA_VERSION,
        exported_at: iso_utc(now_ms),
        app_version: app_version.into(),
        items,
        collections: rows(c, "collection", COLLECTION_COLUMNS, "ORDER BY created_at, id")?,
        smart_collections: rows(c, "smart_collection", SMART_COLUMNS, "ORDER BY created_at, id")?,
        tags: rows(c, "tag", TAG_COLUMNS, "ORDER BY created_at, id")?,
        item_tags: rows(c, "library_item_tag", ITEM_TAG_COLUMNS, "ORDER BY item_id, tag_id")?,
        item_collections: rows(c, "library_item_collection", ITEM_COLLECTION_COLUMNS, "ORDER BY item_id, collection_id")?,
        media_references: refs,
        transcripts: Vec::new(),
        playback_states: rows(c, "playback_state", PLAYBACK_COLUMNS, "WHERE asset_id IN (SELECT id FROM media_asset WHERE role = 'original') ORDER BY item_id")?,
        storage_roots: roots,
        duplicate_candidates: rows(c, "duplicate_candidate", DUPLICATE_COLUMNS, "ORDER BY created_at, id")?,
        url_aliases: rows(c, "url_alias", ALIAS_COLUMNS, "ORDER BY created_at, id")?,
        unknown: Map::new(),
    })
}

/// Writes the export to `dest` through a temporary file, so a failed write
/// never leaves half a file under the chosen name.
pub fn write_export(doc: &Document, dest: &Path) -> Result<u64> {
    let json = serde_json::to_vec_pretty(doc).map_err(|e| PortableError::Io(e.to_string()))?;
    let tmp = dest.with_extension("partial");
    std::fs::write(&tmp, &json).map_err(|e| PortableError::Io(e.to_string()))?;
    std::fs::rename(&tmp, dest).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        PortableError::Io(e.to_string())
    })?;
    Ok(json.len() as u64)
}

// ---------------------------------------------------------------- parse

/// Parses and checks an export (§15.2 step 1). Nothing is written.
pub fn parse(bytes: &[u8]) -> Result<Document> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(PortableError::TooLarge);
    }
    let v: Value =
        serde_json::from_slice(bytes).map_err(|e| PortableError::Malformed(e.to_string()))?;
    let obj = v.as_object().ok_or(PortableError::NotAnExport)?;
    // The version is read first: a newer file may have any shape (AC-16).
    let version = obj
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or(PortableError::NotAnExport)?;
    if obj
        .get("format")
        .and_then(Value::as_str)
        .is_some_and(|f| f != FORMAT)
    {
        return Err(PortableError::NotAnExport);
    }
    if version != u64::from(SCHEMA_VERSION) {
        return Err(PortableError::UnsupportedSchema {
            found: version,
            supported: SCHEMA_VERSION,
        });
    }
    if !obj.contains_key("items") {
        return Err(PortableError::NotAnExport);
    }
    let doc: Document =
        serde_json::from_value(v).map_err(|e| PortableError::Malformed(e.to_string()))?;
    let total = doc.items.len()
        + doc.media_references.len()
        + doc.item_tags.len()
        + doc.item_collections.len()
        + doc.url_aliases.len()
        + doc.playback_states.len();
    if total > MAX_ROWS {
        return Err(PortableError::TooLarge);
    }
    for r in &doc.media_references {
        let p = s(r, "relative_path").unwrap_or("");
        if !safe_relative(p) {
            return Err(PortableError::UnsafePath(p.to_string()));
        }
    }
    Ok(doc)
}

pub fn read_file(path: &Path) -> Result<Document> {
    let meta = std::fs::metadata(path).map_err(|e| PortableError::Io(e.to_string()))?;
    if meta.len() > MAX_BYTES {
        return Err(PortableError::TooLarge);
    }
    parse(&std::fs::read(path).map_err(|e| PortableError::Io(e.to_string()))?)
}

/// A path relative to its root that cannot leave it by its spelling: no
/// absolute or drive form, no `..`, no NUL, no Windows stream (`:`).
pub fn safe_relative(p: &str) -> bool {
    !p.is_empty()
        && p.len() <= 4096
        && !p.contains('\0')
        && !p.contains(':')
        && !p.starts_with('/')
        && !p.starts_with('\\')
        && p.split(['/', '\\'])
            .all(|part| part != ".." && !part.ends_with(' ') && !part.ends_with('.') || part == ".")
}

/// The real file under `root` for `rel`, when it exists and stays inside the
/// root after links and junctions are resolved. `Err` = it leaves the root.
fn resolve_inside(root: &Path, rel: &str) -> Result<Option<PathBuf>> {
    let Ok(root_real) = std::fs::canonicalize(root) else {
        return Ok(None);
    };
    let candidate = root.join(rel.replace('\\', "/"));
    match std::fs::canonicalize(&candidate) {
        Ok(real) if real.starts_with(&root_real) => Ok(real.is_file().then_some(real)),
        Ok(_) => Err(PortableError::UnsafePath(rel.to_string())),
        Err(_) => Ok(None),
    }
}

// ---------------------------------------------------------------- matching

fn url_ok(u: Option<&str>) -> bool {
    u.is_none_or(|u| vv_domain::url_policy::validate_capture_url(u).is_ok())
}

/// The local item an exported item is the same as (§15.2 step 3).
fn find_match(c: &Connection, it: &Row, hash: Option<&str>) -> Result<Option<String>> {
    let one = |sql: &str, p: &[&dyn rusqlite::ToSql]| -> Result<Option<String>> {
        Ok(c.query_row(sql, p, |r| r.get(0)).optional()?)
    };
    if let Some(u) = s(it, "canonical_url") {
        if let Some(id) = one(
            "SELECT id FROM library_item WHERE canonical_url = ?1",
            &[&u],
        )? {
            return Ok(Some(id));
        }
        if let Some(id) = one("SELECT item_id FROM url_alias WHERE url = ?1", &[&u])? {
            return Ok(Some(id));
        }
    }
    if let (Some(p), Some(sid)) = (s(it, "source_platform"), s(it, "source_id")) {
        if let Some(id) = one(
            "SELECT id FROM library_item WHERE source_platform = ?1 AND source_id = ?2",
            &[&p, &sid],
        )? {
            return Ok(Some(id));
        }
    }
    if s(it, "kind") == Some("local_video") {
        if let Some(h) = hash {
            return one(
                "SELECT library_item_id FROM media_asset WHERE role = 'original' AND content_hash = ?1 ORDER BY created_at LIMIT 1",
                &[&h],
            );
        }
    }
    Ok(None)
}

fn original_hashes(doc: &Document) -> HashMap<&str, &str> {
    doc.media_references
        .iter()
        .filter_map(|r| Some((s(r, "item_id")?, s(r, "content_hash")?)))
        .collect()
}

// ---------------------------------------------------------------- preview

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootPreview {
    /// Id inside the export; `map_import_root` refers to it.
    pub id: String,
    pub kind: String,
    pub label: String,
    /// The export carries this root's original path.
    pub has_path: bool,
    pub references: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub schema_version: u32,
    pub exported_at: String,
    pub app_version: String,
    pub items_new: usize,
    /// Same item already in the library (URL, source id or file hash).
    pub items_matched: usize,
    /// Items whose URL fails the URL policy; they are not imported.
    pub items_skipped: usize,
    /// Items in Recently removed; their 30 days start at import (AC-57).
    pub removed: usize,
    pub collections: usize,
    pub smart_collections: usize,
    pub tags: usize,
    pub media_references: usize,
    /// References that name an absolute file (used only with approval).
    pub absolute_paths: usize,
    pub roots: Vec<RootPreview>,
    /// Top-level fields this app does not know; they are ignored.
    pub unknown_fields: usize,
}

pub fn preview(c: &Connection, doc: &Document) -> Result<ImportPreview> {
    let hashes = original_hashes(doc);
    let (mut new, mut matched, mut skipped, mut removed) = (0, 0, 0, 0);
    for it in &doc.items {
        if !url_ok(s(it, "canonical_url")) || !url_ok(s(it, "original_url")) {
            skipped += 1;
            continue;
        }
        let h = s(it, "id").and_then(|id| hashes.get(id).copied());
        if find_match(c, it, h)?.is_some() {
            matched += 1;
        } else {
            new += 1;
        }
        removed += usize::from(it.get("removed_at").is_some_and(|v| !v.is_null()));
    }
    let roots = doc
        .storage_roots
        .iter()
        .filter_map(|r| {
            let id = s(r, "id")?.to_string();
            let references = doc
                .media_references
                .iter()
                .filter(|m| s(m, "root_id") == Some(&id))
                .count();
            Some(RootPreview {
                kind: s(r, "kind").unwrap_or("managed_media").into(),
                label: s(r, "label").unwrap_or("").into(),
                has_path: s(r, "path").is_some(),
                references,
                id,
            })
        })
        .collect();
    Ok(ImportPreview {
        schema_version: doc.schema_version,
        exported_at: doc.exported_at.clone(),
        app_version: doc.app_version.clone(),
        items_new: new,
        items_matched: matched,
        items_skipped: skipped,
        removed,
        collections: doc.collections.len(),
        smart_collections: doc.smart_collections.len(),
        tags: doc.tags.len(),
        media_references: doc.media_references.len(),
        absolute_paths: doc
            .media_references
            .iter()
            .filter(|r| s(r, "absolute_path").is_some())
            .count(),
        roots,
        unknown_fields: doc.unknown.len(),
    })
}

// ---------------------------------------------------------------- apply

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    /// §15.2 step 4: what is already in the library wins.
    #[default]
    KeepExisting,
    /// Notes, favorite, archive and overrides come from the file.
    ReplaceUserFields,
}

#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    pub policy: ConflictPolicy,
    /// Export root id → local `storage_root.id`.
    pub root_map: HashMap<String, String>,
    /// The user approved using absolute file paths from the export (AC-51).
    pub use_original_locations: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub items_added: usize,
    pub items_matched: usize,
    pub items_skipped: usize,
    pub removed_reset: usize,
    pub collections_added: usize,
    pub tags_added: usize,
    pub smart_collections_added: usize,
    /// References whose file was found where the mapping says.
    pub media_linked: usize,
    /// References kept for a later relink.
    pub media_missing: usize,
    /// Disconnected roots made for exported roots nobody mapped.
    pub placeholder_roots: usize,
    /// `external_import` roots made for approved absolute paths.
    pub external_roots: usize,
    pub search_documents: usize,
}

fn insert_row(
    c: &Connection,
    table: &str,
    cols: &[&str],
    row: &Row,
    overrides: &[(&str, SqlValue)],
    or_ignore: bool,
) -> Result<usize> {
    let placeholders: Vec<String> = (1..=cols.len()).map(|n| format!("?{n}")).collect();
    let values: Vec<SqlValue> = cols
        .iter()
        .map(|col| {
            overrides
                .iter()
                .find(|(k, _)| k == col)
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| sql_of(row.get(*col)))
        })
        .collect();
    let sql = format!(
        "INSERT {}INTO {table} ({}) VALUES ({})",
        if or_ignore { "OR IGNORE " } else { "" },
        cols.join(", "),
        placeholders.join(", ")
    );
    Ok(c.execute(&sql, params_from_iter(values))?)
}

fn id_free(c: &Connection, table: &str, id: &str) -> Result<bool> {
    Ok(c.query_row(
        &format!("SELECT 1 FROM {table} WHERE id = ?1"),
        [id],
        |_| Ok(()),
    )
    .optional()?
    .is_none())
}

/// Keeps the exported id when it is free, so a second import of the same
/// file matches instead of duplicating.
fn fresh_id(c: &Connection, table: &str, id: Option<&str>) -> Result<String> {
    match id {
        Some(id) if !id.is_empty() && id.len() <= 64 && id_free(c, table, id)? => {
            Ok(id.to_string())
        }
        _ => Ok(new_id()),
    }
}

fn root_by_path(c: &Connection, kind: &str, path: &str) -> Result<Option<String>> {
    Ok(c.query_row(
        "SELECT id FROM storage_root WHERE kind = ?1 AND path = ?2",
        [kind, path],
        |r| r.get(0),
    )
    .optional()?)
}

fn root_path(c: &Connection, id: &str) -> Result<Option<String>> {
    Ok(
        c.query_row("SELECT path FROM storage_root WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .optional()?,
    )
}

struct Placement {
    root_id: String,
    storage_ref: String,
    storage_kind: &'static str,
    found: bool,
}

/// Imports `doc` in one transaction. Any failure leaves the library as it was.
pub fn apply(
    c: &mut Connection,
    doc: &Document,
    opts: &ImportOptions,
    now_ms: i64,
) -> Result<ImportReport> {
    // Path checks that need the mapped roots run before the transaction.
    for r in &doc.media_references {
        let rel = s(r, "relative_path").unwrap_or("");
        if !safe_relative(rel) {
            return Err(PortableError::UnsafePath(rel.to_string()));
        }
        if let Some(local) = s(r, "root_id").and_then(|id| opts.root_map.get(id)) {
            let path = root_path(c, local)?.ok_or(LibraryError::NotFound)?;
            resolve_inside(Path::new(&path), rel)?;
        }
    }
    let tx = c.transaction()?;
    let mut rep = ImportReport::default();
    let hashes = original_hashes(doc);

    // Items.
    let mut item_map: HashMap<String, String> = HashMap::new();
    let mut new_items: HashSet<String> = HashSet::new();
    for it in &doc.items {
        let Some(old_id) = s(it, "id") else { continue };
        if !url_ok(s(it, "canonical_url")) || !url_ok(s(it, "original_url")) {
            rep.items_skipped += 1;
            continue;
        }
        let removed = it.get("removed_at").is_some_and(|v| !v.is_null());
        if let Some(local) = find_match(&tx, it, hashes.get(old_id).copied())? {
            rep.items_matched += 1;
            if opts.policy == ConflictPolicy::ReplaceUserFields {
                tx.execute(
                    "UPDATE library_item SET user_notes = ?2, is_favorite = ?3, archived_at = ?4, user_overrides = ?5, updated_at = ?6, revision = revision + 1 WHERE id = ?1",
                    params![
                        local,
                        s(it, "user_notes").unwrap_or(""),
                        i(it, "is_favorite").unwrap_or(0).clamp(0, 1),
                        i(it, "archived_at"),
                        s(it, "user_overrides").unwrap_or("{\"v\":1}"),
                        now_ms
                    ],
                )?;
            }
            item_map.insert(old_id.to_string(), local);
            continue;
        }
        let id = fresh_id(&tx, "library_item", Some(old_id))?;
        let removed_at = if removed {
            SqlValue::Integer(now_ms)
        } else {
            SqlValue::Null
        };
        insert_row(
            &tx,
            "library_item",
            ITEM_COLUMNS,
            it,
            &[
                ("id", SqlValue::Text(id.clone())),
                ("removed_at", removed_at),
            ],
            false,
        )?;
        rep.items_added += 1;
        rep.removed_reset += usize::from(removed);
        new_items.insert(id.clone());
        item_map.insert(old_id.to_string(), id);
    }

    // Roots and media (originals of new items only; matched items keep theirs).
    let mut placeholder: HashMap<String, String> = HashMap::new();
    let mut asset_map: HashMap<String, String> = HashMap::new();
    for r in &doc.media_references {
        let (Some(old_asset), Some(old_item), Some(old_root)) =
            (s(r, "asset_id"), s(r, "item_id"), s(r, "root_id"))
        else {
            continue;
        };
        let Some(item) = item_map.get(old_item) else {
            continue;
        };
        if !new_items.contains(item) {
            continue;
        }
        let rel = s(r, "relative_path").unwrap_or("").to_string();
        let storage_kind = if s(r, "storage_kind") == Some("external_reference") {
            "external_reference"
        } else {
            "managed"
        };
        let place = if let Some(local) = opts.root_map.get(old_root) {
            let path = root_path(&tx, local)?.ok_or(LibraryError::NotFound)?;
            let found = resolve_inside(Path::new(&path), &rel)?.is_some();
            Placement {
                root_id: local.clone(),
                storage_ref: rel,
                storage_kind,
                found,
            }
        } else if let Some(abs) = s(r, "absolute_path")
            .filter(|_| opts.use_original_locations)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute() && p.is_file())
        {
            // AC-51: the approved file is referenced where it is, under its own root.
            let dir = abs.parent().map(Path::to_path_buf).unwrap_or_default();
            let name = abs
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let dir_s = dir.to_string_lossy().into_owned();
            let root_id = match root_by_path(&tx, "external_import", &dir_s)? {
                Some(id) => id,
                None => {
                    let id = new_id();
                    tx.execute(
                        "INSERT INTO storage_root (id, kind, label, path, status, created_at, last_verified_at) VALUES (?1, 'external_import', ?2, ?3, 'available', ?4, ?4)",
                        params![id, dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| dir_s.clone()), dir_s, now_ms],
                    )?;
                    rep.external_roots += 1;
                    id
                }
            };
            Placement {
                root_id,
                storage_ref: name,
                storage_kind: "external_reference",
                found: true,
            }
        } else {
            let root_id = match placeholder.get(old_root) {
                Some(id) => id.clone(),
                None => {
                    let src = doc
                        .storage_roots
                        .iter()
                        .find(|x| s(x, "id") == Some(old_root));
                    let kind = match src.and_then(|x| s(x, "kind")) {
                        Some("external_import") => "external_import",
                        _ => "managed_media",
                    };
                    let label = src
                        .and_then(|x| s(x, "label"))
                        .filter(|l| !l.trim().is_empty())
                        .unwrap_or("Imported media");
                    let id = new_id();
                    tx.execute(
                        "INSERT INTO storage_root (id, kind, label, path, status, created_at) VALUES (?1, ?2, ?3, ?4, 'disconnected', ?5)",
                        params![id, kind, label, format!("{UNMAPPED_PREFIX}{id}"), now_ms],
                    )?;
                    rep.placeholder_roots += 1;
                    placeholder.insert(old_root.to_string(), id.clone());
                    id
                }
            };
            Placement {
                root_id,
                storage_ref: rel,
                storage_kind,
                found: false,
            }
        };
        let asset_id = fresh_id(&tx, "media_asset", Some(old_asset))?;
        let media_kind = s(r, "media_kind").unwrap_or("video");
        let hash = s(r, "content_hash");
        tx.execute(
            "INSERT INTO media_asset (id, library_item_id, role, media_kind, storage_kind, storage_root_id, storage_ref, content_hash, hash_algorithm, size_bytes, mime_type, \
             container, video_codec, width, height, audio_tracks, duration_ms, acquired_at, acquisition_status, integrity_state, created_at) \
             VALUES (?1, ?2, 'original', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, 'ready', ?18, ?19)",
            params![
                asset_id,
                item,
                media_kind,
                place.storage_kind,
                place.root_id,
                place.storage_ref,
                hash,
                hash.map(|_| "sha256"),
                i(r, "size_bytes").filter(|n| *n >= 0),
                s(r, "mime_type"),
                s(r, "container"),
                s(r, "video_codec"),
                i(r, "width"),
                i(r, "height"),
                s(r, "audio_tracks"),
                i(r, "duration_ms").filter(|n| *n >= 0),
                i(r, "acquired_at"),
                if place.found { "unknown" } else { "missing" },
                i(r, "created_at").unwrap_or(now_ms),
            ],
        )?;
        if place.found {
            rep.media_linked += 1;
        } else {
            rep.media_missing += 1;
        }
        asset_map.insert(old_asset.to_string(), asset_id);
    }

    // Tags (by name; union).
    let mut tag_map: HashMap<String, String> = HashMap::new();
    for t in &doc.tags {
        let (Some(old), Some(name)) = (s(t, "id"), s(t, "name")) else {
            continue;
        };
        let (tag, created) = get_or_create_tag(&tx, name, now_ms)?;
        rep.tags_added += usize::from(created);
        tag_map.insert(old.to_string(), tag.id);
    }
    for r in &doc.item_tags {
        let (Some(item), Some(tag)) = (
            s(r, "item_id").and_then(|x| item_map.get(x)),
            s(r, "tag_id").and_then(|x| tag_map.get(x)),
        ) else {
            continue;
        };
        let origin = s(r, "origin").unwrap_or("import");
        tx.execute(
            "INSERT OR IGNORE INTO library_item_tag (item_id, tag_id, origin, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![item, tag, origin, i(r, "created_at").unwrap_or(now_ms)],
        )?;
    }

    // Collections: parents first; same name under the same parent is reused.
    let mut coll_map: HashMap<String, String> = HashMap::new();
    let mut depth: HashMap<String, usize> = HashMap::new();
    let mut pending: Vec<&Row> = doc
        .collections
        .iter()
        .filter(|r| s(r, "id").is_some())
        .collect();
    while !pending.is_empty() {
        let before = pending.len();
        let mut rest = Vec::new();
        for r in pending {
            let old = s(r, "id").unwrap_or_default();
            let parent_old = s(r, "parent_id");
            let parent = match parent_old {
                None => None,
                Some(p) => match coll_map.get(p) {
                    Some(m) => Some(m.clone()),
                    None if doc.collections.iter().any(|x| s(x, "id") == Some(p)) => {
                        rest.push(r);
                        continue;
                    }
                    None => None,
                },
            };
            let d = parent_old.and_then(|p| depth.get(p)).map_or(1, |d| d + 1);
            if d > MAX_COLLECTION_DEPTH {
                return Err(PortableError::Library(LibraryError::Invalid(
                    "collection tree too deep",
                )));
            }
            let name = s(r, "name").unwrap_or("").trim().to_string();
            let existing: Option<String> = tx
                .query_row(
                    "SELECT id FROM collection WHERE name = ?1 AND parent_id IS ?2",
                    params![name, parent],
                    |x| x.get(0),
                )
                .optional()?;
            let id = match existing {
                Some(id) => id,
                None => {
                    let id = fresh_id(&tx, "collection", Some(old))?;
                    insert_row(
                        &tx,
                        "collection",
                        COLLECTION_COLUMNS,
                        r,
                        &[
                            ("id", SqlValue::Text(id.clone())),
                            ("name", SqlValue::Text(name)),
                            ("parent_id", parent.map_or(SqlValue::Null, SqlValue::Text)),
                            (
                                "created_at",
                                SqlValue::Integer(i(r, "created_at").unwrap_or(now_ms)),
                            ),
                            ("updated_at", SqlValue::Integer(now_ms)),
                        ],
                        false,
                    )?;
                    rep.collections_added += 1;
                    id
                }
            };
            depth.insert(old.to_string(), d);
            coll_map.insert(old.to_string(), id);
        }
        if rest.len() == before {
            return Err(PortableError::Library(LibraryError::Invalid(
                "collection tree has a cycle",
            )));
        }
        pending = rest;
    }
    for r in &doc.item_collections {
        let (Some(item), Some(coll)) = (
            s(r, "item_id").and_then(|x| item_map.get(x)),
            s(r, "collection_id").and_then(|x| coll_map.get(x)),
        ) else {
            continue;
        };
        tx.execute(
            "INSERT OR IGNORE INTO library_item_collection (item_id, collection_id, added_at) VALUES (?1, ?2, ?3)",
            params![item, coll, i(r, "added_at").unwrap_or(now_ms)],
        )?;
    }

    // Smart collections: references are remapped; an unmapped one stays and
    // makes the rule show as invalid (§15.1).
    for r in &doc.smart_collections {
        let Some(name) = s(r, "name").map(str::trim).filter(|n| !n.is_empty()) else {
            continue;
        };
        if i(r, "rule_schema_version") != Some(RULE_SCHEMA_VERSION) {
            continue;
        }
        let Ok(mut rules) = serde_json::from_str::<SmartRules>(s(r, "rules").unwrap_or("")) else {
            continue;
        };
        rules.tag_ids = rules
            .tag_ids
            .iter()
            .map(|t| tag_map.get(t).cloned().unwrap_or_else(|| t.clone()))
            .collect();
        rules.collection_ids = rules
            .collection_ids
            .iter()
            .map(|x| coll_map.get(x).cloned().unwrap_or_else(|| x.clone()))
            .collect();
        if rules.validate().is_err() {
            continue;
        }
        let json = serde_json::to_string(&rules).map_err(|_| LibraryError::Invalid("rules"))?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT id FROM smart_collection WHERE name = ?1",
                [name],
                |x| x.get(0),
            )
            .optional()?;
        match existing {
            Some(id) if opts.policy == ConflictPolicy::ReplaceUserFields => {
                tx.execute(
                    "UPDATE smart_collection SET rules = ?2, updated_at = ?3 WHERE id = ?1",
                    params![id, json, now_ms],
                )?;
            }
            Some(_) => {}
            None => {
                let id = fresh_id(&tx, "smart_collection", s(r, "id"))?;
                insert_row(
                    &tx,
                    "smart_collection",
                    SMART_COLUMNS,
                    r,
                    &[
                        ("id", SqlValue::Text(id)),
                        ("name", SqlValue::Text(name.to_string())),
                        ("rules", SqlValue::Text(json)),
                        (
                            "sort",
                            SqlValue::Text(s(r, "sort").unwrap_or("added_desc").to_string()),
                        ),
                        (
                            "created_at",
                            SqlValue::Integer(i(r, "created_at").unwrap_or(now_ms)),
                        ),
                        ("updated_at", SqlValue::Integer(now_ms)),
                    ],
                    false,
                )?;
                rep.smart_collections_added += 1;
            }
        }
    }

    // Aliases, duplicates and playback follow their items.
    for r in &doc.url_aliases {
        let (Some(item), Some(url)) = (s(r, "item_id").and_then(|x| item_map.get(x)), s(r, "url"))
        else {
            continue;
        };
        if !url_ok(Some(url)) {
            continue;
        }
        tx.execute(
            "INSERT OR IGNORE INTO url_alias (id, item_id, url, normalization_version, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![new_id(), item, url, i(r, "normalization_version").unwrap_or(0).max(0), i(r, "created_at").unwrap_or(now_ms)],
        )?;
    }
    for r in &doc.duplicate_candidates {
        let (Some(a), Some(b)) = (
            s(r, "item_a_id").and_then(|x| item_map.get(x)),
            s(r, "item_b_id").and_then(|x| item_map.get(x)),
        ) else {
            continue;
        };
        if a == b || s(r, "status") != Some("open") {
            continue;
        }
        let reason = match s(r, "reason") {
            Some(
                x @ ("normalization_conflict"
                | "source_id_conflict"
                | "content_hash"
                | "similar_url"),
            ) => x,
            _ => continue,
        };
        tx.execute(
            "INSERT OR IGNORE INTO duplicate_candidate (id, item_a_id, item_b_id, reason, status, created_at) VALUES (?1, ?2, ?3, ?4, 'open', ?5)",
            params![new_id(), a, b, reason, i(r, "created_at").unwrap_or(now_ms)],
        )?;
    }
    for r in &doc.playback_states {
        let (Some(item), Some(asset)) = (
            s(r, "item_id").and_then(|x| item_map.get(x)),
            s(r, "asset_id").and_then(|x| asset_map.get(x)),
        ) else {
            continue;
        };
        insert_row(
            &tx,
            "playback_state",
            PLAYBACK_COLUMNS,
            r,
            &[
                ("item_id", SqlValue::Text(item.clone())),
                ("asset_id", SqlValue::Text(asset.clone())),
                (
                    "position_ms",
                    SqlValue::Integer(i(r, "position_ms").unwrap_or(0).max(0)),
                ),
                (
                    "duration_ms",
                    i(r, "duration_ms")
                        .filter(|d| *d > 0)
                        .map_or(SqlValue::Null, SqlValue::Integer),
                ),
            ],
            true,
        )?;
    }

    rep.search_documents = repo::rebuild_search_index(&tx)?;
    tx.commit()?;
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_dates() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(1_793_664_000_000), "2026-11-03T00:00:00Z");
        assert_eq!(iso_utc(951_782_400_000), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn relative_paths_cannot_leave_by_spelling() {
        assert!(safe_relative("2026/clip.mp4"));
        assert!(safe_relative("./a/b.mkv"));
        for bad in [
            "",
            "../x",
            "a/../../x",
            "..\\x",
            "/etc/passwd",
            "\\\\server\\x",
            "C:\\x",
            "a/b.mp4:stream",
            "a\0b",
            "dir./x",
            "x /y",
        ] {
            assert!(!safe_relative(bad), "{bad:?} should be refused");
        }
    }

    #[test]
    fn future_and_foreign_files_are_refused_before_anything_else() {
        assert!(matches!(
            parse(br#"{"schema_version": 2, "whatever": true}"#),
            Err(PortableError::UnsupportedSchema { found: 2, .. })
        ));
        assert!(matches!(
            parse(br#"{"format":"other","schema_version":1,"items":[]}"#),
            Err(PortableError::NotAnExport)
        ));
        assert!(matches!(
            parse(b"not json"),
            Err(PortableError::Malformed(_))
        ));
        assert!(matches!(
            parse(br#"[1,2]"#),
            Err(PortableError::NotAnExport)
        ));
        let d = parse(br#"{"schema_version":1,"items":[],"future_field":1}"#).unwrap();
        assert_eq!(d.unknown.len(), 1);
    }
}
