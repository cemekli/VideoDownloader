//! Test double for llama.cpp's `llama-embedding` (`vv-engine` tests only;
//! never shipped or run by the app). It reads the argv the engine builds
//! (`-m`, `-f`, `--embd-separator`, `--embd-output-format json`) and writes
//! the JSON the real one writes: one 1024-value vector per prompt.
//!
//! A vector is a bag of words: each word adds 1 at a position given by its
//! hash, and a few words of the same topic share a position, so tests can
//! check that "veritabanı yedekleme" finds a passage about "PostgreSQL
//! replikasyonu" without any shared word. Everything up to `Query:` (the
//! query instruction) is ignored.
//!
//! The scenario comes from a file `fake-embed-mode` next to the model:
//! `ok` (default), `fail`, `slow` (2 s per run), `short` (one vector too
//! few), `slow-after-1` (the first run is quick, later ones slow). Each run
//! appends its prompt count to `fake-embed-calls` there.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

const DIMS: usize = 1024;

/// Words of one topic, in Turkish and English.
const TOPICS: &[&[&str]] = &[
    &[
        "veritabanı",
        "database",
        "postgresql",
        "replikasyonu",
        "replication",
        "yedekleme",
        "backup",
        "pg_dump",
    ],
    &[
        "ekmek",
        "bread",
        "maya",
        "sourdough",
        "hamur",
        "dough",
        "bake",
    ],
    &[
        "koşu",
        "running",
        "maraton",
        "marathon",
        "antrenman",
        "training",
    ],
];

fn arg_after(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn fail(msg: &str, code: i32) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(code);
}

fn slot(word: &str) -> usize {
    for (i, t) in TOPICS.iter().enumerate() {
        if t.contains(&word) {
            return i;
        }
    }
    // FNV-1a, away from the topic slots.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in word.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    TOPICS.len() + (h as usize) % (DIMS - TOPICS.len())
}

fn vector(prompt: &str) -> Vec<f32> {
    let text = prompt.rsplit_once("Query:").map_or(prompt, |(_, q)| q);
    let mut v = vec![0.0f32; DIMS];
    for w in text
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
    {
        v[slot(&w.to_lowercase())] += 1.0;
    }
    // Never a zero vector: a constant bias in the last place.
    v[DIMS - 1] += 0.01;
    v
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        eprintln!("version: 1 (fake)");
        return;
    }
    let model = PathBuf::from(arg_after(&args, "-m").unwrap_or_else(|| fail("no -m", 1)));
    let file = arg_after(&args, "-f").unwrap_or_else(|| fail("no -f", 1));
    let sep = arg_after(&args, "--embd-separator").unwrap_or_else(|| "\n".into());
    if arg_after(&args, "--embd-output-format").as_deref() != Some("json") {
        fail("expected --embd-output-format json", 1);
    }
    if !model.is_file() {
        fail("failed to load model", 1);
    }
    let dir = model.parent().map(PathBuf::from).unwrap_or_default();
    let mode = std::fs::read_to_string(dir.join("fake-embed-mode"))
        .map(|s| s.trim().to_owned())
        .unwrap_or_else(|_| "ok".into());
    let calls = dir.join("fake-embed-calls");
    let before = std::fs::read_to_string(&calls)
        .map(|s| s.lines().count())
        .unwrap_or(0);
    let body = std::fs::read_to_string(&file).unwrap_or_else(|_| fail("cannot read prompts", 1));
    let prompts: Vec<&str> = body.split(sep.as_str()).collect();
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&calls)
    {
        let _ = writeln!(f, "{}", prompts.len());
    }
    match mode.as_str() {
        "fail" => fail("llama_model_load: error loading model", 1),
        "slow" => std::thread::sleep(Duration::from_secs(2)),
        "slow-after-1" if before >= 1 => std::thread::sleep(Duration::from_secs(2)),
        _ => {}
    }
    let n = if mode == "short" {
        prompts.len().saturating_sub(1)
    } else {
        prompts.len()
    };
    eprintln!("llama_model_loader: loaded meta data (fake)");
    let mut out = String::from("{\n  \"object\": \"list\",\n  \"data\": [\n");
    for (i, p) in prompts.iter().take(n).enumerate() {
        let v = vector(p);
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        let vals: Vec<String> = v.iter().map(|x| format!("{:.6}", x / norm)).collect();
        out.push_str(&format!(
            "    {{\n      \"object\": \"embedding\",\n      \"index\": {i},\n      \"embedding\": [{}]\n    }}{}\n",
            vals.join(","),
            if i + 1 < n { "," } else { "" }
        ));
    }
    out.push_str("  ]\n}\n");
    print!("{out}");
}
