//! Parsing of `yt-dlp --dump-single-json` (DL-05).
//!
//! Only allowlisted fields are kept. Format and manifest URLs, HTTP headers
//! (which can carry cookies), cookies and the description are dropped here,
//! so signed media URLs cannot reach the database or the UI (SPEC §13.4: on
//! retry the source is resolved again, never read back from disk).
//! Every kept string is length-capped and the input size is capped.

use crate::pyjson::nullify_bare_tokens;
use serde::Serialize;
use serde_json::{Map, Value};
use vv_domain::error::ErrorCode;

/// Largest `-J` document accepted. Real YouTube pages are a few MB.
pub const MAX_METADATA_BYTES: usize = 32 * 1024 * 1024;
/// Most formats kept for one video.
pub const MAX_FORMATS: usize = 1_000;
const MAX_TITLE_CHARS: usize = 1_000;
const MAX_SHORT_CHARS: usize = 200;
const MAX_URL_CHARS: usize = 2_048;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VideoMetadata {
    pub id: String,
    pub title: String,
    /// yt-dlp extractor key, e.g. `Youtube`, `Generic`.
    pub extractor_key: String,
    /// The page URL yt-dlp resolved (not a media URL).
    pub webpage_url: Option<String>,
    pub duration_seconds: Option<f64>,
    pub uploader: Option<String>,
    pub channel: Option<String>,
    /// `YYYYMMDD` as yt-dlp reports it; validated, not converted.
    pub upload_date: Option<String>,
    pub timestamp: Option<i64>,
    pub is_live: Option<bool>,
    pub has_drm: bool,
    pub formats: Vec<Format>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Format {
    pub format_id: String,
    pub ext: Option<String>,
    pub protocol: Option<String>,
    /// `None` when the format has no video (yt-dlp's `"none"`).
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    /// Total bitrate, kbit/s.
    pub tbr: Option<f64>,
    pub filesize: Option<u64>,
    pub filesize_approx: Option<u64>,
    pub has_drm: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Extracted {
    Video(VideoMetadata),
    /// Playlist/channel URL: not processed in bulk (SPEC §6, `PLAYLIST_UNSUPPORTED`).
    Playlist {
        id: Option<String>,
        title: Option<String>,
        entry_count: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataError {
    TooLarge,
    /// Not JSON, or not an object. No input text is kept in the error.
    InvalidJson,
    MissingField(&'static str),
    /// `_type` was `url`/`url_transparent`: yt-dlp did not finish resolving.
    Unresolved,
}

impl MetadataError {
    pub fn code(&self) -> ErrorCode {
        ErrorCode::ExtractorFailed
    }
}

pub fn parse(stdout: &[u8]) -> Result<Extracted, MetadataError> {
    if stdout.len() > MAX_METADATA_BYTES {
        return Err(MetadataError::TooLarge);
    }
    let text = String::from_utf8_lossy(stdout);
    let v: Value = serde_json::from_str(nullify_bare_tokens(text.trim()).as_str())
        .map_err(|_| MetadataError::InvalidJson)?;
    let o = v.as_object().ok_or(MetadataError::InvalidJson)?;
    match o.get("_type").and_then(Value::as_str).unwrap_or("video") {
        "playlist" | "multi_video" => Ok(Extracted::Playlist {
            id: short(o, "id"),
            title: capped(o, "title", MAX_TITLE_CHARS),
            entry_count: o
                .get("entries")
                .and_then(Value::as_array)
                .map_or(0, Vec::len),
        }),
        "url" | "url_transparent" => Err(MetadataError::Unresolved),
        _ => video(o).map(Extracted::Video),
    }
}

fn video(o: &Map<String, Value>) -> Result<VideoMetadata, MetadataError> {
    let id = short(o, "id").ok_or(MetadataError::MissingField("id"))?;
    let title = capped(o, "title", MAX_TITLE_CHARS).unwrap_or_else(|| id.clone());
    let formats: Vec<Format> = o
        .get("formats")
        .and_then(Value::as_array)
        .map(|fs| {
            fs.iter()
                .filter_map(Value::as_object)
                .filter_map(format)
                .take(MAX_FORMATS)
                .collect()
        })
        .unwrap_or_default();
    Ok(VideoMetadata {
        extractor_key: short(o, "extractor_key").unwrap_or_default(),
        webpage_url: capped(o, "webpage_url", MAX_URL_CHARS)
            .filter(|u| u.starts_with("https://") || u.starts_with("http://")),
        duration_seconds: positive(o, "duration"),
        uploader: short(o, "uploader"),
        channel: short(o, "channel"),
        upload_date: short(o, "upload_date")
            .filter(|d| d.len() == 8 && d.bytes().all(|b| b.is_ascii_digit())),
        timestamp: o
            .get("timestamp")
            .and_then(Value::as_f64)
            .filter(|t| t.is_finite())
            .map(|t| t as i64),
        is_live: o.get("is_live").and_then(Value::as_bool),
        has_drm: o.get("_has_drm").and_then(Value::as_bool).unwrap_or(false)
            || formats.iter().any(|f| f.has_drm),
        id,
        title,
        formats,
    })
}

fn format(f: &Map<String, Value>) -> Option<Format> {
    let codec = |k| short(f, k).filter(|c| c != "none");
    Some(Format {
        format_id: short(f, "format_id")?,
        ext: short(f, "ext"),
        protocol: short(f, "protocol"),
        vcodec: codec("vcodec"),
        acodec: codec("acodec"),
        width: whole(f, "width").and_then(|w| u32::try_from(w).ok()),
        height: whole(f, "height").and_then(|h| u32::try_from(h).ok()),
        fps: positive(f, "fps"),
        tbr: positive(f, "tbr"),
        filesize: whole(f, "filesize"),
        filesize_approx: whole(f, "filesize_approx"),
        has_drm: f.get("has_drm").and_then(Value::as_bool).unwrap_or(false),
    })
}

fn capped(o: &Map<String, Value>, key: &str, max: usize) -> Option<String> {
    let s = o.get(key)?.as_str()?.trim();
    (!s.is_empty()).then(|| s.chars().filter(|c| !c.is_control()).take(max).collect())
}

fn short(o: &Map<String, Value>, key: &str) -> Option<String> {
    capped(o, key, MAX_SHORT_CHARS)
}

fn positive(o: &Map<String, Value>, key: &str) -> Option<f64> {
    o.get(key)?.as_f64().filter(|x| x.is_finite() && *x > 0.0)
}

fn whole(o: &Map<String, Value>, key: &str) -> Option<u64> {
    let x = o.get(key)?;
    x.as_u64().or_else(|| {
        x.as_f64()
            .filter(|f| f.is_finite() && *f >= 0.0 && f.fract() == 0.0)
            .map(|f| f as u64)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_oversized_and_non_objects() {
        assert_eq!(
            parse(&vec![b' '; MAX_METADATA_BYTES + 1]),
            Err(MetadataError::TooLarge)
        );
        assert_eq!(parse(b"[1,2]"), Err(MetadataError::InvalidJson));
        assert_eq!(
            parse(b"{\"title\":\"x\"}"),
            Err(MetadataError::MissingField("id"))
        );
        assert_eq!(
            parse(b"{\"_type\":\"url\",\"url\":\"https://x.test\"}"),
            Err(MetadataError::Unresolved)
        );
    }

    #[test]
    fn python_nan_does_not_break_parsing() {
        let Extracted::Video(v) = parse(br#"{"id":"a","title":"t","duration":NaN,"formats":[{"format_id":"1","fps":Infinity}]}"#).unwrap() else {
            panic!()
        };
        assert_eq!(v.duration_seconds, None);
        assert_eq!(v.formats[0].fps, None);
    }

    #[test]
    fn strips_control_characters_and_caps_title() {
        let t = format!("a\u{0007}b{}", "x".repeat(5_000));
        let doc = serde_json::json!({"id": "a", "title": t}).to_string();
        let Extracted::Video(v) = parse(doc.as_bytes()).unwrap() else {
            panic!()
        };
        assert!(v.title.starts_with("ab"));
        assert_eq!(v.title.chars().count(), MAX_TITLE_CHARS);
    }
}
