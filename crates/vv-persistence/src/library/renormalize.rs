//! Recompute stored canonical URLs after a normalization rule change
//! (SPEC §11.7, AC-44, STATE.md F1.3).
//!
//! Items whose `normalization_version` is older than
//! [`NORMALIZATION_VERSION`] are visited oldest `added_at` first. An item
//! takes its new value when no other item holds it. Otherwise nothing is
//! merged: the item keeps its old `canonical_url` and version, gets
//! `normalization_conflict = 1` and an open
//! `DuplicateCandidate(normalization_conflict)` with the holder. Because the
//! oldest item is visited first, it is the one that gets a contested value.
//! One transaction: either the whole pass lands or none of it does.
//!
//! Edge case (ADR-0011): when the holder is itself stale but its stored value
//! already equals the new value, it cannot "keep the old value" without
//! breaking the unique index, so the holder keeps it and the visited item is
//! the one marked as the conflict, even if it is older.

use super::repo::{self, LibraryItem};
use super::{Clock, DuplicateReason, LibraryError, SourcePlatform};
use rusqlite::{params, Connection, TransactionBehavior};
use vv_domain::normalize::{normalize, Normalized, NORMALIZATION_VERSION};
use vv_domain::url_policy::validate_capture_url;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenormalizeReport {
    /// Took a new canonical URL / source id.
    pub updated: usize,
    /// Same value under the new rules; only the version moved.
    pub unchanged: usize,
    /// Kept the old value; marked and paired with the holder.
    pub conflicts: usize,
    /// Stored URL no longer passes policy/normalization; left untouched.
    pub skipped: usize,
}

fn holder(c: &Connection, me: &LibraryItem, n: &Normalized) -> super::Result<Option<LibraryItem>> {
    let by_url = repo::item_by_canonical_url(c, &n.canonical_url)?
        .or(repo::item_by_alias(c, &n.canonical_url)?);
    if let Some(h) = by_url.filter(|h| h.id != me.id) {
        return Ok(Some(h));
    }
    if let Some(id) = &n.source_id {
        if let Some(h) = repo::item_by_source(c, n.platform.into(), id)?.filter(|h| h.id != me.id) {
            return Ok(Some(h));
        }
    }
    Ok(None)
}

pub fn renormalize(c: &mut Connection, clock: &dyn Clock) -> super::Result<RenormalizeReport> {
    let now = clock.now_ms();
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let ids: Vec<String> = {
        let mut s = tx.prepare(
            "SELECT id FROM library_item WHERE kind <> 'local_video' AND normalization_version < ?1 \
             AND normalization_conflict = 0 ORDER BY added_at, seq",
        )?;
        let v = s
            .query_map([NORMALIZATION_VERSION], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        v
    };
    let mut report = RenormalizeReport::default();
    for id in ids {
        let me = repo::get_item(&tx, &id)?.ok_or(LibraryError::NotFound)?;
        let n = me
            .canonical_url
            .as_deref()
            .and_then(|u| validate_capture_url(u).ok())
            .and_then(|u| normalize(&u).ok());
        let Some(n) = n else {
            report.skipped += 1;
            continue;
        };
        if let Some(h) = holder(&tx, &me, &n)? {
            tx.execute(
                "UPDATE library_item SET normalization_conflict = 1, revision = revision + 1, updated_at = ?1 WHERE id = ?2",
                params![now, me.id],
            )?;
            repo::open_duplicate_candidate(
                &tx,
                &h.id,
                &me.id,
                DuplicateReason::NormalizationConflict,
                now,
            )?;
            report.conflicts += 1;
            continue;
        }
        let platform = SourcePlatform::from(n.platform);
        let same = me.canonical_url.as_deref() == Some(n.canonical_url.as_str())
            && me.source_platform == platform
            && me.source_id == n.source_id;
        tx.execute(
            "UPDATE library_item SET canonical_url = ?1, source_platform = ?2, source_id = ?3, \
             normalization_version = ?4, revision = revision + 1, updated_at = ?5 WHERE id = ?6",
            params![
                n.canonical_url,
                platform,
                n.source_id,
                n.version,
                now,
                me.id
            ],
        )?;
        if same {
            report.unchanged += 1;
        } else {
            // The derived title may change with the canonical URL.
            let it = repo::get_item(&tx, &me.id)?.ok_or(LibraryError::NotFound)?;
            repo::write_search_document(&tx, &it)?;
            report.updated += 1;
        }
    }
    tx.commit()?;
    Ok(report)
}
