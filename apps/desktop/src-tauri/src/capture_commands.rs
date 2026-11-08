//! Add URL (STATE.md F1.8; SPEC §7.1 CAP-01…CAP-06, CAP-08, §8.1, §9.3).
//!
//! `check_url` runs as the user types: it validates and normalizes offline
//! and looks up an existing item, but writes nothing. `save_url` saves the
//! bookmark (item and search document in one transaction, F1.2) or names the
//! item that already holds the content. `restore_item` brings back an item
//! from Recently removed when its URL is saved again (§7.3, AC-45).
//!
//! The URL the user typed is never echoed back, logged or put into an error:
//! only the canonical form, which carries no secret parameters (CAP-08).

use crate::library::{internal, LibraryState};
use crate::library_commands::{blocking, dto_of, LibraryItemDto};
use serde::Serialize;
use tauri::{AppHandle, Runtime};
use vv_domain::error::{DomainError, ErrorCode};
use vv_domain::normalize::{derived_title, normalize, Normalized, Shape};
use vv_domain::url_policy::validate_capture_url;
use vv_persistence::library::bookmark::{self, MatchedBy, SaveError, SaveOutcome};
use vv_persistence::library::repo::{self, LibraryItem};
use vv_persistence::library::{Clock, SourcePlatform, SystemClock};
use vv_persistence::rusqlite::{Connection, TransactionBehavior};

/// Longest input `check_url` and `save_url` accept; anything longer is not a
/// link someone pasted.
pub const MAX_URL_LEN: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExistingDto {
    pub item: LibraryItemDto,
    /// In Recently removed: the UI offers Restore / Cancel (AC-45).
    pub removed: bool,
    pub matched_by: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlCheck {
    pub platform: &'static str,
    /// `video`, `collection`, `short_link` or `unknown`.
    pub shape: &'static str,
    pub canonical_url: String,
    /// The honest title a new bookmark gets until metadata arrives (CAP-04).
    pub title: String,
    pub existing: Option<ExistingDto>,
    /// Non-blocking notices, e.g. `PLAYLIST_UNSUPPORTED`.
    pub warnings: Vec<ErrorCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveUrlResult {
    /// `created`, or `existing` when nothing was written (CAP-06).
    pub outcome: &'static str,
    pub item: LibraryItemDto,
    pub removed: bool,
    pub matched_by: Option<&'static str>,
    pub warnings: Vec<ErrorCode>,
}

fn shape_name(s: Shape) -> &'static str {
    match s {
        Shape::Video => "video",
        Shape::Collection => "collection",
        Shape::ShortLink => "short_link",
        Shape::Unknown => "unknown",
    }
}

fn matched_name(m: MatchedBy) -> &'static str {
    match m {
        MatchedBy::CanonicalUrl => "canonical_url",
        MatchedBy::Alias => "alias",
        MatchedBy::SourceId => "source_id",
    }
}

fn rejected(code: ErrorCode) -> DomainError {
    DomainError::new(code, "capture-rejected")
}

/// Offline validation and normalization, shared by both commands.
pub fn parse(input: &str) -> Result<Normalized, DomainError> {
    let input = input.trim();
    if input.is_empty() || input.len() > MAX_URL_LEN {
        return Err(rejected(ErrorCode::InvalidUrl));
    }
    let url = validate_capture_url(input).map_err(|e| rejected(ErrorCode::from(&e)))?;
    normalize(&url).map_err(|_| rejected(ErrorCode::UrlHasCredentials))
}

fn warnings_of(n: &Normalized) -> Vec<ErrorCode> {
    if n.shape == Shape::Collection {
        vec![ErrorCode::PlaylistUnsupported]
    } else {
        vec![]
    }
}

pub fn check(c: &Connection, input: &str) -> Result<UrlCheck, DomainError> {
    let n = parse(input)?;
    let existing = bookmark::find_existing(c, &n)
        .map_err(|_| internal("capture-lookup"))?
        .map(|(it, by)| {
            Ok::<_, DomainError>(ExistingDto {
                item: dto_of(c, &it)?,
                removed: it.removed_at.is_some(),
                matched_by: matched_name(by),
            })
        })
        .transpose()?;
    Ok(UrlCheck {
        platform: SourcePlatform::from(n.platform).as_str(),
        shape: shape_name(n.shape),
        canonical_url: n.canonical_url.clone(),
        title: derived_title(&n),
        existing,
        warnings: warnings_of(&n),
    })
}

pub fn save(
    c: &mut Connection,
    clock: &dyn Clock,
    input: &str,
) -> Result<SaveUrlResult, DomainError> {
    // Same checks as `save_bookmark`, so a refusal carries our diagnostic id
    // and never the input.
    parse(input)?;
    let res = bookmark::save_bookmark(c, clock, input.trim()).map_err(|e| match e {
        SaveError::Rejected(code) => rejected(code),
        SaveError::Storage(_) => internal("capture-save"),
    })?;
    let (outcome, item, matched_by) = match &res.outcome {
        SaveOutcome::Created(it) => ("created", it, None),
        SaveOutcome::Existing {
            item, matched_by, ..
        } => ("existing", item, Some(matched_name(*matched_by))),
    };
    Ok(SaveUrlResult {
        outcome,
        item: dto_of(c, item)?,
        removed: item.removed_at.is_some(),
        matched_by,
        warnings: res.warnings,
    })
}

/// Clears `removed_at`; notes, overrides and relations were never touched,
/// so they come back with the item (§7.3). Restoring a live item is a no-op.
pub fn restore(
    c: &mut Connection,
    clock: &dyn Clock,
    id: &str,
) -> Result<LibraryItemDto, DomainError> {
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| internal("restore-begin"))?;
    let it: LibraryItem = repo::get_item(&tx, id)
        .map_err(|_| internal("restore-read"))?
        .ok_or_else(|| internal("item-not-found"))?;
    let it = if it.removed_at.is_some() {
        repo::set_removed(&tx, id, it.revision, None, clock.now_ms())
            .map_err(|_| internal("restore-write"))?
    } else {
        it
    };
    let dto = dto_of(&tx, &it)?;
    tx.commit().map_err(|_| internal("restore-commit"))?;
    Ok(dto)
}

/// Validates the text in the Add URL field and finds the item that already
/// holds it. Writes nothing.
#[tauri::command]
pub async fn check_url<R: Runtime>(
    app: AppHandle<R>,
    url: String,
) -> Result<UrlCheck, DomainError> {
    blocking(app, move |st: &LibraryState| {
        let c = st.get()?.reader()?;
        check(&c, &url)
    })
    .await
}

/// Save Bookmark: works offline; metadata is fetched later (M2).
#[tauri::command]
pub async fn save_url<R: Runtime>(
    app: AppHandle<R>,
    url: String,
) -> Result<SaveUrlResult, DomainError> {
    blocking(app, move |st: &LibraryState| {
        st.get()?.write(|c| save(c, &SystemClock, &url))
    })
    .await
}

/// Restore from Recently removed (AC-45).
#[tauri::command]
pub async fn restore_item<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<LibraryItemDto, DomainError> {
    blocking(app, move |st: &LibraryState| {
        st.get()?.write(|c| restore(c, &SystemClock, &id))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use vv_persistence::library::FixedClock;
    use vv_persistence::{Database, OpenOptions};

    fn db(dir: &std::path::Path) -> Database {
        Database::open(&dir.join("library.sqlite"), &OpenOptions::default()).unwrap()
    }

    #[test]
    fn check_writes_nothing_and_finds_the_saved_item() {
        let d = tempfile::tempdir().unwrap();
        let mut db = db(d.path());
        let clock = FixedClock::new(1_000);
        let c = db.writer();
        let first = check(c, "  https://youtu.be/vvTestVid01  ").unwrap();
        assert_eq!(first.platform, "youtube");
        assert_eq!(first.shape, "video");
        assert_eq!(first.title, "YouTube · vvTestVid01");
        assert!(first.existing.is_none());
        assert_eq!(repo::count_items(c).unwrap(), 0);

        let saved = save(c, &clock, "https://youtu.be/vvTestVid01").unwrap();
        assert_eq!(saved.outcome, "created");
        assert_eq!(saved.item.title, "YouTube · vvTestVid01");
        assert_eq!(saved.item.local_state, "bookmark_only");

        let again = check(c, "https://www.youtube.com/watch?v=vvTestVid01&si=abc").unwrap();
        let ex = again.existing.unwrap();
        assert_eq!(ex.item.id, saved.item.id);
        assert!(!ex.removed);
        let dup = save(c, &clock, "https://www.youtube.com/watch?v=vvTestVid01").unwrap();
        assert_eq!(dup.outcome, "existing");
        assert_eq!(dup.item.id, saved.item.id);
        assert_eq!(repo::count_items(c).unwrap(), 1);
    }

    #[test]
    fn refusals_carry_the_code_and_never_the_input() {
        let d = tempfile::tempdir().unwrap();
        let mut db = db(d.path());
        let c = db.writer();
        for (input, code) in [
            ("", ErrorCode::InvalidUrl),
            ("not a url", ErrorCode::InvalidUrl),
            ("ftp://example.com/a.mp4", ErrorCode::InvalidUrl),
            (
                "https://user:hunter2@example.com/v",
                ErrorCode::UrlHasCredentials,
            ),
            ("http://127.0.0.1/v", ErrorCode::UrlBlocked),
        ] {
            let e = check(c, input).unwrap_err();
            assert_eq!(e.code, code, "{input:?}");
            let e = save(c, &FixedClock::new(1), input).unwrap_err();
            assert_eq!(e.code, code, "{input:?}");
            let json = serde_json::to_string(&e).unwrap();
            assert!(!json.contains("hunter2"), "{json}");
        }
        let long = format!("https://example.com/{}", "a".repeat(MAX_URL_LEN));
        assert_eq!(check(c, &long).unwrap_err().code, ErrorCode::InvalidUrl);
        assert_eq!(repo::count_items(c).unwrap(), 0);
    }

    #[test]
    fn playlists_save_with_a_warning() {
        let d = tempfile::tempdir().unwrap();
        let mut db = db(d.path());
        let c = db.writer();
        let r = save(
            c,
            &FixedClock::new(1),
            "https://www.youtube.com/playlist?list=PLvvTest",
        )
        .unwrap();
        assert_eq!(r.warnings, vec![ErrorCode::PlaylistUnsupported]);
        assert_eq!(
            serde_json::to_value(&r).unwrap()["warnings"][0],
            "PLAYLIST_UNSUPPORTED"
        );
    }

    #[test]
    fn removed_item_is_offered_back_and_restored_with_its_notes() {
        let d = tempfile::tempdir().unwrap();
        let mut db = db(d.path());
        let clock = FixedClock::new(1_000);
        let c = db.writer();
        let saved = save(c, &clock, "https://vk.com/video-1_2").unwrap();
        let it = repo::get_item(c, &saved.item.id).unwrap().unwrap();
        let it = repo::set_notes(c, &it.id, it.revision, "keep me", 2_000).unwrap();
        repo::set_removed(c, &it.id, it.revision, Some(3_000), 3_000).unwrap();

        let again = save(c, &clock, "https://vk.com/video-1_2").unwrap();
        assert_eq!(again.outcome, "existing");
        assert!(again.removed);
        assert!(
            check(c, "https://vk.com/video-1_2")
                .unwrap()
                .existing
                .unwrap()
                .removed
        );
        assert_eq!(repo::count_items(c).unwrap(), 1);

        let back = restore(c, &clock, &it.id).unwrap();
        assert_eq!(back.id, it.id);
        let it = repo::get_item(c, &it.id).unwrap().unwrap();
        assert!(it.removed_at.is_none());
        assert_eq!(it.user_notes, "keep me");
        // A second restore changes nothing.
        let rev = it.revision;
        restore(c, &clock, &it.id).unwrap();
        assert_eq!(repo::get_item(c, &it.id).unwrap().unwrap().revision, rev);
        assert!(restore(c, &clock, "00000000-0000-4000-8000-00000000ffff").is_err());
    }
}
