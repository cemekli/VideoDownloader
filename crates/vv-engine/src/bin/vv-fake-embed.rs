//! Test double for llama.cpp's `llama-server --embedding` (`vv-engine`
//! tests only; never shipped or run by the app). It reads the argv the
//! engine builds, listens on `--host`/`--port`, answers `GET /health` and
//! `POST /v1/embeddings` (only with `Authorization: Bearer <--api-key>`)
//! with the JSON the real one sends: one 1024-value vector per input.
//!
//! A vector is a bag of words: each word adds 1 at a position given by its
//! hash, and a few words of the same topic share a position, so tests can
//! check that "veritabanı yedekleme" finds a passage about "PostgreSQL
//! replikasyonu" without any shared word. Everything up to `Query:` (the
//! query instruction) is ignored.
//!
//! The scenario comes from a file `fake-embed-mode` next to the model:
//! `ok` (default), `fail` (exits at start), `slow` (2 s per request),
//! `short` (one vector too few), `slow-after-1` (the first request is quick,
//! later ones slow). Each request appends its input count to
//! `fake-embed-calls` there; each start appends a line to `fake-embed-starts`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
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

fn append(path: &Path, line: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{line}");
    }
}

fn reply(s: &mut TcpStream, status: &str, body: &str) {
    let _ = write!(
        s,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = s.flush();
}

fn answer(inputs: &[String], n: usize) -> String {
    let mut out = String::from("{\"object\":\"list\",\"data\":[");
    for (i, p) in inputs.iter().take(n).enumerate() {
        let v = vector(p);
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        let vals: Vec<String> = v.iter().map(|x| format!("{:.6}", x / norm)).collect();
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"object\":\"embedding\",\"index\":{i},\"embedding\":[{}]}}",
            vals.join(",")
        ));
    }
    out.push_str("],\"model\":\"fake\"}");
    out
}

fn serve(mut s: TcpStream, key: &str, dir: &Path, mode: &str) {
    let mut r = BufReader::new(s.try_clone().expect("clone"));
    let mut line = String::new();
    if r.read_line(&mut line).is_err() {
        return;
    }
    let mut len = 0usize;
    let mut auth = String::new();
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).is_err() || h.trim().is_empty() {
            break;
        }
        let (k, v) = h.split_once(':').unwrap_or((&h, ""));
        match k.trim().to_ascii_lowercase().as_str() {
            "content-length" => len = v.trim().parse().unwrap_or(0),
            "authorization" => auth = v.trim().to_owned(),
            _ => {}
        }
    }
    let mut body = vec![0u8; len];
    if r.read_exact(&mut body).is_err() {
        return;
    }
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    match (method, path) {
        ("GET", "/health") => reply(&mut s, "200 OK", "{\"status\":\"ok\"}"),
        ("POST", "/v1/embeddings") => {
            if auth != format!("Bearer {key}") {
                return reply(
                    &mut s,
                    "401 Unauthorized",
                    "{\"error\":{\"message\":\"Invalid API Key\"}}",
                );
            }
            let v: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
            let inputs: Vec<String> = v["input"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            let calls = dir.join("fake-embed-calls");
            let before = std::fs::read_to_string(&calls)
                .map(|s| s.lines().count())
                .unwrap_or(0);
            append(&calls, &inputs.len().to_string());
            match mode {
                "slow" => std::thread::sleep(Duration::from_secs(2)),
                "slow-after-1" if before >= 1 => std::thread::sleep(Duration::from_secs(2)),
                _ => {}
            }
            let n = if mode == "short" {
                inputs.len().saturating_sub(1)
            } else {
                inputs.len()
            };
            reply(&mut s, "200 OK", &answer(&inputs, n));
        }
        _ => reply(
            &mut s,
            "404 Not Found",
            "{\"error\":{\"message\":\"File Not Found\"}}",
        ),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        eprintln!("version: 1 (fake)");
        return;
    }
    let model = PathBuf::from(arg_after(&args, "-m").unwrap_or_else(|| fail("no -m", 1)));
    let host = arg_after(&args, "--host").unwrap_or_default();
    let port = arg_after(&args, "--port").unwrap_or_else(|| fail("no --port", 1));
    let key = arg_after(&args, "--api-key").unwrap_or_else(|| fail("no --api-key", 1));
    if host != "127.0.0.1" {
        fail("the fake only listens on 127.0.0.1", 2);
    }
    if !args.iter().any(|a| a == "--embedding") {
        fail("expected --embedding", 1);
    }
    if !model.is_file() {
        fail("failed to load model", 1);
    }
    let dir = model.parent().map(PathBuf::from).unwrap_or_default();
    let mode = std::fs::read_to_string(dir.join("fake-embed-mode"))
        .map(|s| s.trim().to_owned())
        .unwrap_or_else(|_| "ok".into());
    append(
        &dir.join("fake-embed-starts"),
        &std::process::id().to_string(),
    );
    if mode == "fail" {
        fail("llama_model_load: error loading model", 1);
    }
    let l = TcpListener::bind(format!("{host}:{port}"))
        .unwrap_or_else(|e| fail(&format!("couldn't bind HTTP server socket: {e}"), 1));
    eprintln!("main: server is listening on http://{host}:{port} (fake)");
    for s in l.incoming().flatten() {
        serve(s, &key, &dir, &mode);
    }
}
