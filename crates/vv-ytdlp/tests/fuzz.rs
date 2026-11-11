//! Fuzz tests for every parser here that reads yt-dlp's output (docs/testing.md
//! "Fuzz testleri"). Seeds are the captured runs in `tests/fixtures/ytdlp/`;
//! the cases add Turkish letters, emoji, cut lines and invalid UTF-8
//! (`vv-fuzz`). Each target must never panic and must keep its own rules.
//!
//! F6.2 bug (11 Nov 2026): a caption line with `&gt;&gt;` before Turkish
//! text panicked on a byte slice. Fixtures had `& son` only; these tests
//! would have found it (`decode_entities` case in `captions_vtt`).

use serde_json::Value;
use std::path::PathBuf;
use vv_fuzz::{compose, each, mutate_bytes, mutate_str, prefixes, run};
use vv_ytdlp::captions::{self, parse_vtt};
use vv_ytdlp::lines::LineSplitter;
use vv_ytdlp::outcome::{classify, Exit, RunObserver, Stream};
use vv_ytdlp::progress::{parse_line, ProgressEvent};
use vv_ytdlp::{metadata, redact::redact};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct Corpus {
    stdout_lines: Vec<String>,
    stderr_lines: Vec<String>,
    metadata: Vec<String>,
}

fn corpus() -> Corpus {
    let mut c = Corpus {
        stdout_lines: Vec::new(),
        stderr_lines: Vec::new(),
        metadata: Vec::new(),
    };
    let dir = root().join("tests/fixtures/ytdlp/runs");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    files.sort();
    for f in files {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        let out = v["stdout"].as_str().unwrap_or_default();
        if v["kind"] == "metadata" && !out.is_empty() {
            c.metadata.push(out.to_owned());
        }
        c.stdout_lines.extend(out.lines().map(str::to_owned));
        c.stderr_lines.extend(
            v["stderr"]
                .as_str()
                .unwrap_or_default()
                .lines()
                .map(str::to_owned),
        );
    }
    let errors: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("tests/fixtures/ytdlp/errors-source.json")).unwrap(),
    )
    .unwrap();
    for case in errors["cases"].as_array().unwrap() {
        c.stderr_lines.extend(
            case["stderr"]
                .as_str()
                .unwrap_or_default()
                .lines()
                .map(str::to_owned),
        );
    }
    c.stdout_lines.retain(|l| !l.is_empty());
    c.stderr_lines.retain(|l| !l.is_empty());
    assert!(c.stdout_lines.len() > 10 && c.stderr_lines.len() > 10 && !c.metadata.is_empty());
    c
}

const PROGRESS_PIECES: &[&str] = &[
    "vv-progress ",
    "vv-postprocess ",
    "{",
    "}",
    "\"status\":",
    "\"downloading\"",
    "\"finished\"",
    "\"downloaded_bytes\":",
    "\"total_bytes\":",
    "\"total_bytes_estimate\":",
    "\"fragment_index\":",
    "\"fragment_count\":",
    "\"eta\":",
    "\"speed\":",
    "\"postprocessor\":",
    "\"FixupM3u8\"",
    ",",
    "0",
    "1024",
    "-5",
    "NA",
    "NaN",
    "1e400",
];

fn check_progress(line: &str) {
    if let Ok(Some(ProgressEvent::Download(d))) = parse_line(line) {
        if let Some(p) = d.percentage {
            assert!(p.is_finite() && p >= 0.0, "{d:?}");
        }
        if let Some(s) = d.speed_bytes_per_second {
            assert!(s.is_finite(), "{d:?}");
        }
    }
}

#[test]
fn progress_lines() {
    let c = corpus();
    each(
        "progress/prefixes",
        c.stdout_lines.iter().take(6).flat_map(|l| prefixes(l)),
        |l| check_progress(l),
    );
    run(
        "progress",
        |r| {
            if r.chance(70) {
                let i = r.below(c.stdout_lines.len());
                mutate_str(r, &c.stdout_lines[i], PROGRESS_PIECES)
            } else {
                compose(r, PROGRESS_PIECES, 30)
            }
        },
        |l| check_progress(l),
    );
}

#[test]
fn error_lines_classify_and_redact() {
    let c = corpus();
    run(
        "outcome",
        |r| {
            let n = 1 + r.below(6);
            (0..n)
                .map(|_| {
                    let base = if r.chance(50) {
                        c.stderr_lines[r.below(c.stderr_lines.len())].clone()
                    } else {
                        c.stdout_lines[r.below(c.stdout_lines.len())].clone()
                    };
                    (
                        r.chance(70),
                        mutate_str(
                            r,
                            &base,
                            &["ERROR: ", "WARNING: ", "https://", "C:\\Users\\"],
                        ),
                    )
                })
                .collect::<Vec<_>>()
        },
        |lines| {
            let mut o = RunObserver::new();
            for (stderr, l) in lines {
                let _ = classify(l);
                let _ = o.observe(
                    if *stderr {
                        Stream::Stderr
                    } else {
                        Stream::Stdout
                    },
                    l,
                );
            }
            assert!(o.stderr_tail().count() <= vv_ytdlp::outcome::STDERR_TAIL_LINES);
            let _ = o.finish(Exit::Code(1), false);
        },
    );
}

#[test]
fn redaction_never_lets_a_url_secret_through() {
    // Whatever surrounds a URL, its query and user info are gone (AC-36).
    const PIECES: &[&str] = &[
        "ERROR: ",
        "[youtube] ",
        "x",
        " ",
        "(caused by ",
        "'",
        "\"",
        "<",
        "ş",
        "/",
    ];
    run(
        "redact",
        |r| {
            let before = compose(r, PIECES, 12);
            let after = compose(r, PIECES, 12);
            let scheme = *r.pick(&["https://", "HTTP://", "wss://"]);
            format!("{before} {scheme}u:VVCANARY@cdn.example.com/p/ş?sig=VVCANARY&t=1 {after}")
        },
        |line| {
            let out = redact(line);
            assert!(!out.contains("VVCANARY"), "{out}");
        },
    );
    run(
        "redact/any",
        |r| {
            String::from_utf8_lossy(&mutate_bytes(
                r,
                b"ERROR: https://a.example/x?y=1 C:\\Users\\name\\x",
                &[],
            ))
            .into_owned()
        },
        |line| {
            let _ = redact(line);
        },
    );
}

#[test]
fn metadata_json() {
    let c = corpus();
    const PIECES: &[&str] = &[
        "\"title\":",
        "\"upload_date\":",
        "\"20261109\"",
        "\"2026ğ110\"",
        "\"subtitles\":",
        "\"automatic_captions\":",
        "{\"tr\":[{\"ext\":\"vtt\"}]}",
        "\"duration\":",
        "NaN",
        "Infinity",
        "-Infinity",
        "NA",
        "null",
        "[",
        "]",
        "{",
        "}",
        ",",
    ];
    run(
        "metadata",
        |r| {
            let base = c.metadata[r.below(c.metadata.len())].clone();
            if r.chance(50) {
                mutate_str(r, &base, PIECES).into_bytes()
            } else {
                mutate_bytes(r, base.as_bytes(), PIECES)
            }
        },
        |bytes| {
            if let Ok(metadata::Extracted::Video(v)) = metadata::parse(bytes) {
                assert!(v.formats.len() <= metadata::MAX_FORMATS);
                if let Some(d) = v.duration_seconds {
                    assert!(d.is_finite(), "{d}");
                }
                assert!(v.captions.manual.len() <= captions::MAX_TRACKS);
                let _ = captions::choose(&v.captions, &["tr", "en"]);
            }
        },
    );
}

#[test]
fn caption_tracks_and_choice() {
    const LANGS: &[&str] = &[
        "tr",
        "en",
        "tr-orig",
        "en-orig",
        "en-US",
        "de",
        "live_chat",
        "ş",
        "",
        "-orig",
        "tr-TR-orig",
        "zh-Hans",
        "😀",
    ];
    run(
        "caption_tracks",
        |r| {
            let mut o = serde_json::Map::new();
            let map = |r: &mut vv_fuzz::Rng| {
                let mut m = serde_json::Map::new();
                for _ in 0..r.below(8) {
                    let lang = if r.chance(70) {
                        (*r.pick(LANGS)).to_owned()
                    } else {
                        compose(r, LANGS, 3)
                    };
                    m.insert(lang, serde_json::json!([{"ext": *r.pick(&["vtt", "json3", "srv1"]), "url": "https://x.example/c"}]));
                }
                Value::Object(m)
            };
            o.insert("subtitles".into(), map(r));
            o.insert("automatic_captions".into(), map(r));
            if r.chance(70) {
                o.insert(
                    "language".into(),
                    Value::String((*r.pick(LANGS)).to_owned()),
                );
            }
            o
        },
        |o| {
            let t = captions::tracks(o);
            for tr in t.manual.iter().chain(&t.automatic) {
                assert!(captions::valid_track(tr), "{tr}");
            }
            if let Some(ch) = captions::choose(&t, &["tr", "en"]) {
                assert!(t.manual.contains(&ch.track) || t.automatic.contains(&ch.track));
            }
        },
    );
}

const VTT_PIECES: &[&str] = &[
    "WEBVTT\n",
    "\n",
    "\n\n",
    "00:00:01.000 --> 00:00:02.500",
    "00:01.000 --> 00:00.500",
    " align:start position:0%",
    "01:02:03.004",
    " --> ",
    "&gt;&gt; ",
    "&amp;",
    "&nbsp;",
    "&lrm;",
    "&#x15f;",
    "<c>",
    "</c>",
    "<00:00:01.200>",
    "<v Ayşe>",
    "<i>",
    "</i>",
    "NOTE ",
    "STYLE\n::cue { color: lime }",
    "Kind: captions\nLanguage: tr\n",
    "Merhaba",
    "çığlık",
];

fn check_vtt(bytes: &[u8]) {
    let Ok(cues) = parse_vtt(bytes) else { return };
    assert!(cues.len() <= captions::MAX_CUES);
    for w in cues.windows(2) {
        assert!(w[0].start_ms <= w[1].start_ms);
    }
    for c in &cues {
        assert!(c.start_ms < c.end_ms, "{c:?}");
        assert!(!c.text.is_empty() && c.text.chars().count() <= captions::MAX_CUE_CHARS);
        assert!(!c.text.chars().any(char::is_control), "{c:?}");
    }
}

#[test]
fn captions_vtt() {
    let seeds = [
        "WEBVTT\n\n00:00:00.000 --> 00:00:01.200\nMerhaba <b>dünya</b>\n\n00:00:01.200 --> 00:00:02.900\nİkinci satır &amp; son\n",
        "WEBVTT\nKind: captions\nLanguage: tr\n\n00:00:00.100 --> 00:00:01.000 align:start position:0%\n&gt;&gt; Ayşe: çok güzel<00:00:00.400><c> değil</c> mi?\n\n00:00:01.000 --> 00:00:02.000\n&gt;&gt; Şükrü: ığdır'a gidiyoruz &#x15f;&#287;\n",
    ];
    each(
        "captions/prefixes",
        seeds.iter().flat_map(|s| prefixes(s)),
        |s| check_vtt(s.as_bytes()),
    );
    run(
        "captions",
        |r| {
            let base = *r.pick(&seeds);
            match r.below(3) {
                0 => mutate_str(r, base, VTT_PIECES).into_bytes(),
                1 => mutate_bytes(r, base.as_bytes(), VTT_PIECES),
                _ => format!("WEBVTT\n\n{}", compose(r, VTT_PIECES, 60)).into_bytes(),
            }
        },
        |b| check_vtt(b),
    );
}

#[test]
fn line_splitter_keeps_its_bound() {
    run(
        "lines",
        |r| {
            let text = mutate_bytes(
                r,
                "vv-progress {\"a\":1}\nERROR: çok uzun satır 😀\r\nson".as_bytes(),
                &["\n", "\r", "ş"],
            );
            let max = 1 + r.below(40);
            let mut chunks = Vec::new();
            let mut rest = text.as_slice();
            while !rest.is_empty() {
                let n = 1 + r.below(rest.len());
                chunks.push(rest[..n].to_vec());
                rest = &rest[n..];
            }
            (max, chunks)
        },
        |(max, chunks)| {
            let mut s = LineSplitter::new(*max);
            let mut lines = Vec::new();
            for ch in chunks {
                lines.extend(s.push(ch));
                assert!(s.buffered() <= *max);
            }
            lines.extend(s.finish());
            for l in &lines {
                assert!(!l.text.contains(['\n', '\r']), "{l:?}");
            }
        },
    );
}
