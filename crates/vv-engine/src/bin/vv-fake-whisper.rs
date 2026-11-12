//! Test double for whisper.cpp's `whisper-cli` (`vv-engine` tests only;
//! never shipped or run by the app). It reads the argv the engine builds,
//! checks the WAV it is given and writes the same `-oj` JSON and progress
//! lines the real one writes (whisper.cpp v1.9.2 `examples/cli/cli.cpp`).
//!
//! The environment is cleared by `vv-process`, so the scenario comes from a
//! file `fake-whisper-mode` next to the model (`-m`): `ok` (default),
//! `silence`, `fail`, `garbage`, `slow`, `beyond`, `slow-after-1` (the
//! first run is quick, later ones slow), `fail-after-1`, `late-words` (each
//! segment's first word is 400 ms after whisper's segment start, by DTW).
//! Each run appends its WAV's length and `-l` to `fake-whisper-calls` there.
//!
//! With `-dtw` (which needs `-ojf` and `-nfa`, like the real one needs flash
//! attention off) each segment carries `tokens` with `t_dtw` in 10 ms
//! units, -1 except in `late-words`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn arg_after(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn fail(msg: &str, code: i32) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(code);
}

/// Length in ms of a 16 kHz mono s16 WAV; refuses anything else, like
/// whisper.cpp refuses other sample rates.
fn wav_ms(path: &Path) -> i64 {
    let b = std::fs::read(path).unwrap_or_else(|_| fail("failed to read audio", 2));
    if b.len() < 44 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        fail("not a WAV file", 2);
    }
    let mut i = 12;
    let mut fmt_ok = false;
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let len = u32::from_le_bytes(b[i + 4..i + 8].try_into().unwrap()) as usize;
        if id == b"fmt " {
            let ch = u16::from_le_bytes([b[i + 10], b[i + 11]]);
            let rate = u32::from_le_bytes(b[i + 12..i + 16].try_into().unwrap());
            let bits = u16::from_le_bytes([b[i + 22], b[i + 23]]);
            fmt_ok = ch == 1 && rate == 16_000 && bits == 16;
        }
        if id == b"data" {
            if !fmt_ok {
                fail("WAV must be 16 kHz mono 16-bit", 2);
            }
            let n = len.min(b.len() - i - 8);
            return n as i64 * 1000 / 32_000;
        }
        i += 8 + len + (len & 1);
    }
    fail("WAV has no data", 2)
}

fn seg(from: i64, to: i64, text: &str) -> String {
    seg_with(from, to, text, None)
}

/// A segment; with `words` (the first word's DTW time in ms, or -1) it
/// has `-ojf` tokens: `[_BEG_]`, then the words.
fn seg_with(from: i64, to: i64, text: &str, words: Option<i64>) -> String {
    let tokens = match words {
        None => String::new(),
        Some(first) => {
            let t = |ms: i64| if ms < 0 { -1 } else { ms / 10 };
            let mut v = vec![format!(
                "{{\"text\": \"[_BEG_]\", \"id\": 50364, \"p\": 0.9, \"t_dtw\": {}}}",
                t(from)
            )];
            for (i, w) in text.split_whitespace().enumerate() {
                let at = if first < 0 {
                    -1
                } else {
                    first + 100 * i as i64
                };
                v.push(format!(
                    "{{\"text\": \" {w}\", \"id\": 100, \"p\": 0.9, \"t_dtw\": {}}}",
                    t(at)
                ));
            }
            format!(",\n\t\t\t\"tokens\": [{}]", v.join(", "))
        }
    };
    format!(
        "\t\t{{\n\t\t\t\"timestamps\": {{}},\n\t\t\t\"offsets\": {{\"from\": {from}, \"to\": {to}}},\n\t\t\t\"text\": \"{text}\"{tokens}\n\t\t}}"
    )
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        println!("whisper.cpp version: 1.9.2");
        return;
    }
    let model = PathBuf::from(arg_after(&args, "-m").unwrap_or_else(|| fail("no model", 1)));
    if !model.is_file() {
        fail("failed to initialize whisper context", 3);
    }
    let mode = model
        .parent()
        .and_then(|d| std::fs::read_to_string(d.join("fake-whisper-mode")).ok())
        .unwrap_or_else(|| "ok".into());
    let mode = mode.trim();
    let wav = PathBuf::from(arg_after(&args, "-f").unwrap_or_else(|| fail("no input", 2)));
    let out = arg_after(&args, "-of").unwrap_or_else(|| fail("no -of", 1));
    let lang = arg_after(&args, "-l").unwrap_or_else(|| "en".into());
    let has = |f: &str| args.iter().any(|a| a == f);
    if !has("-oj") {
        fail("expected -oj", 1);
    }
    let dtw = arg_after(&args, "-dtw");
    if let Some(preset) = &dtw {
        if !["tiny", "base", "small", "medium"].contains(&preset.as_str()) {
            fail("unknown DTW preset", 1);
        }
        if !has("-ojf") || !has("-nfa") {
            fail("DTW needs -ojf and -nfa", 1);
        }
    }
    let ms = wav_ms(&wav);
    let calls_file = model.with_file_name("fake-whisper-calls");
    let earlier = std::fs::read_to_string(&calls_file)
        .map(|s| s.lines().count())
        .unwrap_or(0);
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&calls_file)
        .unwrap_or_else(|_| fail("cannot log", 1));
    let _ = writeln!(log, "{ms} {lang}");
    let mode = match mode {
        "slow-after-1" if earlier >= 1 => "slow",
        "fail-after-1" if earlier >= 1 => "fail",
        "slow-after-1" | "fail-after-1" => "ok",
        m => m,
    };
    if mode == "fail" {
        fail("failed to process audio", 7);
    }
    for p in (0..=100).step_by(25) {
        eprintln!("whisper_print_progress_callback: progress = {p:3}%");
        let _ = std::io::stderr().flush();
        if mode == "slow" {
            std::thread::sleep(Duration::from_secs(10));
        }
    }
    let detected = if lang == "auto" { "tr" } else { lang.as_str() };
    let segments = match mode {
        "silence" => vec![seg(0, ms, " [BLANK_AUDIO]")],
        // Ends 600 ms past the audio: inside the tolerance, cut to the end.
        "beyond" => vec![
            seg(0, ms / 2, " Merhaba d\u{fc}nya."),
            seg(ms / 2, ms + 600, " Son."),
        ],
        "late-words" if dtw.is_some() => vec![
            seg_with(0, ms / 2, " Merhaba d\u{fc}nya.", Some(400)),
            seg_with(ms / 2, ms, " Son.", Some(ms / 2 + 400)),
        ],
        _ if dtw.is_some() => vec![
            seg_with(
                0,
                ms / 2,
                " Merhaba d\u{fc}nya, \\\"bu\\\" bir deneme.",
                Some(-1),
            ),
            seg_with(ms / 2, ms / 2, " zero", Some(-1)),
            seg_with(ms / 2, ms, " \u{130}kinci sat\u{131}r.", Some(-1)),
        ],
        _ => vec![
            seg(0, ms / 2, " Merhaba d\u{fc}nya, \\\"bu\\\" bir deneme."),
            seg(ms / 2, ms / 2, " zero"),
            seg(ms / 2, ms, " \u{130}kinci sat\u{131}r."),
        ],
    };
    // Stdout gets the text like the real one; the engine must not keep it.
    println!("[00:00:00.000 --> 00:00:01.000]  Merhaba");
    let body = if mode == "garbage" {
        "{\"result\": ".to_owned()
    } else {
        format!(
            "{{\n\t\"systeminfo\": \"AVX = 1\",\n\t\"model\": {{\"type\": \"tiny\"}},\n\t\"params\": {{\"model\": \"m\", \"language\": \"{lang}\", \"translate\": false}},\n\t\"result\": {{\n\t\t\"language\": \"{detected}\"\n\t}},\n\t\"transcription\": [\n{}\n\t]\n}}\n",
            segments.join(",\n")
        )
    };
    std::fs::write(format!("{out}.json"), body)
        .unwrap_or_else(|_| fail("failed to open output", 1));
}
