//! Save Bookmark (SPEC §7.1 CAP-02…CAP-06, CAP-08; §11.7; STATE.md F1.2).
//!
//! Works offline: URL policy and normalization need no network. The item and
//! its search document are written in one `BEGIN IMMEDIATE` transaction, so
//! a concurrent save of the same content waits for the first and then finds
//! it. If two writers still race past the lookup (another process, or a
//! connection without the busy timeout), the unique indexes stop the second
//! insert and the save returns the item that won (AC-03).

use super::repo::{self, LibraryItem};
use super::{Clock, ItemKind, LibraryError, MetadataStatus, SourcePlatform};
use rusqlite::{Connection, TransactionBehavior};
use vv_domain::error::ErrorCode;
use vv_domain::normalize::{normalize, Normalized, Shape};
use vv_domain::url_policy::validate_capture_url;

/// How an existing item was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchedBy {
    CanonicalUrl,
    Alias,
    SourceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    Created(LibraryItem),
    /// Nothing was written; the UI routes to this item (CAP-06). When
    /// `item.removed_at` is set it offers Restore / Cancel (§7.3, AC-45).
    Existing {
        item: LibraryItem,
        matched_by: MatchedBy,
        /// Other items of open duplicate candidates, shown alongside (§11.7).
        open_duplicates: Vec<String>,
    },
}

impl SaveOutcome {
    pub fn item(&self) -> &LibraryItem {
        match self {
            SaveOutcome::Created(i) | SaveOutcome::Existing { item: i, .. } => i,
        }
    }
    pub fn is_created(&self) -> bool {
        matches!(self, SaveOutcome::Created(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveResult {
    pub outcome: SaveOutcome,
    /// Non-blocking notices, e.g. `PLAYLIST_UNSUPPORTED` for a playlist,
    /// channel or profile saved as a single bookmark (§6).
    pub warnings: Vec<ErrorCode>,
}

#[derive(Debug)]
pub enum SaveError {
    /// The input was refused before anything was written. Carries the §19
    /// code only: the rejected URL itself is never echoed (CAP-08).
    Rejected(ErrorCode),
    Storage(LibraryError),
}

impl SaveError {
    pub fn code(&self) -> ErrorCode {
        match self {
            SaveError::Rejected(c) => *c,
            SaveError::Storage(e) => e.code(),
        }
    }
}

impl From<LibraryError> for SaveError {
    fn from(e: LibraryError) -> Self {
        SaveError::Storage(e)
    }
}

impl From<rusqlite::Error> for SaveError {
    fn from(e: rusqlite::Error) -> Self {
        SaveError::Storage(e.into())
    }
}

/// Looks up the item that already holds this content: normalized URL first,
/// then aliases, then platform + source id (§11.7, CAP-06).
pub fn find_existing(
    c: &Connection,
    n: &Normalized,
) -> super::Result<Option<(LibraryItem, MatchedBy)>> {
    let alias = repo::item_by_alias(c, &n.canonical_url)?;
    if let Some(i) = repo::item_by_canonical_url(c, &n.canonical_url)? {
        // A merged-away item keeps its URL while it sits in Recently
        // removed; the URL now belongs to the merge target (ORG-11).
        match alias {
            Some(a) if i.removed_at.is_some() && a.removed_at.is_none() => {
                return Ok(Some((a, MatchedBy::Alias)))
            }
            _ => return Ok(Some((i, MatchedBy::CanonicalUrl))),
        }
    }
    if let Some(i) = alias {
        return Ok(Some((i, MatchedBy::Alias)));
    }
    if let Some(id) = &n.source_id {
        if let Some(i) = repo::item_by_source(c, n.platform.into(), id)? {
            return Ok(Some((i, MatchedBy::SourceId)));
        }
    }
    Ok(None)
}

fn existing(
    c: &Connection,
    item: LibraryItem,
    matched_by: MatchedBy,
) -> super::Result<SaveOutcome> {
    let open_duplicates = repo::open_duplicates_for_item(c, &item.id)?
        .into_iter()
        .map(|d| {
            if d.item_a_id == item.id {
                d.item_b_id
            } else {
                d.item_a_id
            }
        })
        .collect();
    Ok(SaveOutcome::Existing {
        item,
        matched_by,
        open_duplicates,
    })
}

/// Validates, normalizes and saves `input` as a bookmark.
pub fn save_bookmark(
    c: &mut Connection,
    clock: &dyn Clock,
    input: &str,
) -> Result<SaveResult, SaveError> {
    let url = validate_capture_url(input).map_err(|e| SaveError::Rejected(ErrorCode::from(&e)))?;
    // A temporary secret without a canonical form is refused before any
    // write (AC-52); the URL is not logged or returned.
    let n = normalize(&url).map_err(|_| SaveError::Rejected(ErrorCode::UrlHasCredentials))?;
    let warnings = if n.shape == Shape::Collection {
        vec![ErrorCode::PlaylistUnsupported]
    } else {
        vec![]
    };

    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some((item, by)) = find_existing(&tx, &n)? {
        let outcome = existing(&tx, item, by)?;
        tx.commit()?;
        return Ok(SaveResult { outcome, warnings });
    }
    let item = new_bookmark(&n, clock.now_ms());
    match repo::insert_item(&tx, &item) {
        Ok(()) => {
            tx.commit()?;
            Ok(SaveResult {
                outcome: SaveOutcome::Created(item),
                warnings,
            })
        }
        Err(e) if e.is_unique_violation() => {
            drop(tx); // rolls back the failed statement's transaction
            Ok(SaveResult {
                outcome: resolve_lost_race(c, &n)?,
                warnings,
            })
        }
        Err(e) => Err(e.into()),
    }
}

/// After a unique violation, the winner is committed and visible.
pub(crate) fn resolve_lost_race(c: &Connection, n: &Normalized) -> Result<SaveOutcome, SaveError> {
    match find_existing(c, n)? {
        Some((item, by)) => Ok(existing(c, item, by)?),
        // A unique index fired but nothing matches: a bug, never a silent duplicate.
        None => Err(SaveError::Storage(LibraryError::Invalid(
            "unique violation without a matching item",
        ))),
    }
}

pub(crate) fn new_bookmark(n: &Normalized, now_ms: i64) -> LibraryItem {
    let mut it = LibraryItem::new(ItemKind::Bookmark, SourcePlatform::from(n.platform), now_ms);
    it.canonical_url = Some(n.canonical_url.clone());
    it.original_url = Some(n.redacted_original.clone());
    it.normalization_version = n.version;
    it.source_id = n.source_id.clone();
    it.metadata_status = MetadataStatus::Pending;
    it
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::FixedClock;

    /// The fallback path: the lookup missed, the insert hit a unique index.
    #[test]
    fn unique_violation_resolves_to_the_winner() {
        let mut c = Connection::open_in_memory().unwrap();
        c.pragma_update(None, "foreign_keys", true).unwrap();
        crate::migrations::Migrator::new(crate::migrations::MIGRATIONS)
            .unwrap()
            .migrate_up(&mut c)
            .unwrap();
        let clock = FixedClock::new(1_000);
        let first = save_bookmark(&mut c, &clock, "https://youtu.be/vvTestVid01").unwrap();
        assert!(first.outcome.is_created());

        let n = normalize(
            &validate_capture_url("https://www.youtube.com/watch?v=vvTestVid01&si=x").unwrap(),
        )
        .unwrap();
        let err = repo::insert_item(&c, &new_bookmark(&n, 2_000)).unwrap_err();
        assert!(err.is_unique_violation(), "{err}");
        match resolve_lost_race(&c, &n).unwrap() {
            SaveOutcome::Existing {
                item, matched_by, ..
            } => {
                assert_eq!(item.id, first.outcome.item().id);
                assert_eq!(matched_by, MatchedBy::CanonicalUrl);
            }
            o => panic!("{o:?}"),
        }
        assert_eq!(repo::count_items(&c).unwrap(), 1);
    }
}
