//! A fake OpenAI-compatible embedding API (`POST /v1/embeddings`) on
//! 127.0.0.1, in the test process. It stands for the user's Ollama.
//!
//! A vector is a bag of words: each word adds 1 at a position given by its
//! hash, and a few words of the same topic share a position, so tests can
//! check that "veritabanı yedekleme" finds a passage about "PostgreSQL
//! replication" without any shared word.
//!
//! [`FakeEmbed::set_mode`]: `ok` (default), `fail` (HTTP 500), `slow` (2 s
//! per request), `short` (one vector too few), `slow-after-1` (the first
//! request is quick, later ones slow), `dims-512` (vectors of another
//! length). Each request's input count is recorded.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const DIMS: usize = 1024;

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

fn slot(word: &str, dims: usize) -> usize {
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
    TOPICS.len() + (h as usize) % (dims - TOPICS.len() - 1)
}

pub fn vector(text: &str, dims: usize) -> Vec<f32> {
    let mut v = vec![0.0f32; dims];
    for w in text
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
    {
        v[slot(&w.to_lowercase(), dims)] += 1.0;
    }
    // Never a zero vector: a constant bias in the last place.
    v[dims - 1] += 0.01;
    v
}

#[derive(Default)]
struct Shared {
    mode: String,
    calls: Vec<usize>,
    models: Vec<String>,
}

#[derive(Clone)]
pub struct FakeEmbed {
    /// Base address, as a user would type it.
    pub url: String,
    shared: Arc<Mutex<Shared>>,
}

impl FakeEmbed {
    /// Starts the server; with `key`, requests without `Bearer <key>` get 401.
    pub fn start(key: Option<&str>) -> FakeEmbed {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}/v1", l.local_addr().unwrap().port());
        let shared = Arc::new(Mutex::new(Shared {
            mode: "ok".into(),
            ..Default::default()
        }));
        let s2 = shared.clone();
        let key = key.map(str::to_owned);
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                let (sh, k) = (s2.clone(), key.clone());
                std::thread::spawn(move || serve(s, k.as_deref(), &sh));
            }
        });
        FakeEmbed { url, shared }
    }

    pub fn set_mode(&self, m: &str) {
        self.shared.lock().unwrap().mode = m.into();
    }

    /// Input counts of the requests so far.
    pub fn calls(&self) -> Vec<usize> {
        self.shared.lock().unwrap().calls.clone()
    }

    /// Model names asked for.
    pub fn models(&self) -> Vec<String> {
        self.shared.lock().unwrap().models.clone()
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

fn answer(inputs: &[String], n: usize, dims: usize) -> String {
    let data: Vec<serde_json::Value> = inputs
        .iter()
        .take(n)
        .enumerate()
        .map(|(i, p)| {
            serde_json::json!({"object": "embedding", "index": i, "embedding": vector(p, dims)})
        })
        .collect();
    serde_json::json!({"object": "list", "data": data, "model": "fake"}).to_string()
}

fn serve(mut s: TcpStream, key: Option<&str>, sh: &Mutex<Shared>) {
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
    if (method, path) != ("POST", "/v1/embeddings") {
        return reply(&mut s, "404 Not Found", "{\"error\":\"not found\"}");
    }
    if let Some(k) = key {
        if auth != format!("Bearer {k}") {
            return reply(&mut s, "401 Unauthorized", "{\"error\":\"invalid key\"}");
        }
    }
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
    let inputs: Vec<String> = match &v["input"] {
        serde_json::Value::String(s) => vec![s.clone()],
        serde_json::Value::Array(a) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    };
    let (mode, before) = {
        let mut g = sh.lock().unwrap();
        let before = g.calls.len();
        g.calls.push(inputs.len());
        g.models.push(v["model"].as_str().unwrap_or("").to_owned());
        (g.mode.clone(), before)
    };
    match mode.as_str() {
        "fail" => return reply(&mut s, "500 Internal Server Error", "{\"error\":\"boom\"}"),
        "slow" => std::thread::sleep(Duration::from_secs(2)),
        "slow-after-1" if before >= 1 => std::thread::sleep(Duration::from_secs(2)),
        _ => {}
    }
    let n = if mode == "short" {
        inputs.len().saturating_sub(1)
    } else {
        inputs.len()
    };
    let dims = if mode == "dims-512" { 512 } else { DIMS };
    reply(&mut s, "200 OK", &answer(&inputs, n, dims));
}
