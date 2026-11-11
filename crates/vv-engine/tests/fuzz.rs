//! Fuzz tests for the engine's own parsers of files the user gives us
//! (docs/testing.md "Fuzz testleri"): Netscape cookie files (F5.3) and
//! Firefox's `profiles.ini` (F5.2), and what whisper-cli writes (F6.2b:
//! progress lines, the `-oj` JSON, the WAV FFmpeg made). Never a panic;
//! only the platform's cookies are kept; a relative profile never leaves
//! Firefox's folder; transcript segments keep their rules.

use std::path::Component;
use vv_domain::normalize::Platform;
use vv_engine::browsers::firefox_profiles;
use vv_engine::cookiefile::{domains, filter};
use vv_engine::transcribe::{parse_output, parse_progress, wav_duration_ms};
use vv_fuzz::{each, mutate_bytes, mutate_str, prefixes, run};

const COOKIES: &str = "# Netscape HTTP Cookie File\n\
# kullanıcı: Ayşe Çağlar 😀\n\
.reddit.com\tTRUE\t/\tTRUE\t0\treddit_session\tVVCANARY-ş\n\
#HttpOnly_.www.reddit.com\tTRUE\t/\tTRUE\t4102444800\tloid\tabc\n\
.reddit.com.evil.example\tTRUE\t/\tTRUE\t0\tx\ty\n\
evilreddit.com\tTRUE\t/\tTRUE\t0\tx\ty\n\
.youtube.com\tTRUE\t/\tTRUE\t0\tSID\tz\r\n\
.vk.com\tTRUE\t/\tFALSE\t1\told\texpired\n";

const COOKIE_PIECES: &[&str] = &[
    "\t",
    "\n",
    "\r\n",
    "TRUE",
    "FALSE",
    "/",
    ".",
    ".reddit.com",
    "reddit.com",
    ".com",
    "#HttpOnly_",
    "#",
    "0",
    "-1",
    "99999999999999999999",
    "ŞİRKET.reddit.com",
    ".REDDİT.com",
    "xn--",
];

fn platform(i: usize) -> Platform {
    [
        Platform::Reddit,
        Platform::Youtube,
        Platform::Vk,
        Platform::Instagram,
        Platform::Generic,
    ][i % 5]
}

fn check_cookies((text, p): &(String, Platform)) {
    let Ok(f) = filter(text, *p, 1_800_000_000) else {
        return;
    };
    assert!(f.count > 0);
    let bases = domains(*p);
    for line in f.text.lines().skip(1) {
        let body = line.strip_prefix("#HttpOnly_").unwrap_or(line);
        let domain = body
            .split('\t')
            .next()
            .unwrap()
            .trim_start_matches('.')
            .to_ascii_lowercase();
        assert!(
            bases
                .iter()
                .any(|b| domain == *b || domain.ends_with(&format!(".{b}"))),
            "{domain} kept for {p:?}"
        );
    }
}

#[test]
fn cookie_files() {
    each(
        "cookies/prefixes",
        prefixes(COOKIES)
            .into_iter()
            .map(|s| (s.to_owned(), Platform::Reddit)),
        check_cookies,
    );
    run(
        "cookies",
        |r| {
            let text = if r.chance(50) {
                mutate_str(r, COOKIES, COOKIE_PIECES)
            } else {
                String::from_utf8_lossy(&mutate_bytes(r, COOKIES.as_bytes(), COOKIE_PIECES))
                    .into_owned()
            };
            (text, platform(r.below(5)))
        },
        check_cookies,
    );
}

const PROFILES: &str =
    "[Install308046B0AF4A39CB]\nDefault=Profiles/abc.default-release\nLocked=1\n\n\
[Profile0]\nName=Ayşe'nin profili 😀\nIsRelative=1\nPath=Profiles/abc.default-release\n\n\
[Profile1]\nName=eski\nIsRelative=1\nPath=Profiles/x.default\nDefault=1\n\n\
[General]\nStartWithLastProfile=1\nVersion=2\n";

#[test]
fn firefox_profiles_ini() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let ini = root.join("profiles.ini");
    const PIECES: &[&str] = &[
        "[",
        "]",
        "=",
        "\n",
        "[Profile9]\n",
        "Path=",
        "IsRelative=0\n",
        "IsRelative=1\n",
        "Default=1\n",
        "..",
        "../",
        "..\\",
        "C:\\",
        "C:",
        "/etc",
        "\\\\server\\share",
        "Profiles/",
        "Name=",
    ];
    let check = |bytes: &Vec<u8>| {
        std::fs::write(&ini, bytes).unwrap();
        if String::from_utf8_lossy(bytes).contains("IsRelative=0") {
            let _ = firefox_profiles(root);
            return;
        }
        // Every profile came from a relative path: it stays under the root.
        for p in firefox_profiles(root) {
            let rest = p
                .path
                .strip_prefix(root)
                .unwrap_or_else(|_| panic!("{:?} is outside {root:?}", p.path));
            assert!(
                rest.components().all(|c| matches!(c, Component::Normal(_))),
                "{:?} leaves the folder",
                p.path
            );
        }
    };
    for s in prefixes(PROFILES) {
        check(&s.as_bytes().to_vec());
    }
    run(
        "profiles.ini",
        |r| {
            if r.chance(50) {
                mutate_str(r, PROFILES, PIECES).into_bytes()
            } else {
                mutate_bytes(r, PROFILES.as_bytes(), PIECES)
            }
        },
        |b| check(b),
    );
}

/// whisper-cli `-oj` output as v1.9.2 writes it (escapes only `"` and `\\`),
/// with Turkish text, an emoji, a raw tab inside a string and markers.
const WHISPER_JSON: &str = "{\n\t\"systeminfo\": \"AVX = 1\",\n\t\"result\": {\n\t\t\"language\": \"tr\"\n\t},\n\t\"transcription\": [\n\
\t\t{\"timestamps\": {}, \"offsets\": {\"from\": 0, \"to\": 2400}, \"text\": \" Merhaba arkadaşlar, \\\"bugün\\\" İstanbul'dayız.\"},\n\
\t\t{\"timestamps\": {}, \"offsets\": {\"from\": 2400, \"to\": 4000}, \"text\": \" [Müzik]\"},\n\
\t\t{\"timestamps\": {}, \"offsets\": {\"from\": 4000, \"to\": 4000}, \"text\": \" sıfır\"},\n\
\t\t{\"timestamps\": {}, \"offsets\": {\"from\": 4000, \"to\": 6100}, \"text\": \" Çığlık\tatmayın 😀 (gülüşmeler) ığdır\"}\n\
\t]\n}\n";

fn check_whisper(raw: &[u8]) {
    for asked in ["auto", "tr", "en"] {
        let Ok(o) = parse_output(raw, asked) else {
            continue;
        };
        assert!(
            o.language == "und" || (2..=8).contains(&o.language.len()),
            "{}",
            o.language
        );
        for s in &o.segments {
            assert!(s.start_ms < s.end_ms, "{s:?}");
            assert!(!s.text.is_empty() && s.text.trim() == s.text, "{s:?}");
            assert!(!s.text.chars().any(char::is_control), "{s:?}");
        }
    }
}

#[test]
fn whisper_output() {
    let base = parse_output(WHISPER_JSON.as_bytes(), "auto").expect("seed parses");
    assert_eq!(base.language, "tr");
    assert_eq!(base.segments.len(), 2, "{:?}", base.segments);
    each("whisper/prefixes", prefixes(WHISPER_JSON), |s| {
        check_whisper(s.as_bytes())
    });
    const PIECES: &[&str] = &[
        "\"",
        "\\",
        "\\u0000",
        "\\u0007",
        "\\ud83d",
        "{",
        "}",
        "[",
        "]",
        ",",
        ":",
        "\"offsets\": {\"from\": 9, \"to\": 1}",
        "\"text\": ",
        "\"language\": \"İT\"",
        "\"language\": \"auto\"",
        "-1",
        "9223372036854775807",
        "1e3",
        "[BLANK_AUDIO]",
        "(",
        ")",
    ];
    run(
        "whisper_json",
        |r| {
            if r.chance(50) {
                mutate_str(r, WHISPER_JSON, PIECES).into_bytes()
            } else {
                mutate_bytes(r, WHISPER_JSON.as_bytes(), PIECES)
            }
        },
        |b| check_whisper(b),
    );
}

#[test]
fn whisper_progress_lines() {
    let line = "whisper_print_progress_callback: progress =  45%";
    assert_eq!(parse_progress(line), Some(45));
    run(
        "whisper_progress",
        |r| {
            mutate_str(
                r,
                line,
                &["%", "100", "-1", "progress =", "999999999999999999999", " "],
            )
        },
        |l| {
            if let Some(n) = parse_progress(l) {
                assert!((0..=100).contains(&n), "{n}");
            }
        },
    );
}

#[test]
fn wav_headers() {
    // 44-byte header + 32 000 bytes = 1 s of 16 kHz mono 16-bit PCM.
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36u32 + 32_000).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&[1, 0, 1, 0]);
    wav.extend_from_slice(&16_000u32.to_le_bytes());
    wav.extend_from_slice(&32_000u32.to_le_bytes());
    wav.extend_from_slice(&[2, 0, 16, 0]);
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&32_000u32.to_le_bytes());
    wav.resize(44 + 32_000, 0);
    assert_eq!(wav_duration_ms(&wav), Some(1_000));
    let header = wav[..44].to_vec();
    run(
        "wav",
        |r| {
            let mut v = mutate_bytes(r, &header, &["data", "LIST", "RIFF"]);
            if r.chance(30) {
                // A chunk size near u32::MAX.
                let at = r.below(v.len().saturating_sub(4) + 1);
                if let Some(w) = v.get_mut(at..at + 4) {
                    w.copy_from_slice(&u32::MAX.to_le_bytes());
                }
            }
            v.resize(v.len() + r.below(64), 0);
            v
        },
        |b| {
            if let Some(ms) = wav_duration_ms(b) {
                assert!(ms >= 0 && ms <= b.len() as i64 * 1000 / 32_000, "{ms}");
            }
        },
    );
}
