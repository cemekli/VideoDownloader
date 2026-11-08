//! Organizing the library (STATE.md F2.1–F2.3; SPEC §7.3, §11.7; ADR-0015).
//!
//! Tags, collections, smart collections, bulk operations, Undo, Recently
//! removed, orphaned media and duplicate merge. Every undoable operation
//! writes its change and its `operation_log` row in one transaction, so
//! Undo can never see half an operation.
//!
//! A bulk result counts items three ways (ORG-05): `changed`, `unchanged`
//! (already in that state) and `failed` (gone or in Recently removed).

use crate::library::{delete_files, internal, FileDeletion, LibraryState};
use crate::library_commands::{blocking, dto_of, item_detail, ItemDetail, LibraryItemDto};
use serde::Serialize;
use tauri::{AppHandle, Runtime};
use vv_domain::error::{DomainError, ErrorCode};
use vv_persistence::library::organize::{self, ChildPolicy, SmartRules};
use vv_persistence::library::undo::{self, Operation};
use vv_persistence::library::{merge, removal, repo, Clock, LibraryError, SystemClock};
use vv_persistence::rusqlite::{Connection, TransactionBehavior};

/// Longest note the inspector saves.
pub const MAX_NOTES_CHARS: usize = 100_000;
/// Most items one bulk call takes.
pub const MAX_BULK: usize = 50_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoDto {
    /// `tag_items`, `set_archived`, `merge_items`… (see `undo::Operation`).
    pub kind: String,
    pub size: u32,
}

/// What a bulk (or single) operation did (ORG-05).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpResult {
    pub changed: u32,
    pub unchanged: u32,
    pub failed: u32,
    /// Set when this operation is now the one Undo reverts.
    pub undo: Option<UndoDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoResult {
    pub kind: String,
    pub applied: u32,
    /// Changed again since; left alone (§7.3).
    pub skipped: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionDto {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    /// 0 for a top-level collection.
    pub depth: u32,
    pub items: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartCollectionDto {
    pub id: String,
    pub name: String,
    pub rules: SmartRules,
    /// A tag or collection the rules name is gone: shown as broken, never
    /// as silently empty (§7.3).
    pub invalid: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagDto {
    pub id: String,
    pub name: String,
    pub items: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarCounts {
    pub inbox: i64,
    pub removed: i64,
    pub duplicates: i64,
    pub orphans: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sidebar {
    pub collections: Vec<CollectionDto>,
    pub smart_collections: Vec<SmartCollectionDto>,
    pub tags: Vec<TagDto>,
    pub counts: SidebarCounts,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    pub items: u32,
    /// Managed originals kept in Orphaned media.
    pub orphaned: u32,
    pub files: FileDeletion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateDto {
    pub id: String,
    pub reason: String,
    pub created_at: i64,
    pub a: LibraryItemDto,
    pub b: LibraryItemDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedRef {
    pub id: String,
    pub name: String,
}

/// What a merge will do, before it runs (ORG-11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergePreviewDto {
    pub target: LibraryItemDto,
    pub source: LibraryItemDto,
    pub notes_after: String,
    pub tags_to_add: Vec<TagRef>,
    pub collections_to_add: Vec<NamedRef>,
    /// File names only; paths never reach the webview.
    pub media_to_move: Vec<String>,
    pub alias_url: Option<String>,
    pub becomes_favorite: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrphanDto {
    pub id: String,
    pub file_name: String,
    pub root_label: String,
    pub root_status: String,
    pub size_bytes: Option<i64>,
    pub former_title: Option<String>,
    pub orphaned_at: i64,
}

pub mod ops {
    //! The command bodies as plain functions on a connection, for tests.
    use super::*;

    // --- Core (plain functions on a connection; the commands wrap them) --------

    /// `LibraryError` → `DomainError`. A rule the backend refuses (`Invalid`)
    /// keeps its reason as the diagnostic id; §19 has no validation code, so
    /// the UI prevents those inputs itself (ADR-0015).
    pub(crate) fn lib(id: &'static str) -> impl Fn(LibraryError) -> DomainError {
        move |e| match e {
            LibraryError::RevisionConflict { .. } => {
                DomainError::new(ErrorCode::RevisionConflict, id)
            }
            LibraryError::Invalid(reason) => internal(reason),
            LibraryError::NotFound => internal(&format!("{id}-not-found")),
            LibraryError::Sqlite(_) => internal(id),
        }
    }

    pub(crate) fn check_bulk(ids: &[String]) -> Result<(), DomainError> {
        if ids.len() > MAX_BULK {
            return Err(internal("bulk-too-large"));
        }
        Ok(())
    }

    pub(crate) fn unique(ids: &[String]) -> usize {
        ids.iter().collect::<std::collections::HashSet<_>>().len()
    }

    /// Logs `op` and builds the result counts.
    pub(crate) fn finish(
        c: &Connection,
        session: &str,
        now: i64,
        asked: usize,
        live: usize,
        changed: usize,
        op: Option<Operation>,
    ) -> Result<OpResult, DomainError> {
        let undo = match op {
            Some(op) => undo::record(c, session, &op, now)
                .map_err(lib("undo-record"))?
                .map(|_| UndoDto {
                    kind: op.kind().to_owned(),
                    size: op.size() as u32,
                }),
            None => None,
        };
        Ok(OpResult {
            changed: changed as u32,
            unchanged: live.saturating_sub(changed) as u32,
            failed: asked.saturating_sub(live) as u32,
            undo,
        })
    }

    pub(crate) fn live(c: &Connection, ids: &[String]) -> Result<usize, DomainError> {
        Ok(organize::live_ids(c, ids).map_err(lib("live-ids"))?.len())
    }

    pub fn tag_items(
        c: &Connection,
        s: &str,
        now: i64,
        ids: &[String],
        name: &str,
    ) -> Result<OpResult, DomainError> {
        check_bulk(ids)?;
        let n = live(c, ids)?;
        let (tag, tag_created) =
            organize::get_or_create_tag(c, name, now).map_err(lib("tag-create"))?;
        let items = organize::tag_items(c, &tag.id, ids, now).map_err(lib("tag-items"))?;
        let changed = items.len();
        let op = Operation::TagItems {
            tag_id: tag.id,
            tag_created,
            items,
        };
        finish(c, s, now, unique(ids), n, changed, Some(op))
    }

    pub fn untag_items(
        c: &Connection,
        s: &str,
        now: i64,
        ids: &[String],
        tag_id: &str,
    ) -> Result<OpResult, DomainError> {
        check_bulk(ids)?;
        let n = live(c, ids)?;
        let relations = organize::untag_items(c, tag_id, ids).map_err(lib("untag-items"))?;
        let changed = relations.len();
        let op = Operation::UntagItems {
            tag_id: tag_id.to_owned(),
            relations,
        };
        finish(c, s, now, unique(ids), n, changed, Some(op))
    }

    pub fn add_to_collection(
        c: &Connection,
        s: &str,
        now: i64,
        ids: &[String],
        collection_id: &str,
    ) -> Result<OpResult, DomainError> {
        check_bulk(ids)?;
        let n = live(c, ids)?;
        let items = organize::add_to_collection(c, collection_id, ids, now)
            .map_err(lib("collection-add"))?;
        let changed = items.len();
        let op = Operation::AddToCollection {
            collection_id: collection_id.to_owned(),
            items,
        };
        finish(c, s, now, unique(ids), n, changed, Some(op))
    }

    pub fn remove_from_collection(
        c: &Connection,
        s: &str,
        now: i64,
        ids: &[String],
        collection_id: &str,
    ) -> Result<OpResult, DomainError> {
        check_bulk(ids)?;
        let n = live(c, ids)?;
        let rows = organize::remove_from_collection(c, collection_id, ids)
            .map_err(lib("collection-remove"))?;
        let changed = rows.len();
        let op = Operation::RemoveFromCollection {
            collection_id: collection_id.to_owned(),
            rows,
        };
        finish(c, s, now, unique(ids), n, changed, Some(op))
    }

    pub fn set_favorite(
        c: &Connection,
        s: &str,
        now: i64,
        ids: &[String],
        value: bool,
    ) -> Result<OpResult, DomainError> {
        check_bulk(ids)?;
        let n = live(c, ids)?;
        let items = organize::set_favorite(c, ids, value, now).map_err(lib("set-favorite"))?;
        let changed = items.len();
        finish(
            c,
            s,
            now,
            unique(ids),
            n,
            changed,
            Some(Operation::SetFavorite { value, items }),
        )
    }

    pub fn set_archived(
        c: &Connection,
        s: &str,
        now: i64,
        ids: &[String],
        value: bool,
    ) -> Result<OpResult, DomainError> {
        check_bulk(ids)?;
        let n = live(c, ids)?;
        let at = value.then_some(now);
        let rows = organize::set_archived(c, ids, at, now).map_err(lib("set-archived"))?;
        let changed = rows.len();
        finish(
            c,
            s,
            now,
            unique(ids),
            n,
            changed,
            Some(Operation::SetArchived { at, rows }),
        )
    }

    pub fn remove_items(
        c: &Connection,
        s: &str,
        now: i64,
        ids: &[String],
    ) -> Result<OpResult, DomainError> {
        check_bulk(ids)?;
        let items = organize::remove_items(c, ids, now).map_err(lib("remove-items"))?;
        let changed = items.len();
        finish(
            c,
            s,
            now,
            unique(ids),
            changed,
            changed,
            Some(Operation::Remove { at: now, items }),
        )
    }

    /// Brings items back from Recently removed (not an Undo-logged operation).
    pub fn restore_items(
        c: &Connection,
        s: &str,
        now: i64,
        ids: &[String],
    ) -> Result<OpResult, DomainError> {
        check_bulk(ids)?;
        let rows = organize::restore_items(c, ids, now).map_err(lib("restore-items"))?;
        finish(c, s, now, unique(ids), rows.len(), rows.len(), None)
    }

    pub fn rename_tag(
        c: &Connection,
        s: &str,
        now: i64,
        id: &str,
        name: &str,
    ) -> Result<OpResult, DomainError> {
        let old = organize::rename_tag(c, id, name).map_err(lib("rename-tag"))?;
        let new = organize::get_tag(c, id)
            .map_err(lib("rename-tag"))?
            .ok_or_else(|| internal("rename-tag-not-found"))?;
        let changed = usize::from(old.name != new.name);
        let op = (changed == 1).then(|| Operation::RenameTag {
            tag_id: id.to_owned(),
            old_name: old.name,
            new_name: new.name,
        });
        finish(c, s, now, 1, 1, changed, op)
    }

    pub fn merge_tags(
        c: &Connection,
        s: &str,
        now: i64,
        source_id: &str,
        target_id: &str,
    ) -> Result<OpResult, DomainError> {
        let m = organize::merge_tags(c, source_id, target_id).map_err(lib("merge-tags"))?;
        finish(c, s, now, 1, 1, 1, Some(Operation::MergeTags(m)))
    }

    /// Item merge (ORG-11): the source goes to Recently removed; undoable.
    pub fn merge_items(
        c: &Connection,
        s: &str,
        now: i64,
        candidate_id: &str,
        target_id: &str,
    ) -> Result<OpResult, DomainError> {
        let m = merge::merge(c, candidate_id, target_id, now).map_err(lib("merge-items"))?;
        finish(c, s, now, 2, 2, 2, Some(Operation::MergeItems(m)))
    }

    pub fn run_undo(c: &Connection, s: &str, now: i64) -> Result<Option<UndoResult>, DomainError> {
        Ok(undo::undo(c, s, now)
            .map_err(lib("undo"))?
            .map(|r| UndoResult {
                kind: r.kind,
                applied: r.applied as u32,
                skipped: r.skipped as u32,
            }))
    }

    pub(crate) fn count(c: &Connection, sql: &str) -> Result<i64, DomainError> {
        c.query_row(sql, [], |r| r.get(0))
            .map_err(|_| internal("sidebar-count"))
    }

    pub fn sidebar(c: &Connection) -> Result<Sidebar, DomainError> {
        let mut depths: std::collections::HashMap<String, u32> = Default::default();
        let collections = organize::list_collections(c)
            .map_err(lib("sidebar-collections"))?
            .into_iter()
            .map(|x| {
                // Parents come first (depth-first order).
                let depth = x
                    .collection
                    .parent_id
                    .as_ref()
                    .and_then(|p| depths.get(p))
                    .map_or(0, |d| d + 1);
                depths.insert(x.collection.id.clone(), depth);
                CollectionDto {
                    id: x.collection.id,
                    name: x.collection.name,
                    parent_id: x.collection.parent_id,
                    depth,
                    items: x.items,
                }
            })
            .collect();
        let smart_collections = organize::list_smart_collections(c)
            .map_err(lib("sidebar-smart"))?
            .into_iter()
            .map(|x| {
                Ok(SmartCollectionDto {
                    invalid: x.rules.missing_refs(c).map_err(lib("sidebar-smart"))? > 0,
                    id: x.id,
                    name: x.name,
                    rules: x.rules,
                })
            })
            .collect::<Result<_, DomainError>>()?;
        let tags = organize::list_tags(c)
            .map_err(lib("sidebar-tags"))?
            .into_iter()
            .map(|t| TagDto {
                id: t.tag.id,
                name: t.tag.name,
                items: t.items,
            })
            .collect();
        let counts = SidebarCounts {
            inbox: count(
                c,
                "SELECT count(*) FROM library_item i WHERE i.removed_at IS NULL AND i.archived_at IS NULL \
                 AND NOT EXISTS (SELECT 1 FROM library_item_collection m WHERE m.item_id = i.id)",
            )?,
            removed: count(c, "SELECT count(*) FROM library_item WHERE removed_at IS NOT NULL")?,
            duplicates: organize::open_duplicate_count(c).map_err(lib("sidebar-duplicates"))?,
            orphans: count(c, "SELECT count(*) FROM orphan_media")?,
        };
        Ok(Sidebar {
            collections,
            smart_collections,
            tags,
            counts,
        })
    }

    pub fn duplicates(c: &Connection) -> Result<Vec<DuplicateDto>, DomainError> {
        merge::open_candidates(c)
            .map_err(lib("duplicates"))?
            .into_iter()
            .map(|d| {
                Ok(DuplicateDto {
                    a: dto_of(c, &d.a)?,
                    b: dto_of(c, &d.b)?,
                    id: d.id,
                    reason: d.reason,
                    created_at: d.created_at,
                })
            })
            .collect()
    }

    pub(crate) fn file_name(storage_ref: &str) -> String {
        storage_ref
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(storage_ref)
            .to_owned()
    }

    pub fn merge_preview(
        c: &Connection,
        candidate_id: &str,
        target_id: &str,
    ) -> Result<MergePreviewDto, DomainError> {
        let p = merge::preview(c, candidate_id, target_id).map_err(lib("merge-preview"))?;
        Ok(MergePreviewDto {
            target: dto_of(c, &p.target)?,
            source: dto_of(c, &p.source)?,
            notes_after: p.notes_after,
            tags_to_add: p
                .tags_to_add
                .into_iter()
                .map(|t| TagRef {
                    id: t.id,
                    name: t.name,
                })
                .collect(),
            collections_to_add: p
                .collections_to_add
                .into_iter()
                .map(|x| NamedRef {
                    id: x.id,
                    name: x.name,
                })
                .collect(),
            media_to_move: p
                .assets_to_move
                .iter()
                .map(|a| file_name(&a.storage_ref))
                .collect(),
            alias_url: p.alias_url,
            becomes_favorite: p.becomes_favorite,
        })
    }

    pub fn orphans(c: &Connection) -> Result<Vec<OrphanDto>, DomainError> {
        Ok(removal::list_orphans(c)
            .map_err(lib("orphans"))?
            .into_iter()
            .map(|o| OrphanDto {
                file_name: file_name(&o.storage_ref),
                id: o.id,
                root_label: o.root_label,
                root_status: o.root_status,
                size_bytes: o.size_bytes,
                former_title: o.former_item_title,
                orphaned_at: o.orphaned_at,
            })
            .collect())
    }
}

use ops::*;

// --- Command plumbing -------------------------------------------------------

/// Runs `f` in one IMMEDIATE transaction on the writer, with this app run's
/// Undo session and the current time.
async fn write<R: Runtime, T: Send + 'static>(
    app: AppHandle<R>,
    f: impl FnOnce(&Connection, &str, i64) -> Result<T, DomainError> + Send + 'static,
) -> Result<T, DomainError> {
    blocking(app, move |st: &LibraryState| {
        let session = st.session().to_owned();
        st.get()?.write(|c| {
            let tx = c
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|_| internal("tx-begin"))?;
            let r = f(&tx, &session, SystemClock.now_ms())?;
            tx.commit().map_err(|_| internal("tx-commit"))?;
            Ok(r)
        })
    })
    .await
}

async fn read<R: Runtime, T: Send + 'static>(
    app: AppHandle<R>,
    f: impl FnOnce(&Connection) -> Result<T, DomainError> + Send + 'static,
) -> Result<T, DomainError> {
    blocking(app, move |st| {
        let c = st.get()?.reader()?;
        f(&c)
    })
    .await
}

// --- Commands ----------------------------------------------------------------

/// Collections, smart collections, tags and view counts for the sidebar.
#[tauri::command]
pub async fn get_sidebar<R: Runtime>(app: AppHandle<R>) -> Result<Sidebar, DomainError> {
    read(app, sidebar).await
}

/// Reverts the newest operation of this app run; `null` when there is none.
#[tauri::command]
pub async fn undo<R: Runtime>(app: AppHandle<R>) -> Result<Option<UndoResult>, DomainError> {
    write(app, run_undo).await
}

#[tauri::command]
pub async fn tag_items<R: Runtime>(
    app: AppHandle<R>,
    item_ids: Vec<String>,
    name: String,
) -> Result<OpResult, DomainError> {
    write(app, move |c, s, now| {
        ops::tag_items(c, s, now, &item_ids, &name)
    })
    .await
}

#[tauri::command]
pub async fn untag_items<R: Runtime>(
    app: AppHandle<R>,
    item_ids: Vec<String>,
    tag_id: String,
) -> Result<OpResult, DomainError> {
    write(app, move |c, s, now| {
        ops::untag_items(c, s, now, &item_ids, &tag_id)
    })
    .await
}

#[tauri::command]
pub async fn add_to_collection<R: Runtime>(
    app: AppHandle<R>,
    item_ids: Vec<String>,
    collection_id: String,
) -> Result<OpResult, DomainError> {
    write(app, move |c, s, now| {
        ops::add_to_collection(c, s, now, &item_ids, &collection_id)
    })
    .await
}

#[tauri::command]
pub async fn remove_from_collection<R: Runtime>(
    app: AppHandle<R>,
    item_ids: Vec<String>,
    collection_id: String,
) -> Result<OpResult, DomainError> {
    write(app, move |c, s, now| {
        ops::remove_from_collection(c, s, now, &item_ids, &collection_id)
    })
    .await
}

#[tauri::command]
pub async fn set_favorite<R: Runtime>(
    app: AppHandle<R>,
    item_ids: Vec<String>,
    value: bool,
) -> Result<OpResult, DomainError> {
    write(app, move |c, s, now| {
        ops::set_favorite(c, s, now, &item_ids, value)
    })
    .await
}

#[tauri::command]
pub async fn set_archived<R: Runtime>(
    app: AppHandle<R>,
    item_ids: Vec<String>,
    value: bool,
) -> Result<OpResult, DomainError> {
    write(app, move |c, s, now| {
        ops::set_archived(c, s, now, &item_ids, value)
    })
    .await
}

/// Moves items to Recently removed (undoable).
#[tauri::command]
pub async fn remove_items<R: Runtime>(
    app: AppHandle<R>,
    item_ids: Vec<String>,
) -> Result<OpResult, DomainError> {
    write(app, move |c, s, now| {
        ops::remove_items(c, s, now, &item_ids)
    })
    .await
}

#[tauri::command]
pub async fn restore_items<R: Runtime>(
    app: AppHandle<R>,
    item_ids: Vec<String>,
) -> Result<OpResult, DomainError> {
    write(app, move |c, s, now| {
        ops::restore_items(c, s, now, &item_ids)
    })
    .await
}

/// Deletes items in Recently removed for good. Managed originals become
/// orphaned media unless `delete_media` (the user's explicit consent);
/// files are deleted only after the commit. Not undoable (§7.3).
#[tauri::command]
pub async fn delete_items_permanently<R: Runtime>(
    app: AppHandle<R>,
    item_ids: Vec<String>,
    delete_media: bool,
) -> Result<DeleteResult, DomainError> {
    blocking(app, move |st| {
        check_bulk(&item_ids)?;
        st.get()?.write(|c| {
            let tx = c
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|_| internal("tx-begin"))?;
            let r = removal::delete_many(&tx, &item_ids, delete_media, SystemClock.now_ms())
                .map_err(lib("delete-permanently"))?;
            tx.commit().map_err(|_| internal("tx-commit"))?;
            Ok(DeleteResult {
                items: r.items as u32,
                orphaned: r.orphaned as u32,
                files: delete_files(c, &r.files_to_delete),
            })
        })
    })
    .await
}

#[tauri::command]
pub async fn rename_tag<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    name: String,
) -> Result<OpResult, DomainError> {
    write(app, move |c, s, now| ops::rename_tag(c, s, now, &id, &name)).await
}

#[tauri::command]
pub async fn merge_tags<R: Runtime>(
    app: AppHandle<R>,
    source_id: String,
    target_id: String,
) -> Result<OpResult, DomainError> {
    write(app, move |c, s, now| {
        ops::merge_tags(c, s, now, &source_id, &target_id)
    })
    .await
}

/// Deletes a tag and its links; items stay. Not undoable: the UI confirms.
#[tauri::command]
pub async fn delete_tag<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<OpResult, DomainError> {
    write(app, move |c, s, now| {
        let n = organize::delete_tag(c, &id).map_err(lib("delete-tag"))?;
        finish(c, s, now, n, n, n, None)
    })
    .await
}

#[tauri::command]
pub async fn create_collection<R: Runtime>(
    app: AppHandle<R>,
    name: String,
    parent_id: Option<String>,
) -> Result<NamedRef, DomainError> {
    write(app, move |c, _, now| {
        let x = organize::create_collection(c, &name, parent_id.as_deref(), now)
            .map_err(lib("create-collection"))?;
        Ok(NamedRef {
            id: x.id,
            name: x.name,
        })
    })
    .await
}

#[tauri::command]
pub async fn rename_collection<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    name: String,
) -> Result<(), DomainError> {
    write(app, move |c, _, now| {
        organize::rename_collection(c, &id, &name, now).map_err(lib("rename-collection"))
    })
    .await
}

/// The keyboard alternative to dragging in the tree (ORG-08).
#[tauri::command]
pub async fn move_collection<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    parent_id: Option<String>,
) -> Result<(), DomainError> {
    write(app, move |c, _, now| {
        organize::move_collection(c, &id, parent_id.as_deref(), now).map_err(lib("move-collection"))
    })
    .await
}

/// Items and files are never touched (ORG-01, ORG-10). Returns how many
/// collections went.
#[tauri::command]
pub async fn delete_collection<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    policy: ChildPolicy,
) -> Result<u32, DomainError> {
    write(app, move |c, _, now| {
        Ok(
            organize::delete_collection(c, &id, policy, now).map_err(lib("delete-collection"))?
                as u32,
        )
    })
    .await
}

/// Creates (`id` null) or replaces a smart collection (ORG-07).
#[tauri::command]
pub async fn save_smart_collection<R: Runtime>(
    app: AppHandle<R>,
    id: Option<String>,
    name: String,
    rules: SmartRules,
) -> Result<NamedRef, DomainError> {
    write(app, move |c, _, now| {
        let x = organize::save_smart_collection(c, id.as_deref(), &name, &rules, now)
            .map_err(lib("save-smart-collection"))?;
        Ok(NamedRef {
            id: x.id,
            name: x.name,
        })
    })
    .await
}

#[tauri::command]
pub async fn delete_smart_collection<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<(), DomainError> {
    write(app, move |c, _, _| {
        organize::delete_smart_collection(c, &id).map_err(lib("delete-smart-collection"))
    })
    .await
}

/// Saves the user's note if the item is still at `revision`; otherwise
/// REVISION_CONFLICT and nothing is written (ORG-03).
#[tauri::command]
pub async fn set_notes<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    revision: i64,
    notes: String,
) -> Result<ItemDetail, DomainError> {
    write(app, move |c, _, now| {
        if notes.chars().count() > MAX_NOTES_CHARS {
            return Err(internal("notes-too-long"));
        }
        repo::set_notes(c, &id, revision, &notes, now).map_err(lib("set-notes"))?;
        item_detail(c, &id)
    })
    .await
}

#[tauri::command]
pub async fn list_duplicates<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Vec<DuplicateDto>, DomainError> {
    read(app, duplicates).await
}

#[tauri::command]
pub async fn preview_merge<R: Runtime>(
    app: AppHandle<R>,
    candidate_id: String,
    target_id: String,
) -> Result<MergePreviewDto, DomainError> {
    read(app, move |c| merge_preview(c, &candidate_id, &target_id)).await
}

#[tauri::command]
pub async fn merge_items<R: Runtime>(
    app: AppHandle<R>,
    candidate_id: String,
    target_id: String,
) -> Result<OpResult, DomainError> {
    write(app, move |c, s, now| {
        ops::merge_items(c, s, now, &candidate_id, &target_id)
    })
    .await
}

/// "Not a duplicate".
#[tauri::command]
pub async fn dismiss_duplicate<R: Runtime>(
    app: AppHandle<R>,
    candidate_id: String,
) -> Result<(), DomainError> {
    write(app, move |c, _, now| {
        merge::dismiss(c, &candidate_id, now).map_err(lib("dismiss-duplicate"))
    })
    .await
}

#[tauri::command]
pub async fn list_orphans<R: Runtime>(app: AppHandle<R>) -> Result<Vec<OrphanDto>, DomainError> {
    read(app, orphans).await
}

/// A new `local_video` item for the file where it is (AC-53).
#[tauri::command]
pub async fn reimport_orphan<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<LibraryItemDto, DomainError> {
    write(app, move |c, _, now| {
        let it = removal::reimport_orphan(c, &id, now).map_err(lib("reimport-orphan"))?;
        dto_of(c, &it)
    })
    .await
}

/// Forgets an orphan and deletes its file (the user confirmed).
#[tauri::command]
pub async fn delete_orphan<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<FileDeletion, DomainError> {
    blocking(app, move |st| {
        st.get()?.write(|c| {
            let tx = c
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|_| internal("tx-begin"))?;
            let f = removal::delete_orphan(&tx, &id).map_err(lib("delete-orphan"))?;
            tx.commit().map_err(|_| internal("tx-commit"))?;
            Ok(delete_files(c, &[f]))
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::ops::{
        add_to_collection, remove_items, rename_tag, set_archived, set_favorite, tag_items,
    };
    use super::*;
    use vv_persistence::library::repo::LibraryItem;
    use vv_persistence::library::{ItemKind, SourcePlatform};
    use vv_persistence::{Database, OpenOptions};

    fn db(dir: &std::path::Path) -> Database {
        Database::open(&dir.join("library.sqlite"), &OpenOptions::default()).unwrap()
    }

    fn items(c: &Connection, n: usize) -> Vec<String> {
        (0..n)
            .map(|i| {
                let mut it =
                    LibraryItem::new(ItemKind::Bookmark, SourcePlatform::Youtube, 10 + i as i64);
                it.title = Some(format!("Öğe {i}"));
                it.canonical_url = Some(format!("https://www.youtube.com/watch?v=org{i:08}"));
                repo::insert_item(c, &it).unwrap();
                it.id
            })
            .collect()
    }

    #[test]
    fn bulk_counts_changed_unchanged_and_failed() {
        let d = tempfile::tempdir().unwrap();
        let mut db = db(d.path());
        let c = db.writer();
        let ids = items(c, 4);
        set_favorite(c, "s", 100, &ids[..1], true).unwrap();
        organize::remove_items(c, &ids[3..], 100).unwrap();
        let mut asked = ids.clone();
        asked.push(ids[0].clone());
        asked.push("missing".into());
        let r = set_favorite(c, "s", 200, &asked, true).unwrap();
        assert_eq!((r.changed, r.unchanged, r.failed), (2, 1, 2));
        assert_eq!(
            r.undo,
            Some(UndoDto {
                kind: "set_favorite".into(),
                size: 2
            })
        );

        let u = run_undo(c, "s", 300).unwrap().unwrap();
        assert_eq!(
            (u.kind.as_str(), u.applied, u.skipped),
            ("set_favorite", 2, 0)
        );
        assert!(run_undo(c, "s", 400).unwrap().is_none());
        // Another app run never undoes this run's operations.
        set_archived(c, "s", 500, &ids[..2], true).unwrap();
        assert!(run_undo(c, "other", 600).unwrap().is_none());
    }

    #[test]
    fn sidebar_has_tree_depths_counts_and_broken_rules() {
        let d = tempfile::tempdir().unwrap();
        let mut db = db(d.path());
        let c = db.writer();
        let ids = items(c, 3);
        let top = organize::create_collection(c, "Gezi", None, 1).unwrap();
        let sub = organize::create_collection(c, "Deniz", Some(&top.id), 1).unwrap();
        add_to_collection(c, "s", 2, &ids[..1], &sub.id).unwrap();
        tag_items(c, "s", 3, &ids, "yaz").unwrap();
        remove_items(c, "s", 4, &ids[2..]).unwrap();
        let tag = organize::tag_by_name(c, "yaz").unwrap().unwrap();
        let rules = SmartRules {
            tag_ids: vec![tag.id.clone()],
            ..Default::default()
        };
        organize::save_smart_collection(c, None, "Yaz", &rules, 5).unwrap();

        let s = sidebar(c).unwrap();
        assert_eq!(
            s.collections
                .iter()
                .map(|x| (x.name.as_str(), x.depth, x.items))
                .collect::<Vec<_>>(),
            [("Gezi", 0, 0), ("Deniz", 1, 1)]
        );
        assert_eq!(s.tags[0].items, 2);
        assert_eq!(
            s.counts,
            SidebarCounts {
                inbox: 1,
                removed: 1,
                duplicates: 0,
                orphans: 0
            }
        );
        assert!(!s.smart_collections[0].invalid);

        organize::delete_tag(c, &tag.id).unwrap();
        assert!(sidebar(c).unwrap().smart_collections[0].invalid);
    }

    #[test]
    fn rename_to_same_name_logs_nothing_and_taken_name_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let mut db = db(d.path());
        let c = db.writer();
        let ids = items(c, 1);
        tag_items(c, "s", 1, &ids, "deniz").unwrap();
        tag_items(c, "s", 2, &ids, "dağ").unwrap();
        let t = organize::tag_by_name(c, "deniz").unwrap().unwrap();
        let r = rename_tag(c, "s", 3, &t.id, "deniz").unwrap();
        assert_eq!((r.changed, r.undo.is_none()), (0, true));
        let e = rename_tag(c, "s", 4, &t.id, "DAĞ").unwrap_err();
        assert_eq!(e.diagnostic_id, "tag-name-taken");
    }
}
