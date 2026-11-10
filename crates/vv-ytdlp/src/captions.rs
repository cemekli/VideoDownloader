//! Site captions (STATE.md F6.2; SPEC AI-A01, §8.4, §11.5; ADR-0027).
//!
//! Two things live here, both without a process:
//!
//! - [`tracks`] reads which caption tracks a video offers from the `-J`
//!   document: only the track names (`en`, `tr`, `en-orig`), never their
//!   URLs. [`choose`] picks the one track worth keeping.
//! - [`parse_vtt`] turns a WebVTT file into timed cues of plain text. Tags,
//!   styling and voice spans are dropped and entities decoded; the text is
//!   data, shown as text, never as markup (§20, threat model "Metadata/
//!   altyazı"). The rolling lines of automatic captions (each cue repeats
//!   the line before it) are folded so every spoken line is kept once.

use serde::Serialize;
use serde_json::{Map, Value};

/// Most tracks of one kind kept from a `-J` document.
pub const MAX_TRACKS: usize = 300;
/// Largest caption file read. A two-hour talk is well under 1 MiB.
pub const MAX_VTT_BYTES: usize = 16 * 1024 * 1024;
/// Most cues kept from one file.
pub const MAX_CUES: usize = 50_000;
/// Most characters kept from one cue.
pub const MAX_CUE_CHARS: usize = 2_000;

/// Track names that are not speech (chat replays, comments).
const NOT_SPEECH: &[&str] = &["live_chat", "rechat", "danmaku", "comments"];

/// Suffix yt-dlp gives YouTube's automatic track in the spoken language
/// (the others are machine translations of it).
pub const ORIGINAL_SUFFIX: &str = "-orig";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CaptionTracks {
    /// The spoken language yt-dlp reports for the video, if any.
    pub language: Option<String>,
    /// Tracks the uploader provided (`subtitles`), sorted.
    pub manual: Vec<String>,
    /// Tracks the site generated (`automatic_captions`), sorted.
    pub automatic: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptionKind {
    Manual,
    Automatic,
}

/// The track to fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// The track name as yt-dlp knows it (passed to `--sub-langs`).
    pub track: String,
    /// The language stored with the transcript (`-orig` removed).
    pub language: String,
    pub kind: CaptionKind,
}

/// A name that is safe to hand to `--sub-langs` as it is: yt-dlp reads that
/// value as a list of regular expressions, and a leading `-` excludes.
/// Letters, digits, `-` and `_` only, starting with a letter or digit.
pub fn valid_track(s: &str) -> bool {
    (2..=35).contains(&s.len())
        && s.as_bytes()[0].is_ascii_alphanumeric()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn names(o: &Map<String, Value>, key: &str) -> Vec<String> {
    let mut v: Vec<String> = o
        .get(key)
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter(|(_, formats)| formats.as_array().is_some_and(|a| !a.is_empty()))
                .map(|(k, _)| k.clone())
                .filter(|k| valid_track(k) && !NOT_SPEECH.contains(&k.as_str()))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v.truncate(MAX_TRACKS);
    v
}

/// The caption tracks of a `-J` video object.
pub fn tracks(o: &Map<String, Value>) -> CaptionTracks {
    CaptionTracks {
        language: o
            .get("language")
            .and_then(Value::as_str)
            .filter(|l| valid_track(l))
            .map(str::to_owned),
        manual: names(o, "subtitles"),
        automatic: names(o, "automatic_captions"),
    }
}

/// `en-US` -> `en`.
fn base(s: &str) -> String {
    s.split(['-', '_']).next().unwrap_or(s).to_ascii_lowercase()
}

fn language_of(track: &str) -> String {
    track
        .strip_suffix(ORIGINAL_SUFFIX)
        .unwrap_or(track)
        .to_owned()
}

/// Picks the track that best stands for what is said in the video:
///
/// 1. the uploader's track in the spoken language;
/// 2. the site's automatic track in the spoken language (`-orig`, or the
///    spoken language itself);
/// 3. an uploader track in one of `prefer` (in that order), then any
///    uploader track.
///
/// Automatic tracks in other languages are machine translations and are
/// never picked. `None`: nothing worth keeping, local transcription is the
/// way (F6.2b).
pub fn choose(t: &CaptionTracks, prefer: &[&str]) -> Option<Choice> {
    let pick = |track: &String, kind| Choice {
        track: track.clone(),
        language: language_of(track),
        kind,
    };
    let spoken = t.language.as_deref().map(base);
    if let Some(l) = &spoken {
        if let Some(m) = t.manual.iter().find(|m| &base(m) == l) {
            return Some(pick(m, CaptionKind::Manual));
        }
    }
    let orig: Vec<&String> = t
        .automatic
        .iter()
        .filter(|a| a.ends_with(ORIGINAL_SUFFIX))
        .collect();
    let auto = match (&spoken, orig.as_slice()) {
        (Some(l), _) => orig
            .iter()
            .find(|a| &base(a) == l)
            .copied()
            .or_else(|| t.automatic.iter().find(|a| *a == l))
            .or_else(|| t.automatic.iter().find(|a| &base(a) == l)),
        // The spoken language is unknown: only an `-orig` track says which
        // one is not a translation, and only when there is one.
        (None, [one]) => Some(*one),
        (None, _) => None,
    };
    if let Some(a) = auto {
        return Some(pick(a, CaptionKind::Automatic));
    }
    for p in prefer {
        if let Some(m) = t.manual.iter().find(|m| base(m) == *p) {
            return Some(pick(m, CaptionKind::Manual));
        }
    }
    t.manual.first().map(|m| pick(m, CaptionKind::Manual))
}

/// One cue of a caption file, as plain text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VttError {
    TooLarge,
    /// No `WEBVTT` header.
    NotVtt,
}

/// `hh:mm:ss.mmm` or `mm:ss.mmm` (a `,` is read as `.`) to milliseconds.
fn timestamp(s: &str) -> Option<i64> {
    let (clock, frac) = s
        .trim()
        .replace(',', ".")
        .split_once('.')
        .map(|(a, b)| (a.to_owned(), b.to_owned()))?;
    if frac.len() != 3 || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let parts: Vec<&str> = clock.split(':').collect();
    let num = |p: &str| -> Option<i64> {
        (!p.is_empty() && p.len() <= 9 && p.bytes().all(|b| b.is_ascii_digit()))
            .then(|| p.parse().ok())
            .flatten()
    };
    let (h, m, sec) = match parts.as_slice() {
        [m, s] => (0, num(m)?, num(s)?),
        [h, m, s] => (num(h)?, num(m)?, num(s)?),
        _ => return None,
    };
    if m > 59 || sec > 59 {
        return None;
    }
    Some(((h * 60 + m) * 60 + sec) * 1000 + frac.parse::<i64>().ok()?)
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest[..rest.len().min(12)].find(';');
        let decoded = end.and_then(|e| {
            let name = &rest[1..e];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                "lrm" | "rlm" => None,
                _ => {
                    let n = if let Some(h) = name.strip_prefix("#x").or(name.strip_prefix("#X")) {
                        u32::from_str_radix(h, 16).ok()
                    } else {
                        name.strip_prefix('#').and_then(|d| d.parse().ok())
                    };
                    return n
                        .and_then(char::from_u32)
                        .filter(|c| !c.is_control())
                        .map(|c| (Some(c), e));
                }
            };
            if c.is_none() && !matches!(name, "lrm" | "rlm") {
                return None;
            }
            Some((c, e))
        });
        match decoded {
            Some((c, e)) => {
                if let Some(c) = c {
                    out.push(c);
                }
                rest = &rest[e + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// One text line without tags, entities decoded, whitespace collapsed.
fn clean_line(line: &str) -> String {
    let mut no_tags = String::with_capacity(line.len());
    let mut in_tag = false;
    for c in line.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => no_tags.push(c),
            _ => {}
        }
    }
    decode_entities(&no_tags)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_CUE_CHARS)
        .collect()
}

/// Parses a WebVTT file into cues sorted by start time.
///
/// - A cue whose times do not parse, or whose end is not after its start,
///   is left out (times are never guessed).
/// - A line that was already on screen in the cue before is not repeated:
///   automatic captions roll, so every cue shows the previous line again.
/// - Cues left without text are dropped.
pub fn parse_vtt(bytes: &[u8]) -> Result<Vec<Cue>, VttError> {
    if bytes.len() > MAX_VTT_BYTES {
        return Err(VttError::TooLarge);
    }
    let text = String::from_utf8_lossy(bytes);
    let text = text
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    if !text.starts_with("WEBVTT") {
        return Err(VttError::NotVtt);
    }
    let mut cues = Vec::new();
    let mut prev: Vec<String> = Vec::new();
    for block in text.split("\n\n") {
        if cues.len() >= MAX_CUES {
            break;
        }
        let lines: Vec<&str> = block.lines().collect();
        let Some(at) = lines.iter().position(|l| l.contains("-->")) else {
            continue;
        };
        if lines.first().is_some_and(|l| {
            l.starts_with("NOTE") || l.starts_with("STYLE") || l.starts_with("REGION")
        }) {
            continue;
        }
        let (a, b) = lines[at].split_once("-->").unwrap_or_default();
        let b = b.split_whitespace().next().unwrap_or("");
        let (Some(start_ms), Some(end_ms)) = (timestamp(a), timestamp(b)) else {
            continue;
        };
        let shown: Vec<String> = lines[at + 1..]
            .iter()
            .map(|l| clean_line(l))
            .filter(|l| !l.is_empty())
            .collect();
        let new: Vec<&String> = shown.iter().filter(|l| !prev.contains(l)).collect();
        if start_ms < end_ms && !new.is_empty() {
            let text: String = new
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(MAX_CUE_CHARS)
                .collect();
            cues.push(Cue {
                start_ms,
                end_ms,
                text,
            });
        }
        prev = shown;
    }
    cues.sort_by_key(|c| c.start_ms);
    Ok(cues)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(lang: Option<&str>, manual: &[&str], automatic: &[&str]) -> CaptionTracks {
        CaptionTracks {
            language: lang.map(str::to_owned),
            manual: manual.iter().map(|s| s.to_string()).collect(),
            automatic: automatic.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn track_names_are_safe_for_sub_langs() {
        assert!(valid_track("en"));
        assert!(valid_track("zh-Hans"));
        assert!(valid_track("en-orig"));
        assert!(!valid_track("-en"));
        assert!(!valid_track("e"));
        assert!(!valid_track("en.*"));
        assert!(!valid_track("en,tr"));
        assert!(!valid_track("a"));
    }

    #[test]
    fn tracks_keep_names_only_and_skip_chat() {
        let v: Value = serde_json::json!({
            "language": "tr",
            "subtitles": {
                "tr": [{"ext": "vtt", "url": "https://x/secret"}],
                "live_chat": [{"ext": "json"}],
                "en": [],
                "bad lang": [{"ext": "vtt"}]
            },
            "automatic_captions": {"tr-orig": [{"ext": "vtt"}], "en": [{"ext": "vtt"}]}
        });
        let got = tracks(v.as_object().unwrap());
        assert_eq!(got, t(Some("tr"), &["tr"], &["en", "tr-orig"]));
    }

    #[test]
    fn manual_in_the_spoken_language_wins() {
        let c = choose(
            &t(Some("en"), &["de", "en-US"], &["en-orig", "en"]),
            &["tr"],
        )
        .unwrap();
        assert_eq!(
            (c.track.as_str(), c.language.as_str(), c.kind),
            ("en-US", "en-US", CaptionKind::Manual)
        );
    }

    #[test]
    fn automatic_original_comes_next_and_drops_the_suffix() {
        let c = choose(&t(Some("tr"), &["en"], &["en", "tr", "tr-orig"]), &["en"]).unwrap();
        assert_eq!(
            (c.track.as_str(), c.language.as_str(), c.kind),
            ("tr-orig", "tr", CaptionKind::Automatic)
        );
        let c = choose(&t(Some("tr"), &[], &["en", "tr"]), &[]).unwrap();
        assert_eq!(c.track, "tr");
        // Unknown language: the single -orig track is the spoken one.
        let c = choose(&t(None, &[], &["de", "en-orig", "tr"]), &[]).unwrap();
        assert_eq!((c.track.as_str(), c.language.as_str()), ("en-orig", "en"));
    }

    #[test]
    fn translations_are_never_picked() {
        assert_eq!(choose(&t(None, &[], &["de", "en", "tr"]), &["tr"]), None);
        assert_eq!(choose(&t(Some("ja"), &[], &["en", "tr"]), &["tr"]), None);
        assert_eq!(choose(&t(None, &[], &[]), &["tr"]), None);
    }

    #[test]
    fn other_manual_tracks_follow_preference() {
        let c = choose(&t(Some("ja"), &["de", "en", "tr"], &[]), &["tr", "en"]).unwrap();
        assert_eq!(c.track, "tr");
        let c = choose(&t(None, &["de", "fr"], &[]), &["tr"]).unwrap();
        assert_eq!(c.track, "de");
    }

    #[test]
    fn timestamps() {
        assert_eq!(timestamp("00:01.500"), Some(1_500));
        assert_eq!(timestamp("01:02:03.004"), Some(3_723_004));
        assert_eq!(timestamp(" 00:00:10,250 "), Some(10_250));
        assert_eq!(timestamp("00:61.000"), None);
        assert_eq!(timestamp("00:01.5"), None);
        assert_eq!(timestamp("abc"), None);
    }

    #[test]
    fn plain_vtt_with_tags_entities_and_notes() {
        let vtt = "\u{feff}WEBVTT\r\nKind: captions\r\n\r\nNOTE a comment --> not a cue\r\n\r\n1\r\n00:00:01.000 --> 00:00:03.500 align:start\r\n<v Ali>Merhaba &amp; <b>hoş</b> geldiniz</v>\r\n\r\n00:00:04.000 --> 00:00:06.000\r\n<script>alert(1)</script> &lt;b&gt; ikinci\r\nsatır\r\n\r\n00:00:07.000 --> 00:00:07.000\r\nsıfır süre\r\n";
        let cues = parse_vtt(vtt.as_bytes()).unwrap();
        assert_eq!(
            cues,
            vec![
                Cue {
                    start_ms: 1_000,
                    end_ms: 3_500,
                    text: "Merhaba & hoş geldiniz".into()
                },
                Cue {
                    start_ms: 4_000,
                    end_ms: 6_000,
                    text: "alert(1) <b> ikinci satır".into()
                },
            ]
        );
    }

    #[test]
    fn rolling_automatic_captions_keep_each_line_once() {
        let vtt = "WEBVTT\nKind: captions\nLanguage: en\n\n\
00:00:00.160 --> 00:00:02.270 align:start position:0%\n \nhello<00:00:00.480><c> world</c>\n\n\
00:00:02.270 --> 00:00:02.280 align:start position:0%\nhello world\n \n\n\
00:00:02.280 --> 00:00:04.500 align:start position:0%\nhello world\nthis<00:00:02.600><c> is</c><00:00:02.900><c> fine</c>\n\n\
00:00:04.500 --> 00:00:04.510 align:start position:0%\nthis is fine\n \n\n\
00:00:04.510 --> 00:00:06.000 align:start position:0%\nthis is fine\nbye\n";
        let cues = parse_vtt(vtt.as_bytes()).unwrap();
        let texts: Vec<_> = cues
            .iter()
            .map(|c| (c.start_ms, c.end_ms, c.text.as_str()))
            .collect();
        assert_eq!(
            texts,
            vec![
                (160, 2_270, "hello world"),
                (2_280, 4_500, "this is fine"),
                (4_510, 6_000, "bye")
            ]
        );
    }

    #[test]
    fn not_vtt_and_too_large_are_refused() {
        assert_eq!(
            parse_vtt(b"1\n00:00:01,000 --> 00:00:02,000\nsrt\n"),
            Err(VttError::NotVtt)
        );
        let big = vec![b'a'; MAX_VTT_BYTES + 1];
        assert_eq!(parse_vtt(&big), Err(VttError::TooLarge));
    }

    #[test]
    fn cues_are_sorted_and_entities_stay_safe() {
        let vtt = "WEBVTT\n\n00:00:05.000 --> 00:00:06.000\nb &#x41;&#66; &bogus; &#0;\n\n00:00:01.000 --> 00:00:02.000\na &lrm;x\n";
        let cues = parse_vtt(vtt.as_bytes()).unwrap();
        assert_eq!(cues[0].text, "a x");
        assert_eq!(cues[1].text, "b AB &bogus; &#0;");
    }
}
