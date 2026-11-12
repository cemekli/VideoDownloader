//! Markdown note of an item: title, source link, the user's notes and the
//! transcript moments the user picked (STATE.md F6.5; SPEC DATA-11, §8.4
//! step 6).
//!
//! - Every timecode comes from a stored segment: the webview names segment
//!   ids, never times or text, so a note cannot carry an invented moment.
//! - All segments must belong to transcripts of that item; an unknown id
//!   refuses the whole note instead of leaving a gap.
//! - Text from sites and transcripts is escaped, so a Markdown viewer shows
//!   it as text (no HTML, no links that were not in the library).

use crate::library::repo;
use crate::library::SourcePlatform;
use crate::transcripts::{Provenance, TranscriptError};
use rusqlite::{Connection, OptionalExtension};

/// Most moments in one note.
pub const MAX_MOMENTS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// A file name from the title: letters, digits, spaces and `-` only.
    pub file_name: String,
    pub markdown: String,
    pub moments: usize,
}

/// `1:02:05`, `3:07`, `0:03` from milliseconds (rounded down to a second,
/// so the link never starts after the words).
pub fn timecode(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Text as Markdown shows it literally: no tags, emphasis, links or
/// headings from the source.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '|' | '!' | '~' => {
                out.push('\\');
                out.push(c);
            }
            '\r' => {}
            c => out.push(c),
        }
    }
    out
}

/// A link to the moment on the source page, where the site has a time
/// parameter we know (YouTube `t=`); otherwise `None`.
fn moment_link(platform: SourcePlatform, url: &str, ms: i64) -> Option<String> {
    if platform != SourcePlatform::Youtube || !safe_url(url) {
        return None;
    }
    let sep = if url.contains('?') { '&' } else { '?' };
    Some(format!("{url}{sep}t={}s", ms.max(0) / 1000))
}

/// An `https` URL that fits in `<…>` as one Markdown autolink.
fn safe_url(u: &str) -> bool {
    u.starts_with("https://")
        && !u
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>'))
}

fn file_name(title: &str) -> String {
    let base: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let base: String = base.chars().take(80).collect();
    let base = base.trim();
    format!(
        "{} - note.md",
        if base.is_empty() { "VideoVault" } else { base }
    )
}

fn source_label(p: Provenance) -> &'static str {
    match p {
        Provenance::SourceCaptions => "site captions",
        Provenance::GeneratedLocal => "transcribed on this PC",
        Provenance::Imported => "imported",
    }
}

/// Builds the note. `segment_ids` may come in any order and repeat; the
/// note lists them once, in time order. `today` is shown as written on.
pub fn build(
    c: &Connection,
    item_id: &str,
    segment_ids: &[String],
    today: &str,
) -> Result<Note, TranscriptError> {
    if segment_ids.len() > MAX_MOMENTS {
        return Err(TranscriptError::Invalid("too many moments".into()));
    }
    let it = repo::get_item(c, item_id)
        .map_err(|_| TranscriptError::NotFound)?
        .filter(|i| i.removed_at.is_none())
        .ok_or(TranscriptError::NotFound)?;
    let mut rows = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for id in segment_ids {
        if !seen.insert(id.as_str()) {
            continue;
        }
        let row: Option<(i64, i64, String, String, String)> = c
            .query_row(
                "SELECT s.start_ms, s.end_ms, s.text, t.language, t.provenance \
                 FROM transcript_segment s JOIN transcript t ON t.id = s.transcript_id \
                 WHERE s.id = ?1 AND t.item_id = ?2",
                [id, item_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?;
        rows.push(row.ok_or(TranscriptError::NotFound)?);
    }
    rows.sort_by_key(|r| (r.0, r.1));

    let title = it.display_title();
    let text = repo::search_text(&it);
    let url = it.canonical_url.clone().filter(|u| safe_url(u));
    let mut md = format!("# {}\n\n", escape(&title));
    if let Some(u) = &url {
        md.push_str(&format!("Source: <{u}>\n\n"));
    }
    if !text.creator.is_empty() {
        md.push_str(&format!("By {}\n\n", escape(&text.creator)));
    }
    md.push_str(&format!("Written with VideoVault on {today}.\n"));
    if !it.user_notes.trim().is_empty() {
        md.push_str("\n## Notes\n\n");
        md.push_str(&escape(it.user_notes.trim()));
        md.push('\n');
    }
    if !rows.is_empty() {
        md.push_str("\n## Moments\n\n");
        let mut last_source = None;
        for (start, _end, said, lang, prov) in &rows {
            let source = format!(
                "{lang}, {}",
                source_label(Provenance::parse(prov).unwrap_or(Provenance::Imported))
            );
            if last_source.as_ref() != Some(&source) {
                md.push_str(&format!("Transcript: {}\n\n", escape(&source)));
                last_source = Some(source);
            }
            let tc = timecode(*start);
            let stamp = match url
                .as_deref()
                .and_then(|u| moment_link(it.source_platform, u, *start))
            {
                Some(l) => format!("[{tc}](<{l}>)"),
                None => format!("**{tc}**"),
            };
            md.push_str(&format!("- {stamp} {}\n", escape(said)));
        }
    }
    Ok(Note {
        file_name: file_name(&title),
        markdown: md,
        moments: rows.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timecodes_and_escaping() {
        assert_eq!(timecode(3_040), "0:03");
        assert_eq!(timecode(187_999), "3:07");
        assert_eq!(timecode(3_725_000), "1:02:05");
        assert_eq!(timecode(-5), "0:00");
        assert_eq!(
            escape("<img src=x> *a* [l](u) # h"),
            "\\<img src=x\\> \\*a\\* \\[l\\](u) \\# h"
        );
        assert_eq!(
            file_name("Ders 4: Replikasyon / giriş?"),
            "Ders 4 Replikasyon giriş - note.md"
        );
        assert_eq!(file_name("../.."), "VideoVault - note.md");
        assert_eq!(
            moment_link(
                SourcePlatform::Youtube,
                "https://www.youtube.com/watch?v=a",
                12_760
            )
            .unwrap(),
            "https://www.youtube.com/watch?v=a&t=12s"
        );
        assert!(moment_link(SourcePlatform::Vk, "https://vk.com/video1", 1).is_none());
    }
}
