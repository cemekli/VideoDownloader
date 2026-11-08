//! Undo (SPEC §7.3 "Undo semantiği"; STATE.md F2.2; ADR-0015).
//!
//! Each undoable user operation writes one row to `operation_log` in the
//! same transaction as its change. Only the newest row of the current app
//! session can be undone: a newer undoable operation or a restart makes
//! older rows stale. Background work (metadata, thumbnails, indexing) never
//! writes here, so it never invalidates Undo (AC-60).
//!
//! Undo writes back only what the operation changed. Where a field it set
//! has since been changed to something else, that record is skipped and
//! counted, never overwritten.

use super::organize::{self, TagMerge, TagRelation};
use super::{new_id, LibraryError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Operation {
    TagItems {
        tag_id: String,
        /// The tag did not exist before; Undo deletes it if nothing else uses it.
        tag_created: bool,
        items: Vec<String>,
    },
    UntagItems {
        tag_id: String,
        relations: Vec<TagRelation>,
    },
    AddToCollection {
        collection_id: String,
        items: Vec<String>,
    },
    RemoveFromCollection {
        collection_id: String,
        rows: Vec<(String, i64)>,
    },
    SetFavorite {
        value: bool,
        items: Vec<String>,
    },
    SetArchived {
        /// The `archived_at` the operation wrote (`None` = unarchived).
        at: Option<i64>,
        rows: Vec<(String, Option<i64>)>,
    },
    Remove {
        at: i64,
        items: Vec<String>,
    },
    RenameTag {
        tag_id: String,
        old_name: String,
        new_name: String,
    },
    MergeTags(TagMerge),
    MergeItems(super::merge::ItemMerge),
}

impl Operation {
    pub fn kind(&self) -> &'static str {
        match self {
            Operation::TagItems { .. } => "tag_items",
            Operation::UntagItems { .. } => "untag_items",
            Operation::AddToCollection { .. } => "add_to_collection",
            Operation::RemoveFromCollection { .. } => "remove_from_collection",
            Operation::SetFavorite { .. } => "set_favorite",
            Operation::SetArchived { .. } => "set_archived",
            Operation::Remove { .. } => "remove",
            Operation::RenameTag { .. } => "rename_tag",
            Operation::MergeTags(_) => "merge_tags",
            Operation::MergeItems(_) => "merge_items",
        }
    }

    /// Records the operation changed; 0 means there is nothing to undo.
    pub fn size(&self) -> usize {
        match self {
            Operation::TagItems { items, .. }
            | Operation::AddToCollection { items, .. }
            | Operation::SetFavorite { items, .. }
            | Operation::Remove { items, .. } => items.len(),
            Operation::UntagItems { relations, .. } => relations.len(),
            Operation::RemoveFromCollection { rows, .. } => rows.len(),
            Operation::SetArchived { rows, .. } => rows.len(),
            Operation::RenameTag { .. } | Operation::MergeTags(_) | Operation::MergeItems(_) => 1,
        }
    }
}

/// Logs `op` for `session` when it changed anything. Returns the log id.
pub fn record(
    c: &Connection,
    session: &str,
    op: &Operation,
    now_ms: i64,
) -> Result<Option<String>> {
    if op.size() == 0 {
        return Ok(None);
    }
    let id = new_id();
    c.execute(
        "INSERT INTO operation_log (id, session, kind, payload, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            id,
            session,
            op.kind(),
            serde_json::to_string(op).expect("operation serializes"),
            now_ms
        ],
    )?;
    // Older rows can never be undone again; keep the log small.
    c.execute(
        "DELETE FROM operation_log WHERE seq < (SELECT max(seq) FROM operation_log) - 100",
        [],
    )?;
    Ok(Some(id))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Undoable {
    pub id: String,
    pub kind: String,
    pub size: usize,
    pub created_at: i64,
}

/// `(id, session, payload, created_at, undone_at)` of the newest log row.
type LogRow = (String, String, String, i64, Option<i64>);

fn newest(c: &Connection) -> Result<Option<LogRow>> {
    Ok(c.query_row(
        "SELECT id, session, payload, created_at, undone_at FROM operation_log ORDER BY seq DESC LIMIT 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )
    .optional()?)
}

/// The operation Undo would revert now, if any.
pub fn peek(c: &Connection, session: &str) -> Result<Option<Undoable>> {
    let Some((id, s, payload, created_at, undone)) = newest(c)? else {
        return Ok(None);
    };
    if s != session || undone.is_some() {
        return Ok(None);
    }
    let op: Operation = serde_json::from_str(&payload)
        .map_err(|_| LibraryError::Invalid("operation unreadable"))?;
    Ok(Some(Undoable {
        id,
        kind: op.kind().to_owned(),
        size: op.size(),
        created_at,
    }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoReport {
    pub kind: String,
    /// Records put back.
    pub applied: usize,
    /// Records left alone because they were changed since (§7.3).
    pub skipped: usize,
    /// Items whose list row may have changed (the UI reloads; search docs
    /// were already rewritten).
    pub items: Vec<String>,
}

/// Reverts the newest operation of `session`. `Ok(None)`: nothing to undo.
pub fn undo(c: &Connection, session: &str, now_ms: i64) -> Result<Option<UndoReport>> {
    let Some(u) = peek(c, session)? else {
        return Ok(None);
    };
    let payload: String = c.query_row(
        "SELECT payload FROM operation_log WHERE id = ?1",
        [&u.id],
        |r| r.get(0),
    )?;
    let op: Operation = serde_json::from_str(&payload)
        .map_err(|_| LibraryError::Invalid("operation unreadable"))?;
    let (applied, skipped, items) = apply(c, &op, now_ms)?;
    c.execute(
        "UPDATE operation_log SET undone_at = ?1 WHERE id = ?2",
        params![now_ms, u.id],
    )?;
    Ok(Some(UndoReport {
        kind: u.kind,
        applied,
        skipped,
        items,
    }))
}

fn apply(c: &Connection, op: &Operation, now_ms: i64) -> Result<(usize, usize, Vec<String>)> {
    Ok(match op {
        Operation::TagItems {
            tag_id,
            tag_created,
            items,
        } => {
            let mut n = 0;
            for id in items {
                n += c.execute(
                    "DELETE FROM library_item_tag WHERE item_id = ?1 AND tag_id = ?2",
                    params![id, tag_id],
                )?;
            }
            super::repo::reindex_items(c, items)?;
            if *tag_created {
                c.execute(
                    "DELETE FROM tag WHERE id = ?1 AND NOT EXISTS (SELECT 1 FROM library_item_tag WHERE tag_id = ?1)",
                    [tag_id],
                )?;
            }
            (n, items.len() - n, items.clone())
        }
        Operation::UntagItems { tag_id, relations } => {
            let n = organize::restore_tag_relations(c, tag_id, relations)?;
            let ids = relations.iter().map(|r| r.item_id.clone()).collect();
            (n, relations.len() - n, ids)
        }
        Operation::AddToCollection {
            collection_id,
            items,
        } => {
            let mut n = 0;
            for id in items {
                n += c.execute(
                    "DELETE FROM library_item_collection WHERE item_id = ?1 AND collection_id = ?2",
                    params![id, collection_id],
                )?;
            }
            (n, items.len() - n, items.clone())
        }
        Operation::RemoveFromCollection {
            collection_id,
            rows,
        } => {
            let n = organize::restore_memberships(c, collection_id, rows)?;
            (
                n,
                rows.len() - n,
                rows.iter().map(|r| r.0.clone()).collect(),
            )
        }
        Operation::SetFavorite { value, items } => {
            let mut n = 0;
            for id in items {
                // Only where the value is still the one we wrote.
                n += c.execute(
                    "UPDATE library_item SET is_favorite = ?1, revision = revision + 1, updated_at = ?2 \
                     WHERE id = ?3 AND is_favorite = ?4",
                    params![!value, now_ms, id, value],
                )?;
            }
            (n, items.len() - n, items.clone())
        }
        Operation::SetArchived { at, rows } => {
            let mut n = 0;
            for (id, prev) in rows {
                n += c.execute(
                    "UPDATE library_item SET archived_at = ?1, revision = revision + 1, updated_at = ?2 \
                     WHERE id = ?3 AND archived_at IS ?4",
                    params![prev, now_ms, id, at],
                )?;
            }
            (
                n,
                rows.len() - n,
                rows.iter().map(|r| r.0.clone()).collect(),
            )
        }
        Operation::Remove { at, items } => {
            let mut n = 0;
            for id in items {
                n += c.execute(
                    "UPDATE library_item SET removed_at = NULL, revision = revision + 1, updated_at = ?1 \
                     WHERE id = ?2 AND removed_at = ?3",
                    params![now_ms, id, at],
                )?;
            }
            (n, items.len() - n, items.clone())
        }
        Operation::RenameTag {
            tag_id,
            old_name,
            new_name,
        } => {
            let current = organize::get_tag(c, tag_id)?;
            match current {
                Some(t) if t.name == *new_name => match organize::rename_tag(c, tag_id, old_name) {
                    Ok(_) => (1, 0, Vec::new()),
                    Err(LibraryError::Invalid(_)) => (0, 1, Vec::new()),
                    Err(e) => return Err(e),
                },
                _ => (0, 1, Vec::new()),
            }
        }
        Operation::MergeTags(m) => match organize::unmerge_tags(c, m) {
            Ok(()) => (
                1,
                0,
                m.source_relations
                    .iter()
                    .map(|r| r.item_id.clone())
                    .collect(),
            ),
            Err(LibraryError::Invalid(_)) => (0, 1, Vec::new()),
            Err(e) => return Err(e),
        },
        Operation::MergeItems(m) => {
            let skipped = super::merge::unmerge(c, m, now_ms)?;
            (
                usize::from(skipped == 0),
                skipped,
                vec![m.target_id.clone(), m.source_id.clone()],
            )
        }
    })
}
