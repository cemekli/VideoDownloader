//! Persistent job queue (SPEC §11 `Job`, §12.3, §13; STATE.md F3.1;
//! ADR-0019).
//!
//! Everything a worker does to a job goes through this module, and every
//! change is one SQLite transaction. The rules:
//!
//! - **At least once, idempotent output.** A worker claims a job with a
//!   lease and a new generation. Heartbeats, progress and the final result
//!   carry that generation; once the job was reclaimed, cancelled or
//!   retried, the old worker's calls fail with [`JobError::Stale`] and its
//!   output is never published ([`succeed_with`]).
//! - **One open job per idempotency key** (a partial UNIQUE index).
//! - **Dependencies.** A child waits `blocked` (reason `parent`) until its
//!   parent succeeds. If the parent fails for good or is cancelled, the
//!   child is cancelled with `dependency_failed`; when the parent is retried
//!   those children come back, but children the user cancelled stay
//!   cancelled (§13.1, AC-58).
//! - **Retry.** Transient failures wait 5 s, 30 s, 120 s (plus jitter, or a
//!   longer Retry-After) up to `max_attempts` attempts in all; then the job
//!   fails. A move to `blocked` does not use up an attempt (§12.3).
//! - **Crash recovery.** [`recover`] runs at startup before any worker: jobs
//!   left running go back to the queue with a new generation, `pausing`
//!   ends `paused`, `canceling` ends `canceled` (§13.2).
//!
//! Scheduling (how many workers, which kinds) and the handlers themselves
//! live in the app; this module only keeps the state honest.

use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde::Serialize;
use std::fmt;

/// Lease length and heartbeat interval suggested by §13.2.
pub const LEASE_MS: i64 = 30_000;
pub const HEARTBEAT_MS: i64 = 10_000;
/// First attempt plus at most three retries (§13.3).
pub const DEFAULT_MAX_ATTEMPTS: i64 = 4;
/// Waits before retry 1, 2 and 3 (§13.3).
pub const BACKOFF_MS: [i64; 3] = [5_000, 30_000, 120_000];
/// Blocked reason of a child waiting for its parent.
pub const WAITING_FOR_PARENT: &str = "parent";
/// Blocked reason while Offline Mode is on.
pub const OFFLINE: &str = "offline";

macro_rules! str_enum {
    ($(#[$m:meta])* $name:ident { $($v:ident => $s:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($v),+ }
        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$v),+];
            pub fn as_str(self) -> &'static str { match self { $($name::$v => $s),+ } }
            pub fn parse(s: &str) -> Option<Self> { match s { $($s => Some($name::$v),)+ _ => None } }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.as_str()) }
        }
    };
}

str_enum!(
    /// Job types of §13.1. Some are only used from later phases on.
    JobKind {
        Metadata => "metadata",
        ProbeMedia => "probe_media",
        HashMedia => "hash_media",
        Download => "download",
        MergeMedia => "merge_media",
        Thumbnail => "thumbnail",
        ExtractAudio => "extract_audio",
        FetchCaptions => "fetch_captions",
        Transcription => "transcription",
        FtsIndex => "fts_index",
        Ocr => "ocr",
        Embedding => "embedding",
        VisualEmbedding => "visual_embedding",
        ModelDownload => "model_download",
        Reindex => "reindex",
        StorageMove => "storage_move",
    }
);

impl JobKind {
    /// Needs the network: blocked while Offline Mode is on (DATA-05).
    pub fn uses_network(self) -> bool {
        matches!(
            self,
            JobKind::Metadata | JobKind::Download | JobKind::FetchCaptions | JobKind::ModelDownload
        )
    }

    /// Can stop at a checkpoint and continue later (§12.3 pause).
    pub fn pausable(self) -> bool {
        matches!(
            self,
            JobKind::Download | JobKind::StorageMove | JobKind::ModelDownload | JobKind::Reindex
        )
    }
}

str_enum!(
    /// §12.3 job states.
    JobStatus {
        Queued => "queued",
        Blocked => "blocked",
        Running => "running",
        RetryWait => "retry_wait",
        Pausing => "pausing",
        Paused => "paused",
        Canceling => "canceling",
        Canceled => "canceled",
        Succeeded => "succeeded",
        Failed => "failed",
    }
);

impl JobStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            JobStatus::Canceled | JobStatus::Succeeded | JobStatus::Failed
        )
    }
    /// A worker holds a lease.
    pub fn is_active(self) -> bool {
        matches!(
            self,
            JobStatus::Running | JobStatus::Pausing | JobStatus::Canceling
        )
    }
}

str_enum!(
    /// Why a job was cancelled (§11 `cancel_reason`, §13.1).
    CancelReason {
        User => "user",
        DependencyFailed => "dependency_failed",
        ItemRemoved => "item_removed",
        Shutdown => "shutdown",
    }
);

#[derive(Debug)]
pub enum JobError {
    Sqlite(rusqlite::Error),
    NotFound,
    /// The caller's generation is not the job's current one, or the job is
    /// no longer held by a worker: the job was reclaimed, cancelled or
    /// retried meanwhile. The caller must stop and publish nothing.
    Stale,
    /// The action is not allowed from the job's current state.
    InvalidTransition {
        from: JobStatus,
        action: &'static str,
    },
    /// A value broke a rule (e.g. a payload that is not a JSON object).
    Invalid(&'static str),
}

impl fmt::Display for JobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobError::Sqlite(e) => write!(f, "sqlite: {e}"),
            JobError::NotFound => f.write_str("job not found"),
            JobError::Stale => f.write_str("stale job generation"),
            JobError::InvalidTransition { from, action } => {
                write!(f, "cannot {action} a {from} job")
            }
            JobError::Invalid(what) => write!(f, "invalid {what}"),
        }
    }
}

impl std::error::Error for JobError {}

impl From<rusqlite::Error> for JobError {
    fn from(e: rusqlite::Error) -> Self {
        JobError::Sqlite(e)
    }
}

pub type Result<T> = std::result::Result<T, JobError>;

/// A job row. Field names follow §11; serialized camelCase for the UI.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub kind: JobKind,
    pub item_id: Option<String>,
    pub parent_job_id: Option<String>,
    pub status: JobStatus,
    pub payload_version: i64,
    pub payload: serde_json::Value,
    pub idempotency_key: String,
    pub priority: i64,
    pub attempts: i64,
    pub max_attempts: i64,
    pub generation: i64,
    #[serde(skip)]
    pub lease_owner: Option<String>,
    #[serde(skip)]
    pub lease_expires_at: Option<i64>,
    pub next_retry_at: Option<i64>,
    pub cancel_requested: bool,
    pub cancel_reason: Option<CancelReason>,
    pub blocked_reason: Option<String>,
    pub progress: Progress,
    pub checkpoint_ref: Option<String>,
    pub error_code: Option<String>,
    pub redacted_error_message: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
}

/// §13.4 progress. Unknown totals stay `None`; no percentage is invented.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: Option<String>,
    pub bytes_done: Option<i64>,
    pub bytes_total: Option<i64>,
    pub units_done: Option<i64>,
    pub units_total: Option<i64>,
}

/// What to enqueue.
#[derive(Debug, Clone)]
pub struct NewJob {
    pub kind: JobKind,
    pub item_id: Option<String>,
    pub parent_job_id: Option<String>,
    /// A JSON object without secrets (§11).
    pub payload: serde_json::Value,
    /// `(job type, input fingerprint, configuration version)` (§13.2).
    pub idempotency_key: String,
    pub priority: i64,
    pub max_attempts: i64,
}

impl NewJob {
    pub fn new(kind: JobKind, idempotency_key: impl Into<String>) -> Self {
        NewJob {
            kind,
            item_id: None,
            parent_job_id: None,
            payload: serde_json::json!({}),
            idempotency_key: idempotency_key.into(),
            priority: 0,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enqueued {
    pub id: String,
    /// An open job with the same idempotency key already existed; nothing
    /// new was written.
    pub existing: bool,
}

/// What a worker holds after [`claim`].
#[derive(Debug, Clone, PartialEq)]
pub struct Lease {
    pub job: Job,
    pub owner: String,
    pub generation: i64,
}

const COLUMNS: &str = "id, type, item_id, parent_job_id, status, payload_version, payload, idempotency_key, priority, attempts, max_attempts, generation, lease_owner, lease_expires_at, next_retry_at, cancel_requested, cancel_reason, blocked_reason, phase, bytes_done, bytes_total, units_done, units_total, checkpoint_ref, error_code, redacted_error_message, created_at, updated_at, finished_at";

fn from_row(r: &Row<'_>) -> rusqlite::Result<Job> {
    let bad = |i: usize, what: &str| {
        rusqlite::Error::FromSqlConversionFailure(
            i,
            rusqlite::types::Type::Text,
            format!("unknown {what}").into(),
        )
    };
    let kind: String = r.get(1)?;
    let status: String = r.get(4)?;
    let payload: String = r.get(6)?;
    let reason: Option<String> = r.get(16)?;
    Ok(Job {
        id: r.get(0)?,
        kind: JobKind::parse(&kind).ok_or_else(|| bad(1, "job type"))?,
        item_id: r.get(2)?,
        parent_job_id: r.get(3)?,
        status: JobStatus::parse(&status).ok_or_else(|| bad(4, "job status"))?,
        payload_version: r.get(5)?,
        payload: serde_json::from_str(&payload).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(e))
        })?,
        idempotency_key: r.get(7)?,
        priority: r.get(8)?,
        attempts: r.get(9)?,
        max_attempts: r.get(10)?,
        generation: r.get(11)?,
        lease_owner: r.get(12)?,
        lease_expires_at: r.get(13)?,
        next_retry_at: r.get(14)?,
        cancel_requested: r.get::<_, i64>(15)? != 0,
        cancel_reason: match reason {
            Some(s) => Some(CancelReason::parse(&s).ok_or_else(|| bad(16, "cancel reason"))?),
            None => None,
        },
        blocked_reason: r.get(17)?,
        progress: Progress {
            phase: r.get(18)?,
            bytes_done: r.get(19)?,
            bytes_total: r.get(20)?,
            units_done: r.get(21)?,
            units_total: r.get(22)?,
        },
        checkpoint_ref: r.get(23)?,
        error_code: r.get(24)?,
        redacted_error_message: r.get(25)?,
        created_at: r.get(26)?,
        updated_at: r.get(27)?,
        finished_at: r.get(28)?,
    })
}

pub fn get(c: &Connection, id: &str) -> Result<Job> {
    c.query_row(
        &format!("SELECT {COLUMNS} FROM job WHERE id = ?1"),
        [id],
        from_row,
    )
    .optional()?
    .ok_or(JobError::NotFound)
}

/// Jobs for the Downloads screen and diagnostics, newest first.
pub fn list(c: &Connection, statuses: &[JobStatus], limit: i64) -> Result<Vec<Job>> {
    let filter = if statuses.is_empty() {
        String::new()
    } else {
        format!(
            "WHERE status IN ({})",
            statuses
                .iter()
                .map(|s| format!("'{}'", s.as_str()))
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    let mut st = c.prepare(&format!(
        "SELECT {COLUMNS} FROM job {filter} ORDER BY created_at DESC, id LIMIT ?1"
    ))?;
    let rows = st.query_map([limit], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn children(c: &Connection, parent_id: &str) -> Result<Vec<Job>> {
    let mut st = c.prepare(&format!(
        "SELECT {COLUMNS} FROM job WHERE parent_job_id = ?1 ORDER BY created_at, id"
    ))?;
    let rows = st.query_map([parent_id], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// One row of attempt history: `(attempt, generation, outcome, error_code)`.
pub type AttemptRow = (i64, i64, Option<String>, Option<String>);

/// Attempt history, oldest first.
pub fn attempts(c: &Connection, id: &str) -> Result<Vec<AttemptRow>> {
    let mut st = c.prepare("SELECT attempt, generation, outcome, error_code FROM job_attempt WHERE job_id = ?1 ORDER BY attempt")?;
    let rows = st.query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn load(tx: &Transaction<'_>, id: &str) -> Result<Job> {
    get(tx, id)
}

/// Adds a job, or returns the open one with the same idempotency key.
/// A child whose parent has not succeeded waits `blocked` (`parent`); a
/// child of a failed or cancelled parent starts cancelled
/// (`dependency_failed`).
pub fn enqueue(c: &mut Connection, new: &NewJob, now: i64) -> Result<Enqueued> {
    if !new.payload.is_object() {
        return Err(JobError::Invalid("job payload"));
    }
    if new.max_attempts < 1 {
        return Err(JobError::Invalid("max attempts"));
    }
    let tx = c.transaction()?;
    let open: Option<String> = tx
        .query_row("SELECT id FROM job WHERE idempotency_key = ?1 AND status NOT IN ('succeeded','failed','canceled')", [&new.idempotency_key], |r| r.get(0))
        .optional()?;
    if let Some(id) = open {
        return Ok(Enqueued { id, existing: true });
    }
    let (status, blocked, cancel) = match &new.parent_job_id {
        None => (JobStatus::Queued, None, None),
        Some(p) => match load(&tx, p)?.status {
            JobStatus::Succeeded => (JobStatus::Queued, None, None),
            JobStatus::Failed | JobStatus::Canceled => (
                JobStatus::Canceled,
                None,
                Some(CancelReason::DependencyFailed),
            ),
            _ => (JobStatus::Blocked, Some(WAITING_FOR_PARENT), None),
        },
    };
    let id = uuid::Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO job (id, type, item_id, parent_job_id, status, payload, idempotency_key, priority, max_attempts, cancel_reason, blocked_reason, created_at, updated_at, finished_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12, ?13)",
        params![
            id,
            new.kind.as_str(),
            new.item_id,
            new.parent_job_id,
            status.as_str(),
            new.payload.to_string(),
            new.idempotency_key,
            new.priority,
            new.max_attempts,
            cancel.map(|r| r.as_str()),
            blocked,
            now,
            status.is_terminal().then_some(now),
        ],
    )?;
    tx.commit()?;
    Ok(Enqueued {
        id,
        existing: false,
    })
}

/// Takes the next ready job of these kinds for `owner`: highest priority,
/// then oldest. A `retry_wait` job is ready once its time has come.
/// `max_running` caps how many jobs of these kinds may run at once (§13.3).
/// Network jobs are skipped while `offline` (they are blocked by
/// [`set_offline`]; this is the second guard).
pub fn claim(
    c: &mut Connection,
    owner: &str,
    kinds: &[JobKind],
    max_running: usize,
    offline: bool,
    now: i64,
) -> Result<Option<Lease>> {
    if kinds.is_empty() {
        return Ok(None);
    }
    let kinds: Vec<JobKind> = kinds
        .iter()
        .copied()
        .filter(|k| !(offline && k.uses_network()))
        .collect();
    if kinds.is_empty() {
        return Ok(None);
    }
    let in_list = kinds
        .iter()
        .map(|k| format!("'{}'", k.as_str()))
        .collect::<Vec<_>>()
        .join(",");
    let tx = c.transaction()?;
    let running: i64 = tx.query_row(&format!("SELECT count(*) FROM job WHERE type IN ({in_list}) AND status IN ('running','pausing','canceling')"), [], |r| r.get(0))?;
    if running as usize >= max_running {
        return Ok(None);
    }
    let id: Option<String> = tx
        .query_row(
            &format!(
                "SELECT id FROM job WHERE type IN ({in_list})
                   AND (status = 'queued' OR (status = 'retry_wait' AND next_retry_at <= ?1))
                 ORDER BY priority DESC, created_at, id LIMIT 1"
            ),
            [now],
            |r| r.get(0),
        )
        .optional()?;
    let Some(id) = id else { return Ok(None) };
    tx.execute(
        "UPDATE job SET status = 'running', attempts = attempts + 1, generation = generation + 1,
                lease_owner = ?2, lease_expires_at = ?3, next_retry_at = NULL, cancel_requested = 0,
                error_code = NULL, redacted_error_message = NULL, updated_at = ?4
          WHERE id = ?1",
        params![id, owner, now + LEASE_MS, now],
    )?;
    let job = load(&tx, &id)?;
    tx.execute(
        "INSERT INTO job_attempt (job_id, attempt, generation, started_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (job_id, attempt) DO UPDATE SET generation = excluded.generation, started_at = excluded.started_at, ended_at = NULL, outcome = NULL, error_code = NULL",
        params![id, attempt_no(&tx, &id)?, job.generation, now],
    )?;
    tx.commit()?;
    let generation = job.generation;
    Ok(Some(Lease {
        job,
        owner: owner.to_owned(),
        generation,
    }))
}

/// Attempt numbers keep counting across user retries (history is kept).
fn attempt_no(tx: &Transaction<'_>, id: &str) -> Result<i64> {
    let open: Option<i64> = tx.query_row(
        "SELECT max(attempt) FROM job_attempt WHERE job_id = ?1 AND outcome IS NULL",
        [id],
        |r| r.get(0),
    )?;
    if let Some(n) = open {
        return Ok(n);
    }
    let last: Option<i64> = tx.query_row(
        "SELECT max(attempt) FROM job_attempt WHERE job_id = ?1",
        [id],
        |r| r.get(0),
    )?;
    Ok(last.unwrap_or(0) + 1)
}

fn end_attempt(
    tx: &Transaction<'_>,
    id: &str,
    generation: i64,
    outcome: &str,
    code: Option<&str>,
    now: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE job_attempt SET ended_at = ?3, outcome = ?4, error_code = ?5 WHERE job_id = ?1 AND generation = ?2 AND outcome IS NULL",
        params![id, generation, now, outcome, code],
    )?;
    Ok(())
}

/// Loads the job and checks that `generation` still holds it.
fn held(tx: &Transaction<'_>, id: &str, generation: i64) -> Result<Job> {
    let job = load(tx, id)?;
    if job.generation != generation || !job.status.is_active() {
        return Err(JobError::Stale);
    }
    Ok(job)
}

/// What the worker should do after a heartbeat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Continue,
    /// Stop at the next checkpoint, then [`confirm_paused`].
    Pause,
    /// Stop the process tree, then [`confirm_canceled`].
    Cancel,
}

/// Extends the lease and tells the worker whether to go on.
pub fn heartbeat(c: &mut Connection, id: &str, generation: i64, now: i64) -> Result<Signal> {
    let tx = c.transaction()?;
    let job = held(&tx, id, generation)?;
    tx.execute(
        "UPDATE job SET lease_expires_at = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, now + LEASE_MS, now],
    )?;
    tx.commit()?;
    Ok(match job.status {
        JobStatus::Canceling => Signal::Cancel,
        JobStatus::Pausing => Signal::Pause,
        _ => Signal::Continue,
    })
}

/// Records progress; also a heartbeat.
pub fn report_progress(
    c: &mut Connection,
    id: &str,
    generation: i64,
    p: &Progress,
    now: i64,
) -> Result<Signal> {
    let tx = c.transaction()?;
    held(&tx, id, generation)?;
    tx.execute(
        "UPDATE job SET phase = ?2, bytes_done = ?3, bytes_total = ?4, units_done = ?5, units_total = ?6, lease_expires_at = ?7, updated_at = ?8 WHERE id = ?1",
        params![id, p.phase, p.bytes_done, p.bytes_total, p.units_done, p.units_total, now + LEASE_MS, now],
    )?;
    tx.commit()?;
    heartbeat(c, id, generation, now)
}

/// Saves a checkpoint reference (e.g. a partial file the next attempt may
/// continue from). Never a signed URL (§13.4).
pub fn set_checkpoint(
    c: &mut Connection,
    id: &str,
    generation: i64,
    checkpoint: Option<&str>,
    now: i64,
) -> Result<()> {
    let tx = c.transaction()?;
    held(&tx, id, generation)?;
    tx.execute(
        "UPDATE job SET checkpoint_ref = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, checkpoint, now],
    )?;
    tx.commit()?;
    Ok(())
}

/// Marks the job succeeded.
pub fn succeed(c: &mut Connection, id: &str, generation: i64, now: i64) -> Result<()> {
    succeed_with(c, id, generation, now, |_| Ok(()))
}

/// Publishes the job's output and marks it succeeded in one transaction,
/// only while `generation` still holds the job: a worker that was reclaimed
/// or cancelled cannot publish (§13.2). `publish` writes the domain rows
/// (e.g. the media asset) on the same transaction. Waiting children are
/// released to the queue.
pub fn succeed_with<F>(
    c: &mut Connection,
    id: &str,
    generation: i64,
    now: i64,
    publish: F,
) -> Result<()>
where
    F: FnOnce(&Transaction<'_>) -> Result<()>,
{
    let tx = c.transaction()?;
    let job = held(&tx, id, generation)?;
    if job.status == JobStatus::Canceling {
        return Err(JobError::InvalidTransition {
            from: job.status,
            action: "finish",
        });
    }
    publish(&tx)?;
    tx.execute(
        "UPDATE job SET status = 'succeeded', lease_owner = NULL, lease_expires_at = NULL, cancel_requested = 0, finished_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    end_attempt(&tx, id, generation, "succeeded", None, now)?;
    tx.execute(
        "UPDATE job SET status = 'queued', blocked_reason = NULL, updated_at = ?2 WHERE parent_job_id = ?1 AND status = 'blocked' AND blocked_reason = ?3",
        params![id, now, WAITING_FOR_PARENT],
    )?;
    tx.commit()?;
    Ok(())
}

/// How a failure is treated (§13.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// Network drop, timeout, temporary server error, rate limit: retried
    /// while attempts remain. `retry_after_ms` from the server, if any.
    Transient { retry_after_ms: Option<i64> },
    /// DRM, unsupported, access denied, bad input: never retried
    /// automatically.
    Permanent,
}

/// Wait before the next attempt after `attempts` attempts (1-based), with
/// up to 20% jitter derived from the job id so tests are repeatable.
pub fn backoff_ms(id: &str, attempts: i64, retry_after_ms: Option<i64>) -> i64 {
    let base = BACKOFF_MS[((attempts - 1).max(0) as usize).min(BACKOFF_MS.len() - 1)];
    let seed = id.bytes().fold(attempts as u64, |h, b| {
        h.wrapping_mul(31).wrapping_add(b as u64)
    });
    let jitter = (seed % 201) as i64 * base / 1000; // 0..=20%
    (base + jitter).max(retry_after_ms.unwrap_or(0))
}

/// Ends the attempt with an error: `retry_wait` while attempts remain for a
/// transient error, otherwise `failed` (children cancelled with
/// `dependency_failed`). `message` must already be redacted (§20.3).
pub fn fail(
    c: &mut Connection,
    id: &str,
    generation: i64,
    failure: &Failure,
    error_code: &str,
    message: Option<&str>,
    now: i64,
) -> Result<JobStatus> {
    let tx = c.transaction()?;
    let job = held(&tx, id, generation)?;
    let retry = match failure {
        Failure::Transient { retry_after_ms }
            if job.attempts < job.max_attempts && job.status == JobStatus::Running =>
        {
            Some(backoff_ms(id, job.attempts, *retry_after_ms))
        }
        _ => None,
    };
    let status = if let Some(wait) = retry {
        tx.execute(
            "UPDATE job SET status = 'retry_wait', next_retry_at = ?2, lease_owner = NULL, lease_expires_at = NULL, error_code = ?3, redacted_error_message = ?4, updated_at = ?5 WHERE id = ?1",
            params![id, now + wait, error_code, message, now],
        )?;
        end_attempt(&tx, id, generation, "retry", Some(error_code), now)?;
        JobStatus::RetryWait
    } else {
        tx.execute(
            "UPDATE job SET status = 'failed', lease_owner = NULL, lease_expires_at = NULL, cancel_requested = 0, error_code = ?2, redacted_error_message = ?3, finished_at = ?4, updated_at = ?4 WHERE id = ?1",
            params![id, error_code, message, now],
        )?;
        end_attempt(&tx, id, generation, "failed", Some(error_code), now)?;
        cancel_dependents(&tx, id, now)?;
        JobStatus::Failed
    };
    tx.commit()?;
    Ok(status)
}

/// Cancels the open descendants of a job that failed or was cancelled
/// (§13.1). Running children are asked to stop (`canceling`).
fn cancel_dependents(tx: &Transaction<'_>, parent: &str, now: i64) -> Result<()> {
    let kids: Vec<(String, String)> = {
        let mut st = tx.prepare("SELECT id, status FROM job WHERE parent_job_id = ?1 AND status NOT IN ('succeeded','failed','canceled')")?;
        let rows = st.query_map([parent], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for (kid, status) in kids {
        if JobStatus::parse(&status).is_some_and(JobStatus::is_active) {
            tx.execute(
                "UPDATE job SET status = 'canceling', cancel_requested = 1, cancel_reason = 'dependency_failed', updated_at = ?2 WHERE id = ?1",
                params![kid, now],
            )?;
        } else {
            tx.execute(
                "UPDATE job SET status = 'canceled', cancel_reason = 'dependency_failed', blocked_reason = NULL, next_retry_at = NULL, finished_at = ?2, updated_at = ?2 WHERE id = ?1",
                params![kid, now],
            )?;
            cancel_dependents(tx, &kid, now)?;
        }
    }
    Ok(())
}

/// Brings back the descendants cancelled because this job failed: they wait
/// for it again. Children cancelled for any other reason stay cancelled.
fn revive_dependents(tx: &Transaction<'_>, parent: &str, now: i64) -> Result<()> {
    let kids: Vec<String> = {
        let mut st = tx.prepare("SELECT id FROM job WHERE parent_job_id = ?1 AND status = 'canceled' AND cancel_reason = 'dependency_failed'")?;
        let rows = st.query_map([parent], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for kid in kids {
        tx.execute(
            "UPDATE job SET status = 'blocked', blocked_reason = ?2, cancel_reason = NULL, cancel_requested = 0, finished_at = NULL, updated_at = ?3 WHERE id = ?1",
            params![kid, WAITING_FOR_PARENT, now],
        )?;
        revive_dependents(tx, &kid, now)?;
    }
    Ok(())
}

/// User or app cancel. A waiting job ends `canceled` at once; a running one
/// becomes `canceling` and the worker finishes with [`confirm_canceled`]
/// after stopping its process tree (AC-10).
pub fn cancel(c: &mut Connection, id: &str, reason: CancelReason, now: i64) -> Result<JobStatus> {
    let tx = c.transaction()?;
    let job = load(&tx, id)?;
    let status = match job.status {
        JobStatus::Queued | JobStatus::Blocked | JobStatus::RetryWait | JobStatus::Paused => {
            tx.execute(
                "UPDATE job SET status = 'canceled', cancel_reason = ?2, blocked_reason = NULL, next_retry_at = NULL, finished_at = ?3, updated_at = ?3 WHERE id = ?1",
                params![id, reason.as_str(), now],
            )?;
            cancel_dependents(&tx, id, now)?;
            JobStatus::Canceled
        }
        JobStatus::Running | JobStatus::Pausing => {
            tx.execute(
                "UPDATE job SET status = 'canceling', cancel_requested = 1, cancel_reason = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, reason.as_str(), now],
            )?;
            JobStatus::Canceling
        }
        JobStatus::Canceling => JobStatus::Canceling,
        from => {
            return Err(JobError::InvalidTransition {
                from,
                action: "cancel",
            })
        }
    };
    tx.commit()?;
    Ok(status)
}

/// The worker stopped after a cancel request.
pub fn confirm_canceled(c: &mut Connection, id: &str, generation: i64, now: i64) -> Result<()> {
    let tx = c.transaction()?;
    let job = held(&tx, id, generation)?;
    if job.status != JobStatus::Canceling {
        return Err(JobError::InvalidTransition {
            from: job.status,
            action: "confirm cancel of",
        });
    }
    finish_cancel(&tx, id, generation, now)?;
    tx.commit()?;
    Ok(())
}

fn finish_cancel(tx: &Transaction<'_>, id: &str, generation: i64, now: i64) -> Result<()> {
    tx.execute(
        "UPDATE job SET status = 'canceled', lease_owner = NULL, lease_expires_at = NULL, finished_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    end_attempt(tx, id, generation, "canceled", None, now)?;
    cancel_dependents(tx, id, now)
}

/// Asks a running job to pause. Kinds that cannot pause are refused, so a
/// cancel is never shown as a pause (§12.3).
pub fn pause(c: &mut Connection, id: &str, now: i64) -> Result<JobStatus> {
    let tx = c.transaction()?;
    let job = load(&tx, id)?;
    if !job.kind.pausable() {
        return Err(JobError::InvalidTransition {
            from: job.status,
            action: "pause",
        });
    }
    let status = match job.status {
        JobStatus::Running => {
            tx.execute(
                "UPDATE job SET status = 'pausing', updated_at = ?2 WHERE id = ?1",
                params![id, now],
            )?;
            JobStatus::Pausing
        }
        JobStatus::Queued | JobStatus::RetryWait => {
            tx.execute("UPDATE job SET status = 'paused', next_retry_at = NULL, updated_at = ?2 WHERE id = ?1", params![id, now])?;
            JobStatus::Paused
        }
        s @ (JobStatus::Pausing | JobStatus::Paused) => s,
        from => {
            return Err(JobError::InvalidTransition {
                from,
                action: "pause",
            })
        }
    };
    tx.commit()?;
    Ok(status)
}

/// The worker stopped at a checkpoint after a pause request. A pause does
/// not use up an attempt.
pub fn confirm_paused(c: &mut Connection, id: &str, generation: i64, now: i64) -> Result<()> {
    let tx = c.transaction()?;
    let job = held(&tx, id, generation)?;
    if job.status != JobStatus::Pausing {
        return Err(JobError::InvalidTransition {
            from: job.status,
            action: "confirm pause of",
        });
    }
    tx.execute(
        "UPDATE job SET status = 'paused', attempts = max(attempts - 1, 0), lease_owner = NULL, lease_expires_at = NULL, updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    end_attempt(&tx, id, generation, "paused", None, now)?;
    tx.commit()?;
    Ok(())
}

/// Continues a paused job.
pub fn resume(c: &mut Connection, id: &str, now: i64) -> Result<()> {
    let tx = c.transaction()?;
    let job = load(&tx, id)?;
    if job.status != JobStatus::Paused {
        return Err(JobError::InvalidTransition {
            from: job.status,
            action: "resume",
        });
    }
    tx.execute(
        "UPDATE job SET status = 'queued', updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    tx.commit()?;
    Ok(())
}

/// Moves a job to `blocked` because the user or the environment must act
/// (disk full, login required…). From `running` the worker passes its
/// generation; the attempt is not counted (§12.3).
pub fn block(
    c: &mut Connection,
    id: &str,
    generation: Option<i64>,
    reason: &str,
    now: i64,
) -> Result<()> {
    if reason.is_empty() || reason == WAITING_FOR_PARENT {
        return Err(JobError::Invalid("blocked reason"));
    }
    let tx = c.transaction()?;
    let job = load(&tx, id)?;
    match (job.status, generation) {
        (JobStatus::Running | JobStatus::Pausing, Some(g)) => {
            held(&tx, id, g)?;
            tx.execute(
                "UPDATE job SET status = 'blocked', blocked_reason = ?2, attempts = max(attempts - 1, 0), lease_owner = NULL, lease_expires_at = NULL, updated_at = ?3 WHERE id = ?1",
                params![id, reason, now],
            )?;
            end_attempt(&tx, id, g, "blocked", None, now)?;
        }
        (JobStatus::Queued | JobStatus::RetryWait | JobStatus::Paused, _) => {
            tx.execute(
                "UPDATE job SET status = 'blocked', blocked_reason = ?2, next_retry_at = NULL, updated_at = ?3 WHERE id = ?1",
                params![id, reason, now],
            )?;
        }
        (from, _) => {
            return Err(JobError::InvalidTransition {
                from,
                action: "block",
            })
        }
    }
    tx.commit()?;
    Ok(())
}

/// The cause is gone: a job blocked for `reason` goes back to the queue.
/// A child waiting for its parent is not touched.
pub fn unblock(c: &mut Connection, id: &str, now: i64) -> Result<()> {
    let tx = c.transaction()?;
    let job = load(&tx, id)?;
    if job.status != JobStatus::Blocked || job.blocked_reason.as_deref() == Some(WAITING_FOR_PARENT)
    {
        return Err(JobError::InvalidTransition {
            from: job.status,
            action: "unblock",
        });
    }
    tx.execute(
        "UPDATE job SET status = 'queued', blocked_reason = NULL, updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    tx.commit()?;
    Ok(())
}

/// Offline Mode on: waiting network jobs become `blocked` (`offline`) and
/// running ones are asked to stop. Off: offline-blocked jobs return to the
/// queue. Returns how many jobs changed. A retry timer never starts a
/// network request while offline (§13.3).
pub fn set_offline(c: &mut Connection, offline: bool, now: i64) -> Result<usize> {
    let kinds = JobKind::ALL
        .iter()
        .filter(|k| k.uses_network())
        .map(|k| format!("'{}'", k.as_str()))
        .collect::<Vec<_>>()
        .join(",");
    let tx = c.transaction()?;
    let n = if offline {
        tx.execute(
            &format!("UPDATE job SET status = 'blocked', blocked_reason = ?1, next_retry_at = NULL, updated_at = ?2 WHERE type IN ({kinds}) AND status IN ('queued','retry_wait','paused')"),
            params![OFFLINE, now],
        )?
    } else {
        tx.execute("UPDATE job SET status = 'queued', blocked_reason = NULL, updated_at = ?2 WHERE status = 'blocked' AND blocked_reason = ?1", params![OFFLINE, now])?
    };
    tx.commit()?;
    Ok(n)
}

/// Running network jobs a worker must stop because Offline Mode came on;
/// each worker then calls [`block`] with `OFFLINE` and its generation.
pub fn running_network_jobs(c: &Connection) -> Result<Vec<Job>> {
    let kinds = JobKind::ALL
        .iter()
        .filter(|k| k.uses_network())
        .map(|k| format!("'{}'", k.as_str()))
        .collect::<Vec<_>>()
        .join(",");
    let mut st = c.prepare(&format!(
        "SELECT {COLUMNS} FROM job WHERE type IN ({kinds}) AND status IN ('running','pausing')"
    ))?;
    let rows = st.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// User retry of a failed or cancelled job: back to the queue with a fresh
/// attempt budget and a new generation; the attempt history stays.
/// Children cancelled because of this job come back too (AC-58).
pub fn retry(c: &mut Connection, id: &str, now: i64) -> Result<JobStatus> {
    let tx = c.transaction()?;
    let job = load(&tx, id)?;
    if !matches!(job.status, JobStatus::Failed | JobStatus::Canceled) {
        return Err(JobError::InvalidTransition {
            from: job.status,
            action: "retry",
        });
    }
    let parent_done = match &job.parent_job_id {
        None => true,
        Some(p) => load(&tx, p)?.status == JobStatus::Succeeded,
    };
    let (status, blocked) = if parent_done {
        (JobStatus::Queued, None)
    } else {
        (JobStatus::Blocked, Some(WAITING_FOR_PARENT))
    };
    let clash: Option<String> = tx
        .query_row(
            "SELECT id FROM job WHERE idempotency_key = ?1 AND id <> ?2 AND status NOT IN ('succeeded','failed','canceled')",
            params![job.idempotency_key, id],
            |r| r.get(0),
        )
        .optional()?;
    if clash.is_some() {
        return Err(JobError::InvalidTransition {
            from: job.status,
            action: "retry (an open job does the same work)",
        });
    }
    tx.execute(
        "UPDATE job SET status = ?2, blocked_reason = ?3, attempts = 0, generation = generation + 1, cancel_requested = 0, cancel_reason = NULL,
                next_retry_at = NULL, error_code = NULL, redacted_error_message = NULL, finished_at = NULL, updated_at = ?4
          WHERE id = ?1",
        params![id, status.as_str(), blocked, now],
    )?;
    revive_dependents(&tx, id, now)?;
    tx.commit()?;
    Ok(status)
}

/// The item was removed (§7.3): its open jobs are cancelled with
/// `item_removed`. They do not come back if the item is restored.
pub fn cancel_for_item(c: &mut Connection, item_id: &str, now: i64) -> Result<usize> {
    let ids: Vec<String> = {
        let mut st = c.prepare("SELECT id FROM job WHERE item_id = ?1 AND status NOT IN ('succeeded','failed','canceled','canceling')")?;
        let rows = st.query_map([item_id], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for id in &ids {
        cancel(c, id, CancelReason::ItemRemoved, now)?;
    }
    Ok(ids.len())
}

/// [`cancel_for_item`] inside a caller's transaction (removal runs in one):
/// waiting jobs end `canceled`, running ones are asked to stop. Reason
/// `item_removed`; they do not come back on restore (§7.3).
pub fn cancel_open_for_item(c: &Connection, item_id: &str, now: i64) -> Result<usize> {
    let a = c.execute(
        "UPDATE job SET status = 'canceling', cancel_requested = 1, cancel_reason = 'item_removed', updated_at = ?2 WHERE item_id = ?1 AND status IN ('running','pausing')",
        params![item_id, now],
    )?;
    let b = c.execute(
        "UPDATE job SET status = 'canceled', cancel_reason = 'item_removed', blocked_reason = NULL, next_retry_at = NULL, finished_at = ?2, updated_at = ?2 WHERE item_id = ?1 AND status IN ('queued','blocked','retry_wait','paused')",
        params![item_id, now],
    )?;
    Ok(a + b)
}

/// What [`recover`] / [`reclaim_expired`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recovery {
    /// `running` jobs put back in the queue with a new generation.
    pub requeued: Vec<String>,
    /// `pausing` jobs now `paused`.
    pub paused: Vec<String>,
    /// `canceling` jobs now `canceled`.
    pub canceled: Vec<String>,
}

/// Startup reconciliation (§13.2), before any worker starts: no worker of
/// an earlier run is alive, so every held job is released. The interrupted
/// attempt is not counted against the retry budget.
pub fn recover(c: &mut Connection, now: i64) -> Result<Recovery> {
    release(c, None, now)
}

/// Releases jobs whose lease ran out (a worker hung or died) while the app
/// keeps running. The worker's later calls fail with [`JobError::Stale`].
pub fn reclaim_expired(c: &mut Connection, now: i64) -> Result<Recovery> {
    release(c, Some(now), now)
}

fn release(c: &mut Connection, expired_before: Option<i64>, now: i64) -> Result<Recovery> {
    let tx = c.transaction()?;
    let held: Vec<(String, String, i64)> = {
        let mut st = tx.prepare("SELECT id, status, generation FROM job WHERE status IN ('running','pausing','canceling') AND (?1 IS NULL OR lease_expires_at < ?1)")?;
        let rows = st.query_map([expired_before], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut out = Recovery::default();
    for (id, status, generation) in held {
        match JobStatus::parse(&status) {
            Some(JobStatus::Canceling) => {
                finish_cancel(&tx, &id, generation, now)?;
                out.canceled.push(id);
            }
            Some(JobStatus::Pausing) => {
                tx.execute(
                    "UPDATE job SET status = 'paused', attempts = max(attempts - 1, 0), generation = generation + 1, lease_owner = NULL, lease_expires_at = NULL, updated_at = ?2 WHERE id = ?1",
                    params![id, now],
                )?;
                end_attempt(&tx, &id, generation, "interrupted", None, now)?;
                out.paused.push(id);
            }
            _ => {
                tx.execute(
                    "UPDATE job SET status = 'queued', attempts = max(attempts - 1, 0), generation = generation + 1, lease_owner = NULL, lease_expires_at = NULL, updated_at = ?2 WHERE id = ?1",
                    params![id, now],
                )?;
                end_attempt(&tx, &id, generation, "interrupted", None, now)?;
                out.requeued.push(id);
            }
        }
    }
    tx.commit()?;
    Ok(out)
}
