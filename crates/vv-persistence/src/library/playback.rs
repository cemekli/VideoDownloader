//! Where the user stopped watching (SPEC PLAY-02, §11.4 `PlaybackState`;
//! STATE.md F2.4; AC-49).
//!
//! One row per item. Saving a position is not a user edit: it does not bump
//! the item's revision and is not undoable.

use super::*;
use rusqlite::{params, Connection, OptionalExtension};

/// Near the end counts as watched: no resume offer then (PLAY-02).
pub const COMPLETE_RATIO: f64 = 0.95;
/// ...or when less than this is left of a video of a minute or more.
pub const COMPLETE_TAIL_MS: i64 = 10_000;
/// Positions under this are "not started"; no resume offer.
pub const MIN_RESUME_MS: i64 = 5_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackState {
    pub item_id: String,
    pub asset_id: String,
    pub position_ms: i64,
    pub duration_ms: Option<i64>,
    pub last_played_at: i64,
    pub completed_at: Option<i64>,
}

impl PlaybackState {
    /// A resume offer makes sense: started, not finished.
    pub fn resumable(&self) -> bool {
        self.completed_at.is_none() && self.position_ms >= MIN_RESUME_MS
    }
}

fn is_complete(position_ms: i64, duration_ms: Option<i64>) -> bool {
    match duration_ms {
        Some(d) if d > 0 => {
            position_ms as f64 >= d as f64 * COMPLETE_RATIO
                || (d >= 6 * COMPLETE_TAIL_MS && d - position_ms <= COMPLETE_TAIL_MS)
        }
        _ => false,
    }
}

pub fn get(c: &Connection, item_id: &str) -> Result<Option<PlaybackState>> {
    Ok(c.query_row(
        "SELECT item_id, asset_id, position_ms, duration_ms, last_played_at, completed_at \
         FROM playback_state WHERE item_id = ?1",
        [item_id],
        |r| {
            Ok(PlaybackState {
                item_id: r.get(0)?,
                asset_id: r.get(1)?,
                position_ms: r.get(2)?,
                duration_ms: r.get(3)?,
                last_played_at: r.get(4)?,
                completed_at: r.get(5)?,
            })
        },
    )
    .optional()?)
}

pub fn all(c: &Connection) -> Result<Vec<PlaybackState>> {
    let mut stmt = c.prepare(
        "SELECT item_id, asset_id, position_ms, duration_ms, last_played_at, completed_at \
         FROM playback_state ORDER BY item_id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(PlaybackState {
            item_id: r.get(0)?,
            asset_id: r.get(1)?,
            position_ms: r.get(2)?,
            duration_ms: r.get(3)?,
            last_played_at: r.get(4)?,
            completed_at: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Records the position. The asset must be a video or audio file of this
/// item (its original or a preview made from it).
pub fn save(
    c: &Connection,
    item_id: &str,
    asset_id: &str,
    position_ms: i64,
    duration_ms: Option<i64>,
    now_ms: i64,
) -> Result<PlaybackState> {
    if position_ms < 0 || duration_ms.is_some_and(|d| d <= 0) {
        return Err(LibraryError::Invalid("playback position"));
    }
    let owner: Option<(String, AssetRole)> = c
        .query_row(
            "SELECT library_item_id, role FROM media_asset WHERE id = ?1",
            [asset_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    match owner {
        Some((owner, AssetRole::Original | AssetRole::Preview)) if owner == item_id => {}
        Some(_) => return Err(LibraryError::Invalid("asset of another item")),
        None => return Err(LibraryError::NotFound),
    }
    let position_ms = match duration_ms {
        Some(d) => position_ms.min(d),
        None => position_ms,
    };
    let completed = is_complete(position_ms, duration_ms).then_some(now_ms);
    c.execute(
        "INSERT INTO playback_state (item_id, asset_id, position_ms, duration_ms, last_played_at, completed_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT (item_id) DO UPDATE SET asset_id = excluded.asset_id, position_ms = excluded.position_ms, \
         duration_ms = excluded.duration_ms, last_played_at = excluded.last_played_at, \
         completed_at = CASE WHEN excluded.completed_at IS NULL THEN NULL \
                             ELSE coalesce(playback_state.completed_at, excluded.completed_at) END",
        params![item_id, asset_id, position_ms, duration_ms, now_ms, completed],
    )?;
    get(c, item_id)?.ok_or(LibraryError::NotFound)
}

/// Puts a row back as it was exported (portable import, §15).
pub fn restore(c: &Connection, s: &PlaybackState) -> Result<()> {
    c.execute(
        "INSERT OR REPLACE INTO playback_state (item_id, asset_id, position_ms, duration_ms, last_played_at, completed_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            s.item_id,
            s.asset_id,
            s.position_ms.max(0),
            s.duration_ms.filter(|d| *d > 0),
            s.last_played_at,
            s.completed_at
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_is_the_last_five_percent_or_ten_seconds() {
        assert!(!is_complete(50_000, Some(100_000)));
        assert!(is_complete(95_000, Some(100_000)));
        assert!(is_complete(3_590_000, Some(3_600_000)));
        assert!(!is_complete(3_000_000, Some(3_600_000)));
        assert!(!is_complete(10_000, None));
        assert!(
            !is_complete(0, Some(5_000)),
            "a short clip is not done at its start"
        );
    }
}
