//! Tags, collections, smart collections and bulk item flags (SPEC §7.3
//! ORG-01…10, §11.4; STATE.md F2.1–F2.2; ADR-0015).
//!
//! Every function takes a `&Connection` so the caller composes it with the
//! operation log ([`super::undo`]) in one transaction. Functions that change
//! something return exactly what they changed, which is what Undo needs:
//! nothing that was already in the target state is reported or logged.
//! Items in Recently removed are never touched by these operations.

use super::repo;
use super::search::{ArchiveFilter, Filters};
use super::{new_id, ItemKind, LibraryError, Result, SourcePlatform};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub const MAX_TAG_LEN: usize = 100;
pub const MAX_COLLECTION_NAME_LEN: usize = 200;
/// Deepest collection tree the UI is expected to show; deeper moves are refused.
pub const MAX_COLLECTION_DEPTH: usize = 8;

// --- Names -----------------------------------------------------------------

/// Display form: trimmed, inner whitespace collapsed to one space.
pub fn clean_name(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Uniqueness key of a tag: case folded like search (Turkish `ı`/`İ` → `i`),
/// so "İstanbul", "istanbul" and "ISTANBUL" are one tag.
pub fn normalize_tag(s: &str) -> String {
    clean_name(s)
        .chars()
        .flat_map(|c| match c {
            'İ' | 'I' | 'ı' => vec!['i'],
            c => c.to_lowercase().collect(),
        })
        .filter(|&c| c != '\u{307}')
        .collect()
}

fn checked_name(s: &str, max: usize, what: &'static str) -> Result<String> {
    let n = clean_name(s);
    if n.is_empty() || n.chars().count() > max {
        return Err(LibraryError::Invalid(what));
    }
    Ok(n)
}

/// Ids of live (not removed) items among `ids`, in the given order, deduplicated.
pub fn live_ids(c: &Connection, ids: &[String]) -> Result<Vec<String>> {
    let mut s =
        c.prepare_cached("SELECT 1 FROM library_item WHERE id = ?1 AND removed_at IS NULL")?;
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for id in ids {
        if seen.insert(id.as_str()) && s.exists([id])? {
            out.push(id.clone());
        }
    }
    Ok(out)
}

// --- Tags ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub normalized_name: String,
    pub created_at: i64,
}

/// One item–tag relation, as stored (Undo puts it back exactly).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagRelation {
    pub item_id: String,
    pub origin: String,
    pub proposal_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagCount {
    pub tag: Tag,
    /// Live items carrying the tag.
    pub items: i64,
}

fn tag_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: r.get(0)?,
        name: r.get(1)?,
        normalized_name: r.get(2)?,
        created_at: r.get(3)?,
    })
}

pub fn get_tag(c: &Connection, id: &str) -> Result<Option<Tag>> {
    Ok(c.query_row(
        "SELECT id, name, normalized_name, created_at FROM tag WHERE id = ?1",
        [id],
        tag_row,
    )
    .optional()?)
}

pub fn tag_by_name(c: &Connection, name: &str) -> Result<Option<Tag>> {
    Ok(c.query_row(
        "SELECT id, name, normalized_name, created_at FROM tag WHERE normalized_name = ?1",
        [normalize_tag(name)],
        tag_row,
    )
    .optional()?)
}

pub fn list_tags(c: &Connection) -> Result<Vec<TagCount>> {
    let mut s = c.prepare(
        "SELECT t.id, t.name, t.normalized_name, t.created_at, \
         (SELECT count(*) FROM library_item_tag lt JOIN library_item i ON i.id = lt.item_id \
          WHERE lt.tag_id = t.id AND i.removed_at IS NULL) \
         FROM tag t ORDER BY t.normalized_name",
    )?;
    let v = s
        .query_map([], |r| {
            Ok(TagCount {
                tag: tag_row(r)?,
                items: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(v)
}

pub fn item_tags(c: &Connection, item_id: &str) -> Result<Vec<Tag>> {
    let mut s = c.prepare_cached(
        "SELECT t.id, t.name, t.normalized_name, t.created_at FROM library_item_tag lt \
         JOIN tag t ON t.id = lt.tag_id WHERE lt.item_id = ?1 ORDER BY t.normalized_name",
    )?;
    let v = s
        .query_map([item_id], tag_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(v)
}

fn insert_tag(c: &Connection, t: &Tag) -> Result<()> {
    c.execute(
        "INSERT INTO tag (id, name, normalized_name, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![t.id, t.name, t.normalized_name, t.created_at],
    )?;
    Ok(())
}

/// The tag with this name, created when there is none. `true` = created.
pub fn get_or_create_tag(c: &Connection, name: &str, now_ms: i64) -> Result<(Tag, bool)> {
    let name = checked_name(name, MAX_TAG_LEN, "tag name must be 1–100 characters")?;
    if let Some(t) = tag_by_name(c, &name)? {
        return Ok((t, false));
    }
    let t = Tag {
        id: new_id(),
        normalized_name: normalize_tag(&name),
        name,
        created_at: now_ms,
    };
    insert_tag(c, &t)?;
    Ok((t, true))
}

/// Adds the tag to live items that do not have it. Returns those items.
pub fn tag_items(c: &Connection, tag_id: &str, ids: &[String], now_ms: i64) -> Result<Vec<String>> {
    let mut added = Vec::new();
    let mut s = c.prepare_cached(
        "INSERT OR IGNORE INTO library_item_tag (item_id, tag_id, origin, created_at) \
         VALUES (?1, ?2, 'user', ?3)",
    )?;
    for id in live_ids(c, ids)? {
        if s.execute(params![id, tag_id, now_ms])? == 1 {
            added.push(id);
        }
    }
    repo::reindex_items(c, &added)?;
    Ok(added)
}

/// Removes the tag from live items that have it. Returns the removed rows.
pub fn untag_items(c: &Connection, tag_id: &str, ids: &[String]) -> Result<Vec<TagRelation>> {
    let mut out = Vec::new();
    for id in live_ids(c, ids)? {
        let rel: Option<TagRelation> = c
            .query_row(
                "DELETE FROM library_item_tag WHERE item_id = ?1 AND tag_id = ?2 \
                 RETURNING item_id, origin, proposal_id, created_at",
                params![id, tag_id],
                |r| {
                    Ok(TagRelation {
                        item_id: r.get(0)?,
                        origin: r.get(1)?,
                        proposal_id: r.get(2)?,
                        created_at: r.get(3)?,
                    })
                },
            )
            .optional()?;
        out.extend(rel);
    }
    let ids: Vec<String> = out.iter().map(|r| r.item_id.clone()).collect();
    repo::reindex_items(c, &ids)?;
    Ok(out)
}

/// Puts relations back as they were (Undo). Missing items or tag: skipped.
pub fn restore_tag_relations(c: &Connection, tag_id: &str, rels: &[TagRelation]) -> Result<usize> {
    let mut n = 0;
    let mut ids = Vec::new();
    for r in rels {
        n += c.execute(
            "INSERT OR IGNORE INTO library_item_tag (item_id, tag_id, origin, proposal_id, created_at) \
             SELECT ?1, ?2, ?3, ?4, ?5 WHERE EXISTS (SELECT 1 FROM library_item WHERE id = ?1) \
             AND EXISTS (SELECT 1 FROM tag WHERE id = ?2)",
            params![r.item_id, tag_id, r.origin, r.proposal_id, r.created_at],
        )?;
        ids.push(r.item_id.clone());
    }
    repo::reindex_items(c, &ids)?;
    Ok(n)
}

pub fn tag_relations(c: &Connection, tag_id: &str) -> Result<Vec<TagRelation>> {
    let mut s = c.prepare(
        "SELECT item_id, origin, proposal_id, created_at FROM library_item_tag WHERE tag_id = ?1 \
         ORDER BY item_id",
    )?;
    let v = s
        .query_map([tag_id], |r| {
            Ok(TagRelation {
                item_id: r.get(0)?,
                origin: r.get(1)?,
                proposal_id: r.get(2)?,
                created_at: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(v)
}

fn tagged_ids(c: &Connection, tag_id: &str) -> Result<Vec<String>> {
    Ok(tag_relations(c, tag_id)?
        .into_iter()
        .map(|r| r.item_id)
        .collect())
}

/// Renames a tag; its items are reindexed in the same transaction so the new
/// name is searchable at once and the old one is not (ORG-09, AC-39).
/// Returns the tag as it was. A name another tag holds is refused (merge).
pub fn rename_tag(c: &Connection, id: &str, new_name: &str) -> Result<Tag> {
    let old = get_tag(c, id)?.ok_or(LibraryError::NotFound)?;
    let name = checked_name(new_name, MAX_TAG_LEN, "tag name must be 1–100 characters")?;
    let norm = normalize_tag(&name);
    if let Some(other) = tag_by_name(c, &name)? {
        if other.id != id {
            return Err(LibraryError::Invalid("tag-name-taken"));
        }
    }
    c.execute(
        "UPDATE tag SET name = ?1, normalized_name = ?2 WHERE id = ?3",
        params![name, norm, id],
    )?;
    repo::reindex_items(c, &tagged_ids(c, id)?)?;
    Ok(old)
}

/// What a tag merge did, enough to undo it exactly (AC-46).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagMerge {
    pub source: Tag,
    pub source_relations: Vec<TagRelation>,
    pub target_id: String,
    /// Items that got the target tag from the merge (they did not have it).
    pub target_added: Vec<String>,
}

/// Moves every relation of `source_id` to `target_id` and deletes the source.
pub fn merge_tags(c: &Connection, source_id: &str, target_id: &str) -> Result<TagMerge> {
    if source_id == target_id {
        return Err(LibraryError::Invalid("cannot merge a tag into itself"));
    }
    let source = get_tag(c, source_id)?.ok_or(LibraryError::NotFound)?;
    get_tag(c, target_id)?.ok_or(LibraryError::NotFound)?;
    let rels = tag_relations(c, source_id)?;
    let mut added = Vec::new();
    for r in &rels {
        let n = c.execute(
            "INSERT OR IGNORE INTO library_item_tag (item_id, tag_id, origin, proposal_id, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![r.item_id, target_id, r.origin, r.proposal_id, r.created_at],
        )?;
        if n == 1 {
            added.push(r.item_id.clone());
        }
    }
    c.execute("DELETE FROM tag WHERE id = ?1", [source_id])?;
    let ids: Vec<String> = rels.iter().map(|r| r.item_id.clone()).collect();
    repo::reindex_items(c, &ids)?;
    Ok(TagMerge {
        source,
        source_relations: rels,
        target_id: target_id.to_owned(),
        target_added: added,
    })
}

/// Undoes [`merge_tags`]: the source tag comes back with the same id, name
/// and relations; the relations the merge added to the target go away.
pub fn unmerge_tags(c: &Connection, m: &TagMerge) -> Result<()> {
    if get_tag(c, &m.source.id)?.is_some() {
        return Err(LibraryError::Invalid("source tag exists again"));
    }
    if tag_by_name(c, &m.source.name)?.is_some() {
        return Err(LibraryError::Invalid("tag-name-taken"));
    }
    insert_tag(c, &m.source)?;
    for id in &m.target_added {
        c.execute(
            "DELETE FROM library_item_tag WHERE item_id = ?1 AND tag_id = ?2",
            params![id, m.target_id],
        )?;
    }
    restore_tag_relations(c, &m.source.id, &m.source_relations)?;
    repo::reindex_items(c, &m.target_added)?;
    Ok(())
}

/// Deletes a tag and its relations (not undoable; the UI confirms first).
/// Items are reindexed so the name stops matching.
pub fn delete_tag(c: &Connection, id: &str) -> Result<usize> {
    let ids = tagged_ids(c, id)?;
    let n = c.execute("DELETE FROM tag WHERE id = ?1", [id])?;
    if n == 0 {
        return Err(LibraryError::NotFound);
    }
    repo::reindex_items(c, &ids)?;
    Ok(ids.len())
}

// --- Collections -----------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub sort_order: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionCount {
    pub collection: Collection,
    /// Live items directly in this collection (not in sub-collections).
    pub items: i64,
}

fn collection_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Collection> {
    Ok(Collection {
        id: r.get(0)?,
        name: r.get(1)?,
        parent_id: r.get(2)?,
        sort_order: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
    })
}

const COLLECTION_COLUMNS: &str = "id, name, parent_id, sort_order, created_at, updated_at";

pub fn get_collection(c: &Connection, id: &str) -> Result<Option<Collection>> {
    Ok(c.query_row(
        &format!("SELECT {COLLECTION_COLUMNS} FROM collection WHERE id = ?1"),
        [id],
        collection_row,
    )
    .optional()?)
}

/// Every collection, parents before children, siblings by sort order then name.
pub fn list_collections(c: &Connection) -> Result<Vec<CollectionCount>> {
    let mut s = c.prepare(&format!(
        "SELECT {COLLECTION_COLUMNS}, \
         (SELECT count(*) FROM library_item_collection lc JOIN library_item i ON i.id = lc.item_id \
          WHERE lc.collection_id = collection.id AND i.removed_at IS NULL) \
         FROM collection ORDER BY sort_order, lower(name), id"
    ))?;
    let all: Vec<CollectionCount> = s
        .query_map([], |r| {
            Ok(CollectionCount {
                collection: collection_row(r)?,
                items: r.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    // Depth-first order so the UI can draw the tree in one pass.
    fn walk(all: &[CollectionCount], parent: Option<&str>, out: &mut Vec<CollectionCount>) {
        for c in all
            .iter()
            .filter(|c| c.collection.parent_id.as_deref() == parent)
        {
            out.push(c.clone());
            walk(all, Some(&c.collection.id), out);
        }
    }
    let mut out = Vec::with_capacity(all.len());
    walk(&all, None, &mut out);
    Ok(out)
}

fn children(c: &Connection, id: &str) -> Result<Vec<String>> {
    let mut s = c.prepare_cached("SELECT id FROM collection WHERE parent_id = ?1")?;
    let v = s
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(v)
}

/// `id` and every collection below it, parents first.
pub fn subtree(c: &Connection, id: &str) -> Result<Vec<String>> {
    let mut out = vec![id.to_owned()];
    let mut i = 0;
    while i < out.len() {
        let kids = children(c, &out[i])?;
        out.extend(kids);
        i += 1;
    }
    Ok(out)
}

/// Levels in the subtree of `id`, itself included (a leaf is 1).
fn height(c: &Connection, id: &str) -> Result<usize> {
    let mut level = vec![id.to_owned()];
    let mut h = 0;
    while !level.is_empty() && h <= MAX_COLLECTION_DEPTH {
        h += 1;
        let mut next = Vec::new();
        for x in &level {
            next.extend(children(c, x)?);
        }
        level = next;
    }
    Ok(h)
}

fn depth(c: &Connection, id: Option<&str>) -> Result<usize> {
    let mut d = 0;
    let mut cur = id.map(str::to_owned);
    while let Some(id) = cur {
        d += 1;
        if d > MAX_COLLECTION_DEPTH + 1 {
            break;
        }
        cur = get_collection(c, &id)?
            .ok_or(LibraryError::NotFound)?
            .parent_id;
    }
    Ok(d)
}

pub fn create_collection(
    c: &Connection,
    name: &str,
    parent_id: Option<&str>,
    now_ms: i64,
) -> Result<Collection> {
    let name = checked_name(
        name,
        MAX_COLLECTION_NAME_LEN,
        "collection name must be 1–200 characters",
    )?;
    if depth(c, parent_id)? >= MAX_COLLECTION_DEPTH {
        return Err(LibraryError::Invalid("collection tree too deep"));
    }
    let col = Collection {
        id: new_id(),
        name,
        parent_id: parent_id.map(str::to_owned),
        sort_order: 0,
        created_at: now_ms,
        updated_at: now_ms,
    };
    c.execute(
        &format!("INSERT INTO collection ({COLLECTION_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"),
        params![
            col.id,
            col.name,
            col.parent_id,
            col.sort_order,
            col.created_at,
            col.updated_at
        ],
    )?;
    Ok(col)
}

pub fn rename_collection(c: &Connection, id: &str, name: &str, now_ms: i64) -> Result<()> {
    let name = checked_name(
        name,
        MAX_COLLECTION_NAME_LEN,
        "collection name must be 1–200 characters",
    )?;
    let n = c.execute(
        "UPDATE collection SET name = ?1, updated_at = ?2 WHERE id = ?3",
        params![name, now_ms, id],
    )?;
    if n == 0 {
        return Err(LibraryError::NotFound);
    }
    Ok(())
}

/// Moves a collection under `new_parent` (or to the top). A move into its
/// own subtree would make a cycle and is refused (§11.4). This is the
/// keyboard alternative to dragging in the tree (ORG-08).
pub fn move_collection(
    c: &Connection,
    id: &str,
    new_parent: Option<&str>,
    now_ms: i64,
) -> Result<()> {
    get_collection(c, id)?.ok_or(LibraryError::NotFound)?;
    if let Some(p) = new_parent {
        if subtree(c, id)?.iter().any(|x| x == p) {
            return Err(LibraryError::Invalid("collection-cycle"));
        }
        let below = height(c, id)?;
        if depth(c, Some(p))? + below > MAX_COLLECTION_DEPTH {
            return Err(LibraryError::Invalid("collection tree too deep"));
        }
    }
    c.execute(
        "UPDATE collection SET parent_id = ?1, updated_at = ?2 WHERE id = ?3",
        params![new_parent, now_ms, id],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildPolicy {
    /// Sub-collections move to the deleted collection's parent (default).
    MoveUp,
    /// Sub-collections are deleted too.
    DeleteAll,
}

/// Deletes a collection (ORG-01, ORG-10). Items and files are never touched;
/// only memberships go (AC-06, AC-48). Returns how many collections went.
pub fn delete_collection(
    c: &Connection,
    id: &str,
    policy: ChildPolicy,
    now_ms: i64,
) -> Result<usize> {
    let col = get_collection(c, id)?.ok_or(LibraryError::NotFound)?;
    let doomed = match policy {
        ChildPolicy::MoveUp => {
            c.execute(
                "UPDATE collection SET parent_id = ?1, updated_at = ?2 WHERE parent_id = ?3",
                params![col.parent_id, now_ms, id],
            )?;
            vec![id.to_owned()]
        }
        ChildPolicy::DeleteAll => subtree(c, id)?,
    };
    // Children first, so the RESTRICT parent key never fires.
    for x in doomed.iter().rev() {
        c.execute("DELETE FROM collection WHERE id = ?1", [x])?;
    }
    Ok(doomed.len())
}

/// Live items added to the collection that were not in it.
pub fn add_to_collection(
    c: &Connection,
    collection_id: &str,
    ids: &[String],
    now_ms: i64,
) -> Result<Vec<String>> {
    get_collection(c, collection_id)?.ok_or(LibraryError::NotFound)?;
    let mut s = c.prepare_cached(
        "INSERT OR IGNORE INTO library_item_collection (item_id, collection_id, added_at) VALUES (?1, ?2, ?3)",
    )?;
    let mut added = Vec::new();
    for id in live_ids(c, ids)? {
        if s.execute(params![id, collection_id, now_ms])? == 1 {
            added.push(id);
        }
    }
    Ok(added)
}

/// Membership rows removed: `(item_id, added_at)`.
pub fn remove_from_collection(
    c: &Connection,
    collection_id: &str,
    ids: &[String],
) -> Result<Vec<(String, i64)>> {
    let mut out = Vec::new();
    for id in live_ids(c, ids)? {
        let at: Option<i64> = c
            .query_row(
                "DELETE FROM library_item_collection WHERE item_id = ?1 AND collection_id = ?2 RETURNING added_at",
                params![id, collection_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(at) = at {
            out.push((id, at));
        }
    }
    Ok(out)
}

pub fn restore_memberships(
    c: &Connection,
    collection_id: &str,
    rows: &[(String, i64)],
) -> Result<usize> {
    let mut n = 0;
    for (id, at) in rows {
        n += c.execute(
            "INSERT OR IGNORE INTO library_item_collection (item_id, collection_id, added_at) \
             SELECT ?1, ?2, ?3 WHERE EXISTS (SELECT 1 FROM library_item WHERE id = ?1) \
             AND EXISTS (SELECT 1 FROM collection WHERE id = ?2)",
            params![id, collection_id, at],
        )?;
    }
    Ok(n)
}

pub fn item_collections(c: &Connection, item_id: &str) -> Result<Vec<String>> {
    let mut s = c.prepare_cached(
        "SELECT collection_id FROM library_item_collection WHERE item_id = ?1 ORDER BY collection_id",
    )?;
    let v = s
        .query_map([item_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(v)
}

// --- Favorite, archive, remove (bulk) --------------------------------------

/// Sets `is_favorite` on live items whose value differs. Returns them.
pub fn set_favorite(
    c: &Connection,
    ids: &[String],
    value: bool,
    now_ms: i64,
) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for id in live_ids(c, ids)? {
        let n = c.execute(
            "UPDATE library_item SET is_favorite = ?1, revision = revision + 1, updated_at = ?2 \
             WHERE id = ?3 AND is_favorite <> ?1",
            params![value, now_ms, id],
        )?;
        if n == 1 {
            out.push(id);
        }
    }
    Ok(out)
}

/// Archives (`Some(at)`) or unarchives (`None`) live items whose state
/// differs. Returns `(id, previous archived_at)`.
pub fn set_archived(
    c: &Connection,
    ids: &[String],
    at: Option<i64>,
    now_ms: i64,
) -> Result<Vec<(String, Option<i64>)>> {
    let mut out = Vec::new();
    for id in live_ids(c, ids)? {
        let prev: Option<i64> = c.query_row(
            "SELECT archived_at FROM library_item WHERE id = ?1",
            [&id],
            |r| r.get(0),
        )?;
        if prev.is_some() == at.is_some() {
            continue;
        }
        c.execute(
            "UPDATE library_item SET archived_at = ?1, revision = revision + 1, updated_at = ?2 WHERE id = ?3",
            params![at, now_ms, id],
        )?;
        out.push((id, prev));
    }
    Ok(out)
}

/// Soft delete (§7.3): sets `removed_at` on live items. Returns them.
pub fn remove_items(c: &Connection, ids: &[String], now_ms: i64) -> Result<Vec<String>> {
    let live = live_ids(c, ids)?;
    for id in &live {
        c.execute(
            "UPDATE library_item SET removed_at = ?1, revision = revision + 1, updated_at = ?1 WHERE id = ?2",
            params![now_ms, id],
        )?;
    }
    Ok(live)
}

/// Brings removed items back. Returns `(id, removed_at it had)`.
pub fn restore_items(c: &Connection, ids: &[String], now_ms: i64) -> Result<Vec<(String, i64)>> {
    let mut out = Vec::new();
    for id in ids {
        let at: Option<Option<i64>> = c
            .query_row(
                "SELECT removed_at FROM library_item WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(Some(at)) = at {
            c.execute(
                "UPDATE library_item SET removed_at = NULL, revision = revision + 1, updated_at = ?1 WHERE id = ?2",
                params![now_ms, id],
            )?;
            out.push((id.clone(), at));
        }
    }
    Ok(out)
}

// --- Smart collections (ORG-07) --------------------------------------------

pub const RULE_SCHEMA_VERSION: i64 = 1;

/// Rules v1: every condition must hold (AND); a list matches any of its
/// values. Unknown fields are refused, so a newer rule file is never
/// silently half-applied.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct SmartRules {
    pub text: String,
    pub platforms: Vec<String>,
    pub kinds: Vec<String>,
    pub tag_ids: Vec<String>,
    pub collection_ids: Vec<String>,
    pub favorite_only: bool,
    pub include_archived: bool,
    pub added_within_days: Option<u32>,
    pub min_duration_ms: Option<i64>,
    pub max_duration_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmartCollection {
    pub id: String,
    pub name: String,
    pub rules: SmartRules,
    pub created_at: i64,
    pub updated_at: i64,
}

impl SmartRules {
    /// Checks the rules; `Err` names the first problem.
    pub fn validate(&self) -> std::result::Result<(), &'static str> {
        if self.text.chars().count() > 500 {
            return Err("text too long");
        }
        if self
            .platforms
            .iter()
            .any(|p| SourcePlatform::parse(p).is_none())
        {
            return Err("unknown platform");
        }
        if self.kinds.iter().any(|k| ItemKind::parse(k).is_none()) {
            return Err("unknown kind");
        }
        if self.added_within_days.is_some_and(|d| d == 0 || d > 36_500) {
            return Err("added_within_days out of range");
        }
        if self.min_duration_ms.is_some_and(|d| d < 0)
            || self.max_duration_ms.is_some_and(|d| d < 0)
        {
            return Err("negative duration");
        }
        if let (Some(a), Some(b)) = (self.min_duration_ms, self.max_duration_ms) {
            if a > b {
                return Err("min duration above max");
            }
        }
        let empty = self.text.trim().is_empty()
            && self.platforms.is_empty()
            && self.kinds.is_empty()
            && self.tag_ids.is_empty()
            && self.collection_ids.is_empty()
            && !self.favorite_only
            && self.added_within_days.is_none()
            && self.min_duration_ms.is_none()
            && self.max_duration_ms.is_none();
        if empty {
            return Err("no condition");
        }
        Ok(())
    }

    /// Search filters for these rules at `now_ms`. Validate first.
    pub fn filters(&self, now_ms: i64) -> Filters {
        Filters {
            platforms: self
                .platforms
                .iter()
                .filter_map(|p| SourcePlatform::parse(p))
                .collect(),
            kinds: self
                .kinds
                .iter()
                .filter_map(|k| ItemKind::parse(k))
                .collect(),
            favorite_only: self.favorite_only,
            archived: if self.include_archived {
                ArchiveFilter::Include
            } else {
                ArchiveFilter::Exclude
            },
            added_since: self
                .added_within_days
                .map(|d| now_ms - i64::from(d) * 86_400_000),
            tag_ids: self.tag_ids.clone(),
            collection_ids: self.collection_ids.clone(),
            min_duration_ms: self.min_duration_ms,
            max_duration_ms: self.max_duration_ms,
            ..Default::default()
        }
    }

    /// Tags or collections the rules name that no longer exist: the UI
    /// shows the rule as invalid instead of silently matching less.
    pub fn missing_refs(&self, c: &Connection) -> Result<usize> {
        let mut n = 0;
        for t in &self.tag_ids {
            n += usize::from(get_tag(c, t)?.is_none());
        }
        for x in &self.collection_ids {
            n += usize::from(get_collection(c, x)?.is_none());
        }
        Ok(n)
    }
}

fn smart_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<(String, String, i64, String, i64, i64)> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
    ))
}

fn to_smart(row: (String, String, i64, String, i64, i64)) -> Result<SmartCollection> {
    let (id, name, version, rules, created_at, updated_at) = row;
    if version != RULE_SCHEMA_VERSION {
        return Err(LibraryError::Invalid("unknown rule schema version"));
    }
    let rules: SmartRules = serde_json::from_str(&rules)
        .map_err(|_| LibraryError::Invalid("stored rules unreadable"))?;
    Ok(SmartCollection {
        id,
        name,
        rules,
        created_at,
        updated_at,
    })
}

pub fn get_smart_collection(c: &Connection, id: &str) -> Result<Option<SmartCollection>> {
    c.query_row(
        "SELECT id, name, rule_schema_version, rules, created_at, updated_at FROM smart_collection WHERE id = ?1",
        [id],
        smart_row,
    )
    .optional()?
    .map(to_smart)
    .transpose()
}

pub fn list_smart_collections(c: &Connection) -> Result<Vec<SmartCollection>> {
    let mut s = c.prepare(
        "SELECT id, name, rule_schema_version, rules, created_at, updated_at FROM smart_collection \
         ORDER BY lower(name), id",
    )?;
    let rows: Vec<_> = s
        .query_map([], smart_row)?
        .collect::<rusqlite::Result<_>>()?;
    rows.into_iter().map(to_smart).collect()
}

/// Creates (`id = None`) or replaces a smart collection.
pub fn save_smart_collection(
    c: &Connection,
    id: Option<&str>,
    name: &str,
    rules: &SmartRules,
    now_ms: i64,
) -> Result<SmartCollection> {
    let name = checked_name(
        name,
        MAX_COLLECTION_NAME_LEN,
        "collection name must be 1–200 characters",
    )?;
    rules.validate().map_err(LibraryError::Invalid)?;
    let json = serde_json::to_string(rules).expect("rules serialize");
    match id {
        None => {
            let id = new_id();
            c.execute(
                "INSERT INTO smart_collection (id, name, rule_schema_version, rules, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![id, name, RULE_SCHEMA_VERSION, json, now_ms],
            )?;
            get_smart_collection(c, &id)?.ok_or(LibraryError::NotFound)
        }
        Some(id) => {
            let n = c.execute(
                "UPDATE smart_collection SET name = ?1, rule_schema_version = ?2, rules = ?3, updated_at = ?4 WHERE id = ?5",
                params![name, RULE_SCHEMA_VERSION, json, now_ms, id],
            )?;
            if n == 0 {
                return Err(LibraryError::NotFound);
            }
            get_smart_collection(c, id)?.ok_or(LibraryError::NotFound)
        }
    }
}

pub fn delete_smart_collection(c: &Connection, id: &str) -> Result<()> {
    if c.execute("DELETE FROM smart_collection WHERE id = ?1", [id])? == 0 {
        return Err(LibraryError::NotFound);
    }
    Ok(())
}

/// Number of open duplicate candidates whose two items are both live.
pub fn open_duplicate_count(c: &Connection) -> Result<i64> {
    Ok(c.query_row(
        "SELECT count(*) FROM duplicate_candidate d \
         JOIN library_item a ON a.id = d.item_a_id JOIN library_item b ON b.id = d.item_b_id \
         WHERE d.status = 'open' AND a.removed_at IS NULL AND b.removed_at IS NULL",
        [],
        |r| r.get(0),
    )?)
}

/// Puts one item into several collections (merge); ignores ones it is in.
pub fn add_to_collection_many(
    c: &Connection,
    item_id: &str,
    collection_ids: &[String],
    now_ms: i64,
) -> Result<()> {
    for x in collection_ids {
        c.execute(
            "INSERT OR IGNORE INTO library_item_collection (item_id, collection_id, added_at) VALUES (?1, ?2, ?3)",
            params![item_id, x, now_ms],
        )?;
    }
    Ok(())
}
