//! Transcription models: the catalog, consent and the `model_download` job
//! (SPEC §17.1, §20.2; STATE.md F6.1; ADR-0026).
//!
//! - The catalog is `tools/models.json`, compiled in. The trust anchor is
//!   here: files come only from [`REPOSITORY`] at the pinned commit, and an
//!   entry without a pinned SHA-256 and size cannot be agreed to.
//! - Nothing is downloaded without consent (AC-20): [`agree_and_queue`]
//!   writes the consent row and the job in one transaction, and the job
//!   refuses to run when the row no longer matches the catalog entry.
//! - Every address the download connects to, redirects included, is vetted
//!   by the network guard's policy (public addresses, ports 80/443, Offline
//!   Mode), and redirects may only go to Hugging Face's own hosts over
//!   HTTPS. The pinned hashes decide whether a file is good, not the host.
//! - The file is written as `<file>.part` and renamed only after its size,
//!   SHA-256 and the SHA-1 published by whisper.cpp all match. Pause and
//!   Offline Mode keep the part file and resume with a range request;
//!   cancel and a hash mismatch delete it.

use crate::context::{Context, Store};
use crate::storage;
use crate::worker::{Stop, Work};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use vv_domain::error::ErrorCode;
use vv_netguard::{Policy, SystemResolver};
use vv_persistence::jobs::{self, Enqueued, JobError, JobKind, Lease, NewJob, Progress};
use vv_persistence::models::{self, Installation, ModelSpec, ModelStatus};
use vv_persistence::rusqlite::Connection;

/// The only repository model files may come from.
pub const REPOSITORY: &str = "https://huggingface.co/ggerganov/whisper.cpp";
/// Hosts a download may be redirected to (Hugging Face's CDN and storage).
pub const REDIRECT_HOSTS: &[&str] = &["huggingface.co", "hf.co"];
pub const PROVIDER: &str = "whisper_cpp";
const MAX_REDIRECTS: usize = 5;
const CATALOG_JSON: &str = include_str!("../../../tools/models.json");

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub schema_version: u32,
    pub provider: String,
    pub repository: String,
    pub revision: Option<String>,
    pub license: String,
    pub models: Vec<CatalogModel>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogModel {
    pub id: String,
    pub label: String,
    pub file: String,
    pub languages: String,
    pub sha1: String,
    pub sha256: Option<String>,
    pub size: Option<i64>,
    /// Memory whisper.cpp's README lists for the model; not our measurement
    /// (AI-A07). F6.6 replaces it with measured values.
    pub upstream_memory_mb: i64,
}

fn hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Catalog {
    /// Parses and checks a catalog: the anchored repository, a commit id,
    /// bare `ggml-*.bin` file names and well-formed hashes.
    pub fn parse(json: &str) -> Result<Catalog, String> {
        let c: Catalog = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if c.schema_version != 1 || c.provider != PROVIDER {
            return Err("unsupported catalog".into());
        }
        if c.repository != REPOSITORY {
            return Err("catalog points outside the allowed repository".into());
        }
        if c.revision.as_deref().is_some_and(|r| !hex(r, 40)) {
            return Err("revision is not a commit id".into());
        }
        for m in &c.models {
            let name_ok = m.file.starts_with("ggml-")
                && m.file.ends_with(".bin")
                && m.file
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'_')
                && !m.file.contains("..");
            if !name_ok || !m.id.starts_with("whisper-") || !hex(&m.sha1, 40) {
                return Err(format!("bad entry {}", m.id));
            }
            if m.sha256.as_deref().is_some_and(|h| !hex(h, 64)) || m.size.is_some_and(|s| s <= 0) {
                return Err(format!("bad pin for {}", m.id));
            }
        }
        Ok(c)
    }

    pub fn model(&self, id: &str) -> Option<&CatalogModel> {
        self.models.iter().find(|m| m.id == id)
    }

    /// What a consent for `m` records, or `None` while it is not pinned.
    pub fn spec(&self, m: &CatalogModel) -> Option<ModelSpec> {
        let rev = self.revision.as_deref()?;
        Some(ModelSpec {
            id: m.id.clone(),
            provider: self.provider.clone(),
            version: rev.to_owned(),
            file_name: m.file.clone(),
            size_bytes: m.size?,
            sha256: m.sha256.clone()?,
            license: self.license.clone(),
            source_url: format!("{}/resolve/{rev}/{}", self.repository, m.file),
        })
    }
}

/// The compiled-in catalog. A broken catalog is a build error caught by the
/// tests, so here it degrades to "no models".
pub fn catalog() -> &'static Catalog {
    static C: OnceLock<Catalog> = OnceLock::new();
    C.get_or_init(|| {
        Catalog::parse(CATALOG_JSON).unwrap_or(Catalog {
            schema_version: 1,
            provider: PROVIDER.into(),
            repository: REPOSITORY.into(),
            revision: None,
            license: String::new(),
            models: Vec::new(),
        })
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadPayload {
    pub model_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestError {
    UnknownModel,
    NotPinned,
    Db,
}

pub fn idempotency_key(spec: &ModelSpec) -> String {
    format!("model_download:{}:{}", spec.id, spec.sha256)
}

/// The user agreed to download `model_id` (§17.1): records the consent and
/// queues the download in one transaction. Asking again while a download is
/// open returns that job.
pub fn agree_and_queue(
    c: &mut Connection,
    cat: &Catalog,
    model_id: &str,
    now: i64,
) -> Result<Enqueued, RequestError> {
    let m = cat.model(model_id).ok_or(RequestError::UnknownModel)?;
    let spec = cat.spec(m).ok_or(RequestError::NotPinned)?;
    let tx = c.transaction().map_err(|_| RequestError::Db)?;
    let inst = models::consent(&tx, &spec, now).map_err(|_| RequestError::Db)?;
    if inst.status != ModelStatus::Ready {
        models::set_status(&tx, &spec.id, ModelStatus::Pending, now)
            .map_err(|_| RequestError::Db)?;
    }
    let mut job = NewJob::new(JobKind::ModelDownload, idempotency_key(&spec));
    job.payload = serde_json::to_value(DownloadPayload {
        model_id: spec.id.clone(),
    })
    .map_err(|_| RequestError::Db)?;
    // A 1.5 GB file over a flaky link: give it more tries than a page fetch.
    job.max_attempts = 6;
    let q = jobs::enqueue_in(&tx, &job, now).map_err(|_| RequestError::Db)?;
    tx.commit().map_err(|_| RequestError::Db)?;
    Ok(q)
}

/// Path of an installed model's file (inside `<data>/models`).
pub fn model_path(models_dir: &Path, inst: &Installation) -> PathBuf {
    models_dir.join(&inst.spec.file_name)
}

fn part_path(models_dir: &Path, file: &str) -> PathBuf {
    models_dir.join(format!("{file}.part"))
}

/// Start-up check (§26.2 "model missing/corrupt"): an installed model whose
/// file is gone or has the wrong size is marked so. Full hashes are checked
/// by [`verify`], which is slow for big files.
pub fn reconcile(c: &Connection, models_dir: &Path, now: i64) {
    let Ok(all) = models::list(c) else { return };
    for inst in all.into_iter().filter(|i| i.status != ModelStatus::Pending) {
        let len = fs::metadata(model_path(models_dir, &inst)).map(|m| m.len());
        let status = match len {
            Err(_) => ModelStatus::Missing,
            Ok(n) if n as i64 != inst.spec.size_bytes => ModelStatus::Corrupt,
            Ok(_) => inst.status,
        };
        if status != inst.status {
            let s = status;
            let _ = models::set_status(c, &inst.spec.id, s, now);
        }
    }
}

/// SHA-256 and SHA-1 of a file.
pub fn hash_file(path: &Path) -> io::Result<(String, String)> {
    let mut f = File::open(path)?;
    let mut a = Sha256::new();
    let mut b = Sha1::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        a.update(&buf[..n]);
        b.update(&buf[..n]);
    }
    Ok((to_hex(&a.finalize()), to_hex(&b.finalize())))
}

fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Hashes an installed model again; marks it ready or corrupt/missing.
pub fn verify(c: &Connection, models_dir: &Path, id: &str, now: i64) -> Option<ModelStatus> {
    let inst = models::get(c, id).ok().flatten()?;
    let path = model_path(models_dir, &inst);
    let status = match hash_file(&path) {
        Err(_) => ModelStatus::Missing,
        Ok((sha256, _)) if sha256 == inst.spec.sha256 => ModelStatus::Ready,
        Ok(_) => ModelStatus::Corrupt,
    };
    let r = if status == ModelStatus::Ready {
        models::mark_ready(c, id, now)
    } else {
        models::set_status(c, id, status, now)
    };
    r.ok().map(|_| status)
}

/// Removes a model: its row (and consent) and its files.
pub fn remove(c: &Connection, models_dir: &Path, id: &str) -> bool {
    match models::remove(c, id) {
        Ok(Some(inst)) => {
            let _ = fs::remove_file(model_path(models_dir, &inst));
            let _ = fs::remove_file(part_path(models_dir, &inst.spec.file_name));
            true
        }
        _ => false,
    }
}

// ---------------------------------------------------------------- download

/// Where a download may connect. The app uses [`Fetch::hugging_face`];
/// tests point it at a loopback server through `policy.test_targets`.
#[derive(Debug, Clone)]
pub struct Fetch {
    pub policy: Policy,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
}

impl Fetch {
    pub fn new(policy: Policy) -> Self {
        Fetch {
            policy,
            connect_timeout: Duration::from_secs(20),
            read_timeout: Duration::from_secs(60),
        }
    }

    /// May the download go to `url`? HTTPS to Hugging Face, or a loopback
    /// test target the policy names.
    pub fn allowed(&self, url: &str) -> bool {
        let Ok(u) = url::Url::parse(url) else {
            return false;
        };
        if !u.username().is_empty() || u.password().is_some() {
            return false;
        }
        let host = u.host_str().unwrap_or("").to_ascii_lowercase();
        if u.scheme() == "https" {
            return REDIRECT_HOSTS
                .iter()
                .any(|h| host == *h || host.ends_with(&format!(".{h}")));
        }
        if u.scheme() == "http" {
            let port = u.port_or_known_default().unwrap_or(0);
            let ip = host.trim_matches(['[', ']']).parse().ok();
            return ip.is_some_and(|ip| {
                self.policy
                    .test_targets
                    .contains(&SocketAddr::new(ip, port))
            });
        }
        false
    }

    fn agent(&self) -> ureq::Agent {
        let policy = self.policy.clone();
        ureq::AgentBuilder::new()
            .redirects(0)
            .timeout_connect(self.connect_timeout)
            .timeout_read(self.read_timeout)
            .user_agent("VideoVault")
            .resolver(move |netloc: &str| -> io::Result<Vec<SocketAddr>> {
                let (host, port) = netloc
                    .rsplit_once(':')
                    .and_then(|(h, p)| Some((h, p.parse::<u16>().ok()?)))
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "address"))?;
                policy
                    .vet(&SystemResolver, host, port)
                    .map(|a| vec![a])
                    .map_err(|_| io::Error::new(io::ErrorKind::PermissionDenied, "blocked"))
            })
            .build()
    }
}

/// How a download ended, before the hash check.
#[derive(Debug, PartialEq, Eq)]
pub enum Fetched {
    Complete,
    Stopped,
}

fn http_code(status: u16) -> ErrorCode {
    match status {
        401 | 403 => ErrorCode::AccessDenied,
        404 | 410 => ErrorCode::ContentUnavailable,
        429 => ErrorCode::RateLimited,
        500..=599 => ErrorCode::NetworkUnavailable,
        _ => ErrorCode::ExtractorFailed,
    }
}

/// Downloads `url` into `part` until it holds `size` bytes, resuming what
/// is already there. `stop` is asked between reads; `progress` gets the
/// byte count.
pub fn fetch_to_part(
    fetch: &Fetch,
    url: &str,
    part: &Path,
    size: u64,
    stop: &dyn Fn() -> bool,
    progress: &dyn Fn(u64),
) -> Result<Fetched, ErrorCode> {
    let mut have = fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    if have > size {
        let _ = fs::remove_file(part);
        have = 0;
    }
    if have == size {
        return Ok(Fetched::Complete);
    }
    let agent = fetch.agent();
    let mut current = url.to_owned();
    let mut resp = None;
    for _ in 0..=MAX_REDIRECTS {
        if fetch.policy.offline {
            return Err(ErrorCode::OfflineMode);
        }
        if !fetch.allowed(&current) {
            return Err(ErrorCode::UrlBlocked);
        }
        let mut req = agent.get(&current);
        if have > 0 {
            req = req.set("Range", &format!("bytes={have}-"));
        }
        let r = match req.call() {
            Ok(r) => r,
            Err(ureq::Error::Status(code, r)) if (300..400).contains(&code) => r,
            Err(ureq::Error::Status(416, _)) => {
                // Range past the end: start over.
                let _ = fs::remove_file(part);
                return Err(ErrorCode::NetworkUnavailable);
            }
            Err(ureq::Error::Status(code, _)) => return Err(http_code(code)),
            Err(ureq::Error::Transport(t)) => {
                return Err(match t.kind() {
                    ureq::ErrorKind::Dns | ureq::ErrorKind::ConnectionFailed
                        if t.to_string().contains("blocked") =>
                    {
                        ErrorCode::UrlBlocked
                    }
                    _ => ErrorCode::NetworkUnavailable,
                })
            }
        };
        if (300..400).contains(&r.status()) {
            let Some(loc) = r.header("location") else {
                return Err(ErrorCode::ExtractorFailed);
            };
            let next = url::Url::parse(&current)
                .and_then(|b| b.join(loc))
                .map_err(|_| ErrorCode::UrlBlocked)?;
            current = next.to_string();
            continue;
        }
        resp = Some(r);
        break;
    }
    let Some(resp) = resp else {
        return Err(ErrorCode::UrlBlocked);
    };
    let mut file = match resp.status() {
        206 if have > 0 => OpenOptions::new()
            .append(true)
            .open(part)
            .map_err(|_| ErrorCode::PathNotWritable)?,
        200 => {
            have = 0;
            File::create(part).map_err(|_| ErrorCode::PathNotWritable)?
        }
        s => return Err(http_code(s)),
    };
    let remaining = size - have;
    if let Some(len) = resp
        .header("content-length")
        .and_then(|v| v.parse::<u64>().ok())
    {
        if len != remaining {
            return Err(ErrorCode::ChecksumMismatch);
        }
    }
    let mut body = resp.into_reader().take(remaining + 1);
    let mut buf = vec![0u8; 256 * 1024];
    progress(have);
    loop {
        if stop() {
            let _ = file.flush();
            return Ok(Fetched::Stopped);
        }
        let n = match body.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => {
                let _ = file.flush();
                return Err(ErrorCode::NetworkUnavailable);
            }
        };
        if have + n as u64 > size {
            return Err(ErrorCode::ChecksumMismatch);
        }
        if let Err(e) = file.write_all(&buf[..n]) {
            return Err(storage::io_code(&e));
        }
        have += n as u64;
        progress(have);
    }
    file.sync_all().map_err(|e| storage::io_code(&e))?;
    if have != size {
        // The connection ended early; the part file is kept for a resume.
        return Err(ErrorCode::NetworkUnavailable);
    }
    Ok(Fetched::Complete)
}

/// The `model_download` job.
pub fn run<S: Store>(cx: &Context<S>, lease: Lease) {
    let fetch = Fetch::new(cx.run_policy());
    run_with(cx, lease, &fetch, catalog(), &|s| s.source_url.clone())
}

/// [`run`] with the catalog and the download URL given (tests serve the
/// file from a loopback server).
pub fn run_with<S: Store>(
    cx: &Context<S>,
    lease: Lease,
    fetch: &Fetch,
    cat: &Catalog,
    url_for: &dyn Fn(&ModelSpec) -> String,
) {
    let w = Work::new(cx, lease);
    let Ok(p) = serde_json::from_value::<DownloadPayload>(w.lease.job.payload.clone()) else {
        w.end_error(ErrorCode::InputRejected, None);
        return;
    };
    // Consent must still be on record and describe this catalog entry.
    let inst = cx
        .store
        .read(|c| models::get(c, &p.model_id).ok().flatten());
    let spec = cat.model(&p.model_id).and_then(|m| cat.spec(m));
    let (Some(inst), Some(spec)) = (inst, spec) else {
        w.end_error(ErrorCode::InputRejected, Some("no consent for this model"));
        return;
    };
    if !inst.matches(&spec) {
        w.end_error(
            ErrorCode::InputRejected,
            Some("consent is for another model file"),
        );
        return;
    }
    let dir = &cx.dirs.models;
    if fs::create_dir_all(dir).is_err() {
        w.end_error(ErrorCode::PathNotWritable, None);
        return;
    }
    let size = spec.size_bytes as u64;
    let target = dir.join(&spec.file_name);
    let part = part_path(dir, &spec.file_name);
    // Already there and good (e.g. a retry after the rename): just verify.
    if !(target.exists() && fs::metadata(&target).is_ok_and(|m| m.len() == size)) {
        let have = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        let need = size.saturating_sub(have) + (64 << 20);
        if storage::free_bytes(dir).is_some_and(|free| free < need) {
            w.end_error(ErrorCode::DiskFull, None);
            return;
        }
        let report = |done: u64| {
            w.progress(
                &Progress {
                    phase: Some("download".into()),
                    bytes_done: Some(done as i64),
                    bytes_total: Some(size as i64),
                    ..Default::default()
                },
                false,
            );
        };
        match fetch_to_part(
            fetch,
            &url_for(&spec),
            &part,
            size,
            &|| w.should_stop(),
            &report,
        ) {
            Ok(Fetched::Complete) => {}
            Ok(Fetched::Stopped) => {
                let stop = w.stopped().unwrap_or(Stop::Cancel);
                if stop == Stop::Cancel {
                    let _ = fs::remove_file(&part);
                }
                w.end_stopped(stop);
                return;
            }
            Err(code) => {
                if code == ErrorCode::ChecksumMismatch || w.stopped() == Some(Stop::Cancel) {
                    let _ = fs::remove_file(&part);
                }
                w.end_error(code, None);
                return;
            }
        }
        w.progress(
            &Progress {
                phase: Some("verify".into()),
                bytes_done: Some(size as i64),
                bytes_total: Some(size as i64),
                ..Default::default()
            },
            true,
        );
        let sha1 = cat
            .model(&spec.id)
            .map(|m| m.sha1.clone())
            .unwrap_or_default();
        match hash_file(&part) {
            Ok((a, b)) if a == spec.sha256 && b == sha1 => {}
            Ok(_) => {
                let _ = fs::remove_file(&part);
                w.end_error(ErrorCode::ChecksumMismatch, None);
                return;
            }
            Err(e) => {
                w.end_error(storage::io_code(&e), None);
                return;
            }
        }
        if fs::rename(&part, &target).is_err() {
            w.end_error(ErrorCode::PathNotWritable, None);
            return;
        }
    } else {
        match hash_file(&target) {
            Ok((a, _)) if a == spec.sha256 => {}
            _ => {
                let _ = fs::remove_file(&target);
                w.end_error(ErrorCode::ChecksumMismatch, None);
                return;
            }
        }
    }
    let now = cx.now();
    let (id, g) = (w.id().to_owned(), w.generation());
    // Cancelled or reclaimed at the last moment: the file is good but the
    // row stays as it is; the next download only verifies it.
    let _ = cx.store.write(|c| {
        jobs::succeed_with(c, &id, g, now, |tx| {
            models::mark_ready(tx, &spec.id, now).map_err(|_| JobError::Invalid("model row"))
        })
    });
    cx.events.job_changed(&id, None);
}
