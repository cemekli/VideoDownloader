//! STATE.md F6.1: model catalog, consent and the `model_download` job
//! against a loopback server (SPEC §17.1, AC-20; ADR-0026).

mod common;

use common::*;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use vv_engine::models::{self, Catalog, Fetch, RequestError};
use vv_netguard::Policy;
use vv_persistence::jobs::{self, JobKind, JobStatus};
use vv_persistence::models::{self as rows, ModelStatus};

/// (path, Range header) of each request the test server saw.
type Seen = Arc<Mutex<Vec<(String, Option<String>)>>>;

const REV: &str = "0123456789abcdef0123456789abcdef01234567";

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn body() -> Vec<u8> {
    (0..300_000u32).map(|i| (i % 251) as u8).collect()
}

fn catalog_for(bytes: &[u8], sha1_of: &[u8]) -> Catalog {
    let json = serde_json::json!({
        "schemaVersion": 1,
        "provider": "whisper_cpp",
        "repository": models::REPOSITORY,
        "revision": REV,
        "license": "MIT",
        "models": [{
            "id": "whisper-tiny", "label": "Tiny", "file": "ggml-tiny.bin",
            "languages": "multilingual",
            "sha1": hex(&Sha1::digest(sha1_of)),
            "sha256": hex(&Sha256::digest(bytes)),
            "size": bytes.len(),
            "upstreamMemoryMb": 273
        }]
    });
    Catalog::parse(&json.to_string()).unwrap()
}

/// A tiny HTTP/1.1 server: `/file` serves `body` (with Range support),
/// `/redirect` sends to `/file`, `/away` sends to a host outside the
/// allow-list. Records each request's path and Range header.
struct Server {
    addr: SocketAddr,
    seen: Seen,
}

fn serve(body: Vec<u8>) -> Server {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            handle(s, &body, &seen2);
        }
    });
    Server { addr, seen }
}

fn handle(mut s: TcpStream, body: &[u8], seen: &Mutex<Vec<(String, Option<String>)>>) {
    let mut r = BufReader::new(s.try_clone().unwrap());
    let mut line = String::new();
    r.read_line(&mut line).unwrap();
    let path = line.split_whitespace().nth(1).unwrap_or("").to_owned();
    let mut range = None;
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).unwrap() == 0 || h == "\r\n" {
            break;
        }
        if let Some(v) = h
            .strip_prefix("Range: ")
            .or_else(|| h.strip_prefix("range: "))
        {
            range = Some(v.trim().to_owned());
        }
    }
    seen.lock().unwrap().push((path.clone(), range.clone()));
    let send = |s: &mut TcpStream, head: String, data: &[u8]| {
        let _ = s.write_all(head.as_bytes());
        let _ = s.write_all(data);
    };
    match path.as_str() {
        "/redirect" => send(
            &mut s,
            "HTTP/1.1 302 Found\r\nLocation: /file\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            &[],
        ),
        "/away" => send(
            &mut s,
            "HTTP/1.1 302 Found\r\nLocation: https://evil.example/ggml-tiny.bin\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            &[],
        ),
        "/file" => {
            let from = range
                .as_deref()
                .and_then(|r| r.strip_prefix("bytes="))
                .and_then(|r| r.trim_end_matches('-').parse::<usize>().ok());
            match from {
                Some(n) => send(
                    &mut s,
                    format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {n}-{}/{}\r\nConnection: close\r\n\r\n",
                        body.len() - n,
                        body.len() - 1,
                        body.len()
                    ),
                    &body[n..],
                ),
                None => send(
                    &mut s,
                    format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()),
                    body,
                ),
            }
        }
        _ => send(
            &mut s,
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            &[],
        ),
    }
}

fn fetch_for(srv: &Server) -> Fetch {
    Fetch::new(Policy {
        test_targets: vec![srv.addr],
        ..Policy::default()
    })
}

/// Claims the queued model download and runs it against `path` on `srv`.
fn run_download(lib: &Lib, srv: &Server, cat: &Catalog, path: &str) -> jobs::Job {
    let lease = lib
        .write(|c| jobs::claim(c, "test-owner", &[JobKind::ModelDownload], 1, false, 1))
        .unwrap()
        .expect("a queued model download");
    let id = lease.job.id.clone();
    let url = format!("http://{}{path}", srv.addr);
    models::run_with(lib.cx(), lease, &fetch_for(srv), cat, &|_| url.clone());
    lib.job(&id)
}

#[test]
fn shipped_catalog_parses_and_is_anchored() {
    let c = models::catalog();
    assert!(!c.models.is_empty(), "tools/models.json did not parse");
    assert_eq!(c.repository, models::REPOSITORY);
    // Turkish needs multilingual models (DEC-11).
    assert!(c.models.iter().all(|m| m.languages == "multilingual"));
    let other = include_str!("../../../tools/models.json")
        .replace(models::REPOSITORY, "https://huggingface.co/someone/else");
    assert!(Catalog::parse(&other).is_err());
    let traversal =
        include_str!("../../../tools/models.json").replace("ggml-tiny.bin", "../ggml-tiny.bin");
    assert!(Catalog::parse(&traversal).is_err());
}

#[test]
fn nothing_is_queued_or_downloaded_without_consent() {
    // AC-20: the library opens and works with no model and no consent.
    let lib = library(Setup::fake());
    assert!(lib.write(|c| rows::list(c).unwrap()).is_empty());
    assert!(lib
        .write(|c| jobs::list(c, JobStatus::ALL, 100).unwrap())
        .is_empty());
    lib.run();
    assert!(!lib.cx().dirs.models.exists());
    // An entry without pinned hashes cannot be agreed to.
    let json = include_str!("../../../tools/models.json");
    let mut cat = Catalog::parse(json).unwrap();
    cat.revision = None;
    assert_eq!(
        lib.write(|c| models::agree_and_queue(c, &cat, "whisper-tiny", 1)),
        Err(RequestError::NotPinned)
    );
    assert_eq!(
        lib.write(|c| models::agree_and_queue(c, &cat, "whisper-nope", 1)),
        Err(RequestError::UnknownModel)
    );
    assert!(lib.write(|c| rows::list(c).unwrap()).is_empty());
}

#[test]
fn agreed_download_follows_the_redirect_verifies_and_installs() {
    let lib = library(Setup::fake());
    let bytes = body();
    let cat = catalog_for(&bytes, &bytes);
    let srv = serve(bytes.clone());
    let q = lib
        .write(|c| models::agree_and_queue(c, &cat, "whisper-tiny", 10))
        .unwrap();
    // Asking twice returns the open job.
    let again = lib
        .write(|c| models::agree_and_queue(c, &cat, "whisper-tiny", 11))
        .unwrap();
    assert!(again.existing && again.id == q.id);
    let inst = lib.write(|c| rows::get(c, "whisper-tiny").unwrap().unwrap());
    assert_eq!((inst.status, inst.consent_at), (ModelStatus::Pending, 10));
    assert_eq!(
        inst.spec.source_url,
        format!("{}/resolve/{REV}/ggml-tiny.bin", models::REPOSITORY)
    );

    let job = run_download(&lib, &srv, &cat, "/redirect");
    assert_eq!(job.status, JobStatus::Succeeded, "{job:?}");
    let dir = &lib.cx().dirs.models;
    assert_eq!(std::fs::read(dir.join("ggml-tiny.bin")).unwrap(), bytes);
    assert!(!dir.join("ggml-tiny.bin.part").exists());
    let inst = lib.write(|c| rows::get(c, "whisper-tiny").unwrap().unwrap());
    assert_eq!(inst.status, ModelStatus::Ready);
    assert!(inst.verified_at.is_some());
    let paths: Vec<String> = srv
        .seen
        .lock()
        .unwrap()
        .iter()
        .map(|s| s.0.clone())
        .collect();
    assert_eq!(paths, vec!["/redirect", "/file"]);
}

#[test]
fn a_part_file_is_resumed_with_a_range_request() {
    let lib = library(Setup::fake());
    let bytes = body();
    let cat = catalog_for(&bytes, &bytes);
    let srv = serve(bytes.clone());
    lib.write(|c| models::agree_and_queue(c, &cat, "whisper-tiny", 1))
        .unwrap();
    let dir = lib.cx().dirs.models.clone();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("ggml-tiny.bin.part"), &bytes[..120_000]).unwrap();
    let job = run_download(&lib, &srv, &cat, "/file");
    assert_eq!(job.status, JobStatus::Succeeded, "{job:?}");
    assert_eq!(
        srv.seen.lock().unwrap()[0].1.as_deref(),
        Some("bytes=120000-")
    );
    assert_eq!(std::fs::read(dir.join("ggml-tiny.bin")).unwrap(), bytes);
}

#[test]
fn a_hash_mismatch_fails_and_leaves_no_file() {
    let lib = library(Setup::fake());
    let bytes = body();
    let mut other = bytes.clone();
    other[1000] ^= 0xff;
    // Pinned for `bytes`, the server sends `other` (same length).
    let cat = catalog_for(&bytes, &bytes);
    let srv = serve(other);
    lib.write(|c| models::agree_and_queue(c, &cat, "whisper-tiny", 1))
        .unwrap();
    let job = run_download(&lib, &srv, &cat, "/file");
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error_code.as_deref(), Some("CHECKSUM_MISMATCH"));
    let dir = &lib.cx().dirs.models;
    assert!(!dir.join("ggml-tiny.bin").exists());
    assert!(!dir.join("ggml-tiny.bin.part").exists());
    let inst = lib.write(|c| rows::get(c, "whisper-tiny").unwrap().unwrap());
    assert_eq!(inst.status, ModelStatus::Pending);
}

#[test]
fn the_published_sha1_is_checked_too() {
    let lib = library(Setup::fake());
    let bytes = body();
    // SHA-256 pin matches the bytes, the published SHA-1 does not.
    let cat = catalog_for(&bytes, b"something else");
    let srv = serve(bytes);
    lib.write(|c| models::agree_and_queue(c, &cat, "whisper-tiny", 1))
        .unwrap();
    let job = run_download(&lib, &srv, &cat, "/file");
    assert_eq!(job.error_code.as_deref(), Some("CHECKSUM_MISMATCH"));
}

#[test]
fn redirects_outside_hugging_face_are_refused() {
    let lib = library(Setup::fake());
    let bytes = body();
    let cat = catalog_for(&bytes, &bytes);
    let srv = serve(bytes);
    lib.write(|c| models::agree_and_queue(c, &cat, "whisper-tiny", 1))
        .unwrap();
    let job = run_download(&lib, &srv, &cat, "/away");
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error_code.as_deref(), Some("URL_BLOCKED"));
    let f = fetch_for(&srv);
    assert!(f.allowed("https://huggingface.co/ggerganov/whisper.cpp/resolve/x/ggml-tiny.bin"));
    assert!(f.allowed("https://cas-bridge.xethub.hf.co/xet-bridge-us/abc"));
    assert!(!f.allowed("http://huggingface.co/x"));
    assert!(!f.allowed("https://huggingface.co.evil.example/x"));
    assert!(!f.allowed("https://user:pw@huggingface.co/x"));
    assert!(!f.allowed("http://127.0.0.1:1/x"));
}

#[test]
fn a_consent_for_another_file_is_not_used() {
    let lib = library(Setup::fake());
    let bytes = body();
    let cat = catalog_for(&bytes, &bytes);
    let srv = serve(bytes.clone());
    lib.write(|c| models::agree_and_queue(c, &cat, "whisper-tiny", 1))
        .unwrap();
    // The catalog moved on (new file) after the user agreed.
    let mut newer = bytes.clone();
    newer.push(0);
    let cat2 = catalog_for(&newer, &newer);
    let job = run_download(&lib, &srv, &cat2, "/file");
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error_code.as_deref(), Some("INPUT_REJECTED"));
    assert!(!lib.cx().dirs.models.join("ggml-tiny.bin").exists());
}

#[test]
fn start_up_marks_a_missing_model_and_verify_finds_corruption() {
    let lib = library(Setup::fake());
    let bytes = body();
    let cat = catalog_for(&bytes, &bytes);
    let srv = serve(bytes.clone());
    lib.write(|c| models::agree_and_queue(c, &cat, "whisper-tiny", 1))
        .unwrap();
    assert_eq!(
        run_download(&lib, &srv, &cat, "/file").status,
        JobStatus::Succeeded
    );
    let dir = lib.cx().dirs.models.clone();
    let file = dir.join("ggml-tiny.bin");
    let mut bad = bytes.clone();
    bad[5] ^= 1;
    std::fs::write(&file, &bad).unwrap();
    // Same size: only a full hash notices.
    lib.write(|c| models::reconcile(c, &dir, 5));
    assert_eq!(
        lib.write(|c| rows::get(c, "whisper-tiny").unwrap().unwrap().status),
        ModelStatus::Ready
    );
    assert_eq!(
        lib.write(|c| models::verify(c, &dir, "whisper-tiny", 6)),
        Some(ModelStatus::Corrupt)
    );
    std::fs::remove_file(&file).unwrap();
    lib.write(|c| models::reconcile(c, &dir, 7));
    assert_eq!(
        lib.write(|c| rows::get(c, "whisper-tiny").unwrap().unwrap().status),
        ModelStatus::Missing
    );
    // Remove forgets the consent and deletes the files.
    std::fs::write(&file, &bytes).unwrap();
    assert!(lib.write(|c| models::remove(c, &dir, "whisper-tiny")));
    assert!(!file.exists());
    assert!(lib.write(|c| rows::list(c).unwrap()).is_empty());
}

#[test]
fn offline_mode_blocks_without_connecting() {
    let lib = library(Setup::fake());
    let bytes = body();
    let cat = catalog_for(&bytes, &bytes);
    let srv = serve(bytes);
    lib.write(|c| models::agree_and_queue(c, &cat, "whisper-tiny", 1))
        .unwrap();
    let lease = lib
        .write(|c| jobs::claim(c, "test-owner", &[JobKind::ModelDownload], 1, false, 1))
        .unwrap()
        .unwrap();
    let id = lease.job.id.clone();
    let mut f = fetch_for(&srv);
    f.policy.offline = true;
    let url = format!("http://{}/file", srv.addr);
    models::run_with(lib.cx(), lease, &f, &cat, &|_| url.clone());
    let job = lib.job(&id);
    assert_eq!(job.status, JobStatus::Blocked);
    assert!(srv.seen.lock().unwrap().is_empty());
}
