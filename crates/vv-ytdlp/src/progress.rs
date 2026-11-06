//! Parsing of our progress-template lines (SPEC §13.4).
//!
//! yt-dlp prints `vv-progress {...}` for download updates and
//! `vv-postprocess {...}` for post-processors. With `--quiet` the first go to
//! stdout and the second to stderr, so callers feed both streams here.
//!
//! §13.4 rule: when the total is unknown, percentage and ETA are `None`.
//! `total_bytes_estimate` is kept separately and never turned into a
//! percentage; for fragment downloads the percentage comes from fragments.

use crate::args::{POSTPROCESS_PREFIX, PROGRESS_PREFIX};
use crate::pyjson::nullify_bare_tokens;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Download,
    Postprocess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    Downloading,
    Finished,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PostprocessStatus {
    Started,
    Processing,
    Finished,
}

/// One download update, in SPEC §13.4 field names.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DownloadProgress {
    pub status: DownloadStatus,
    pub bytes_done: u64,
    pub bytes_total: Option<u64>,
    /// yt-dlp's guess for fragment downloads. Shown as "about", never as %.
    pub bytes_total_estimate: Option<u64>,
    /// Fragments completed / total, when yt-dlp downloads in fragments.
    pub units_done: Option<u64>,
    pub units_total: Option<u64>,
    pub percentage: Option<f64>,
    pub eta_seconds: Option<u64>,
    pub speed_bytes_per_second: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ProgressEvent {
    Download(DownloadProgress),
    Postprocess {
        /// yt-dlp class name without the `PP` suffix, e.g. `FixupM3u8`.
        postprocessor: String,
        status: PostprocessStatus,
    },
}

impl ProgressEvent {
    pub fn phase(&self) -> Phase {
        match self {
            ProgressEvent::Download(_) => Phase::Download,
            ProgressEvent::Postprocess { .. } => Phase::Postprocess,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressError {
    /// The line had our prefix but its body was not the expected object.
    Malformed,
    /// A status value this version does not know.
    UnknownStatus,
}

/// Parses one line. `Ok(None)` means "not a progress line" (ordinary output,
/// warnings, errors); `Err` means it had our prefix but did not parse, which
/// usually means a truncated line from a killed process.
pub fn parse_line(line: &str) -> Result<Option<ProgressEvent>, ProgressError> {
    let line = line.trim_end();
    if let Some(body) = line.strip_prefix(PROGRESS_PREFIX) {
        let v = object(body)?;
        return download(&v).map(|d| Some(ProgressEvent::Download(d)));
    }
    if let Some(body) = line.strip_prefix(POSTPROCESS_PREFIX) {
        let v = object(body)?;
        let status = match v.get("status").and_then(Value::as_str) {
            Some("started") => PostprocessStatus::Started,
            Some("processing") => PostprocessStatus::Processing,
            Some("finished") => PostprocessStatus::Finished,
            _ => return Err(ProgressError::UnknownStatus),
        };
        let postprocessor = v
            .get("postprocessor")
            .and_then(Value::as_str)
            .ok_or(ProgressError::Malformed)?
            .chars()
            .take(64)
            .collect();
        return Ok(Some(ProgressEvent::Postprocess {
            postprocessor,
            status,
        }));
    }
    Ok(None)
}

fn object(body: &str) -> Result<Value, ProgressError> {
    let v: Value =
        serde_json::from_str(&nullify_bare_tokens(body)).map_err(|_| ProgressError::Malformed)?;
    if v.is_object() {
        Ok(v)
    } else {
        Err(ProgressError::Malformed)
    }
}

fn download(v: &Value) -> Result<DownloadProgress, ProgressError> {
    let status = match v.get("status").and_then(Value::as_str) {
        Some("downloading") => DownloadStatus::Downloading,
        Some("finished") => DownloadStatus::Finished,
        Some("error") => DownloadStatus::Error,
        _ => return Err(ProgressError::UnknownStatus),
    };
    let bytes_done = count(v, "downloaded_bytes").unwrap_or(0);
    let bytes_total = count(v, "total_bytes").filter(|t| *t > 0);
    let bytes_total_estimate = count(v, "total_bytes_estimate").filter(|t| *t > 0);
    let units_total = count(v, "fragment_count").filter(|t| *t > 0);
    let units_done = units_total.and(count(v, "fragment_index"));

    let percentage = match (bytes_total, units_done, units_total) {
        (Some(total), _, _) => Some(ratio(bytes_done, total)),
        (None, Some(done), Some(total)) => Some(ratio(done, total)),
        _ => None,
    };
    let percentage = if status == DownloadStatus::Finished && bytes_total.is_some() {
        Some(100.0)
    } else {
        percentage
    };
    let eta_seconds = percentage.and(count(v, "eta"));
    let speed_bytes_per_second = v
        .get("speed")
        .and_then(Value::as_f64)
        .filter(|s| s.is_finite() && *s >= 0.0);

    Ok(DownloadProgress {
        status,
        bytes_done,
        bytes_total,
        bytes_total_estimate,
        units_done,
        units_total,
        percentage,
        eta_seconds,
        speed_bytes_per_second,
    })
}

/// A non-negative whole number; yt-dlp sometimes prints floats (`166756.0`).
fn count(v: &Value, key: &str) -> Option<u64> {
    let x = v.get(key)?;
    if let Some(n) = x.as_u64() {
        return Some(n);
    }
    let f = x.as_f64()?;
    (f.is_finite() && f >= 0.0 && f <= u64::MAX as f64).then_some(f.round() as u64)
}

fn ratio(done: u64, total: u64) -> f64 {
    ((done as f64 / total as f64) * 100.0).clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dl(body: &str) -> DownloadProgress {
        match parse_line(&format!("vv-progress {body}")).unwrap().unwrap() {
            ProgressEvent::Download(d) => d,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn ignores_other_lines() {
        assert_eq!(parse_line("[download] Destination: x.mp4"), Ok(None));
        assert_eq!(parse_line("WARNING: something"), Ok(None));
    }

    #[test]
    fn truncated_line_is_malformed_not_a_panic() {
        assert_eq!(
            parse_line("vv-progress {\"status\":\"downloading\",\"downloaded_b"),
            Err(ProgressError::Malformed)
        );
        assert_eq!(parse_line("vv-progress [1]"), Err(ProgressError::Malformed));
        assert_eq!(
            parse_line("vv-progress {\"status\":\"paused\"}"),
            Err(ProgressError::UnknownStatus)
        );
    }

    #[test]
    fn estimate_never_becomes_a_percentage() {
        let d = dl(
            r#"{"status":"downloading","downloaded_bytes":10,"total_bytes":NA,"total_bytes_estimate":100.0,"eta":5,"fragment_index":NA,"fragment_count":NA}"#,
        );
        assert_eq!(d.bytes_total, None);
        assert_eq!(d.bytes_total_estimate, Some(100));
        assert_eq!(d.percentage, None);
        assert_eq!(d.eta_seconds, None, "§13.4: no ETA without a total");
    }

    #[test]
    fn fragments_give_units_and_percentage() {
        let d = dl(
            r#"{"status":"downloading","downloaded_bytes":10,"total_bytes":NA,"fragment_index":1,"fragment_count":4,"eta":NA}"#,
        );
        assert_eq!((d.units_done, d.units_total), (Some(1), Some(4)));
        assert_eq!(d.percentage, Some(25.0));
    }
}
