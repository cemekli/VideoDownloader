//! Duplicate candidates and item merge (SPEC §7.3 ORG-11, §11.7; STATE.md
//! F2.3; ADR-0015).
//!
//! A merge never deletes anything: the source item is soft-deleted (it sits
//! in Recently removed), its media, tags and collections move or are copied
//! to the target, its URL becomes an alias of the target, and the notes are
//! joined. [`preview`] shows exactly that before anything is written, and
//! the merge is undoable.

use super::organize::{self, Collection, Tag};
use super::repo::{self, LibraryItem, MediaAsset, UrlAlias};
use super::{new_id, DuplicateStatus, LibraryError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateView {
    pub id: String,
    pub reason: String,
    pub created_at: i64,
    pub a: LibraryItem,
    pub b: LibraryItem,
}

/// Open candidates whose two items are both live, newest first.
pub fn open_candidates(c: &Connection) -> Result<Vec<CandidateView>> {
    let mut s = c.prepare(
        "SELECT d.id, d.reason, d.created_at, d.item_a_id, d.item_b_id FROM duplicate_candidate d \
         JOIN library_item a ON a.id = d.item_a_id JOIN library_item b ON b.id = d.item_b_id \
         WHERE d.status = 'open' AND a.removed_at IS NULL AND b.removed_at IS NULL \
         ORDER BY d.created_at DESC, d.id",
    )?;
    let rows: Vec<(String, String, i64, String, String)> = s
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    rows.into_iter()
        .map(|(id, reason, created_at, a, b)| {
            Ok(CandidateView {
                id,
                reason,
                created_at,
                a: repo::get_item(c, &a)?.ok_or(LibraryError::NotFound)?,
                b: repo::get_item(c, &b)?.ok_or(LibraryError::NotFound)?,
            })
        })
        .collect()
}

/// The two items of an open candidate, ordered `(target, source)`.
fn pair(c: &Connection, candidate_id: &str, target_id: &str) -> Result<(LibraryItem, LibraryItem)> {
    let (a, b, status): (String, String, DuplicateStatus) = c
        .query_row(
            "SELECT item_a_id, item_b_id, status FROM duplicate_candidate WHERE id = ?1",
            [candidate_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .ok_or(LibraryError::NotFound)?;
    if status != DuplicateStatus::Open {
        return Err(LibraryError::Invalid("candidate is not open"));
    }
    let source_id = if target_id == a {
        b
    } else if target_id == b {
        a
    } else {
        return Err(LibraryError::Invalid("target is not part of the candidate"));
    };
    let t = repo::get_item(c, target_id)?.ok_or(LibraryError::NotFound)?;
    let s = repo::get_item(c, &source_id)?.ok_or(LibraryError::NotFound)?;
    if t.removed_at.is_some() || s.removed_at.is_some() {
        return Err(LibraryError::Invalid(
            "an item of the candidate was removed",
        ));
    }
    Ok((t, s))
}

/// Notes after a merge: the target's, then the source's, both kept.
pub fn joined_notes(target: &str, source: &str) -> String {
    match (target.trim().is_empty(), source.trim().is_empty()) {
        (_, true) => target.to_owned(),
        (true, false) => source.to_owned(),
        (false, false) => format!("{target}\n\n{source}"),
    }
}

/// What a merge will do, shown before it runs (ORG-11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergePreview {
    pub target: LibraryItem,
    pub source: LibraryItem,
    pub notes_after: String,
    pub tags_to_add: Vec<Tag>,
    pub collections_to_add: Vec<Collection>,
    pub assets_to_move: Vec<MediaAsset>,
    /// The source link, kept as an alias of the target.
    pub alias_url: Option<String>,
    pub becomes_favorite: bool,
}

pub fn preview(c: &Connection, candidate_id: &str, target_id: &str) -> Result<MergePreview> {
    let (t, s) = pair(c, candidate_id, target_id)?;
    let have: Vec<String> = organize::item_tags(c, &t.id)?
        .into_iter()
        .map(|x| x.id)
        .collect();
    let tags_to_add = organize::item_tags(c, &s.id)?
        .into_iter()
        .filter(|x| !have.contains(&x.id))
        .collect();
    let have = organize::item_collections(c, &t.id)?;
    let collections_to_add = organize::item_collections(c, &s.id)?
        .into_iter()
        .filter(|x| !have.contains(x))
        .filter_map(|x| organize::get_collection(c, &x).transpose())
        .collect::<Result<_>>()?;
    let alias_url = s
        .canonical_url
        .clone()
        .filter(|u| Some(u) != t.canonical_url.as_ref())
        .filter(|u| repo::item_by_alias(c, u).ok().flatten().is_none());
    Ok(MergePreview {
        notes_after: joined_notes(&t.user_notes, &s.user_notes),
        tags_to_add,
        collections_to_add,
        assets_to_move: repo::assets_for_item(c, &s.id)?,
        alias_url,
        becomes_favorite: s.is_favorite && !t.is_favorite,
        target: t,
        source: s,
    })
}

/// Everything a merge changed, so Undo can put it back (ORG-11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemMerge {
    pub candidate_id: String,
    pub target_id: String,
    pub source_id: String,
    pub notes_before: String,
    pub notes_after: String,
    pub tags_added: Vec<String>,
    pub collections_added: Vec<String>,
    pub assets_moved: Vec<String>,
    /// Thumbnail the target got from the source (it had none).
    pub thumbnail_set: Option<String>,
    pub alias_id: Option<String>,
    pub favorite_set: bool,
    pub source_removed_at: i64,
}

pub fn merge(
    c: &Connection,
    candidate_id: &str,
    target_id: &str,
    now_ms: i64,
) -> Result<ItemMerge> {
    let p = preview(c, candidate_id, target_id)?;
    let (t, s) = (&p.target, &p.source);

    for tag in &p.tags_to_add {
        c.execute(
            "INSERT OR IGNORE INTO library_item_tag (item_id, tag_id, origin, proposal_id, created_at) \
             SELECT ?1, tag_id, origin, proposal_id, created_at FROM library_item_tag WHERE item_id = ?2 AND tag_id = ?3",
            params![t.id, s.id, tag.id],
        )?;
    }
    let collections_added: Vec<String> =
        p.collections_to_add.iter().map(|x| x.id.clone()).collect();
    organize::add_to_collection_many(c, &t.id, &collections_added, now_ms)?;

    let assets_moved: Vec<String> = p.assets_to_move.iter().map(|a| a.id.clone()).collect();
    for a in &assets_moved {
        c.execute(
            "UPDATE media_asset SET library_item_id = ?1 WHERE id = ?2",
            params![t.id, a],
        )?;
    }
    let thumbnail_set = match (&t.thumbnail_asset_id, &s.thumbnail_asset_id) {
        (None, Some(th)) => Some(th.clone()),
        _ => None,
    };
    let alias_id = match &p.alias_url {
        Some(url) => {
            let a = UrlAlias {
                id: new_id(),
                item_id: t.id.clone(),
                url: url.clone(),
                normalization_version: s.normalization_version,
                created_at: now_ms,
            };
            repo::insert_url_alias(c, &a)?;
            Some(a.id)
        }
        None => None,
    };
    c.execute(
        "UPDATE library_item SET user_notes = ?1, \
         thumbnail_asset_id = COALESCE(thumbnail_asset_id, ?2), \
         is_favorite = (is_favorite OR ?3), revision = revision + 1, updated_at = ?4 WHERE id = ?5",
        params![
            p.notes_after,
            thumbnail_set,
            p.becomes_favorite,
            now_ms,
            t.id
        ],
    )?;
    c.execute(
        "UPDATE library_item SET removed_at = ?1, revision = revision + 1, updated_at = ?1 WHERE id = ?2",
        params![now_ms, s.id],
    )?;
    repo::decide_duplicate_candidate(c, candidate_id, DuplicateStatus::Merged, now_ms)?;
    repo::reindex_items(c, &[t.id.clone(), s.id.clone()])?;

    Ok(ItemMerge {
        candidate_id: candidate_id.to_owned(),
        target_id: t.id.clone(),
        source_id: s.id.clone(),
        notes_before: t.user_notes.clone(),
        notes_after: p.notes_after.clone(),
        tags_added: p.tags_to_add.iter().map(|x| x.id.clone()).collect(),
        collections_added,
        assets_moved,
        thumbnail_set,
        alias_id,
        favorite_set: p.becomes_favorite,
        source_removed_at: now_ms,
    })
}

/// Undoes [`merge`]. Parts changed since the merge are left alone; returns
/// how many parts were skipped (0 = fully undone).
pub fn unmerge(c: &Connection, m: &ItemMerge, now_ms: i64) -> Result<usize> {
    let mut skipped = 0;
    let n = c.execute(
        "UPDATE library_item SET removed_at = NULL, revision = revision + 1, updated_at = ?1 \
         WHERE id = ?2 AND removed_at = ?3",
        params![now_ms, m.source_id, m.source_removed_at],
    )?;
    skipped += usize::from(n == 0);
    for a in &m.assets_moved {
        c.execute(
            "UPDATE media_asset SET library_item_id = ?1 WHERE id = ?2 AND library_item_id = ?3",
            params![m.source_id, a, m.target_id],
        )?;
    }
    if let Some(th) = &m.thumbnail_set {
        c.execute(
            "UPDATE library_item SET thumbnail_asset_id = NULL WHERE id = ?1 AND thumbnail_asset_id = ?2",
            params![m.target_id, th],
        )?;
    }
    for t in &m.tags_added {
        c.execute(
            "DELETE FROM library_item_tag WHERE item_id = ?1 AND tag_id = ?2",
            params![m.target_id, t],
        )?;
    }
    for x in &m.collections_added {
        c.execute(
            "DELETE FROM library_item_collection WHERE item_id = ?1 AND collection_id = ?2",
            params![m.target_id, x],
        )?;
    }
    if m.notes_before != m.notes_after {
        let n = c.execute(
            "UPDATE library_item SET user_notes = ?1, revision = revision + 1, updated_at = ?2 \
             WHERE id = ?3 AND user_notes = ?4",
            params![m.notes_before, now_ms, m.target_id, m.notes_after],
        )?;
        skipped += usize::from(n == 0);
    }
    if m.favorite_set {
        c.execute(
            "UPDATE library_item SET is_favorite = 0, revision = revision + 1, updated_at = ?1 \
             WHERE id = ?2 AND is_favorite = 1",
            params![now_ms, m.target_id],
        )?;
    }
    if let Some(a) = &m.alias_id {
        c.execute("DELETE FROM url_alias WHERE id = ?1", [a])?;
    }
    c.execute(
        "UPDATE OR IGNORE duplicate_candidate SET status = 'open', decided_at = NULL WHERE id = ?1",
        [&m.candidate_id],
    )?;
    repo::reindex_items(c, &[m.target_id.clone(), m.source_id.clone()])?;
    Ok(skipped)
}

/// "Not a duplicate": closes the candidate without touching either item.
pub fn dismiss(c: &Connection, candidate_id: &str, now_ms: i64) -> Result<()> {
    repo::decide_duplicate_candidate(c, candidate_id, DuplicateStatus::Dismissed, now_ms)
}
