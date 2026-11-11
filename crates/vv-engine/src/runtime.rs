//! The worker runtime (STATE.md F3; SPEC §13.2, §13.3; ADR-0019,
//! ADR-0021).
//!
//! One scheduler thread. At start it reconciles (`jobs::recover`, storage
//! move clean-ups, staging folders of finished jobs), then about twice a
//! second, or at once when woken:
//!
//! - releases expired leases (`reclaim_expired`);
//! - claims ready jobs per pool and runs each on its own thread. Pool
//!   limits are §13.3's starting values: 3 metadata, 2 downloads, 1 storage
//!   move. `claim` counts running jobs in the database, so the limit holds
//!   across restarts.
//!
//! Offline Mode is mirrored into the context: running network jobs stop
//! and block themselves, waiting ones are blocked by `jobs::set_offline`.

use crate::context::{Context, Store};
use crate::{captions, download, metadata, models, storage_move};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use vv_persistence::jobs::{self, JobKind, JobStatus};

pub const TICK: Duration = Duration::from_millis(500);

/// (kinds, max running) per pool (§13.3).
pub const POOLS: &[(&[JobKind], usize)] = &[
    (&[JobKind::Metadata], 3),
    (&[JobKind::Download], 2),
    (&[JobKind::StorageMove], 1),
    // One model at a time: they are hundreds of MB to GB (F6.1).
    (&[JobKind::ModelDownload], 1),
    // Small text files; two runs of yt-dlp each (F6.2).
    (&[JobKind::FetchCaptions], 2),
];

pub struct Engine<S: Store> {
    cx: Arc<Context<S>>,
    wake: Arc<(Mutex<bool>, Condvar)>,
    stop: Arc<AtomicBool>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl<S: Store> Engine<S> {
    pub fn new(cx: Context<S>) -> Self {
        Engine {
            cx: Arc::new(cx),
            wake: Arc::new((Mutex::new(false), Condvar::new())),
            stop: Arc::new(AtomicBool::new(false)),
            thread: Mutex::new(None),
        }
    }

    pub fn context(&self) -> &Context<S> {
        &self.cx
    }

    /// Start-up reconciliation (§13.2). Runs before any worker.
    pub fn reconcile(&self) {
        let cx = &self.cx;
        let now = cx.now();
        let offline = cx
            .store
            .read(|c| vv_persistence::settings::offline_mode(c).unwrap_or(false));
        cx.offline.store(offline, Ordering::SeqCst);
        cx.store.write(|c| {
            let _ = jobs::recover(c, now);
            let _ = jobs::set_offline(c, offline, now);
            let _ = storage_move::retry_cleanups(c, now);
            let _ = metadata::enqueue_pending(c, now, 500);
            models::reconcile(c, &cx.dirs.models, now);
        });
        self.unblock_tools();
        // Cookie-file working copies left by a crash (§21.1), then secrets
        // no profile points at (ADR-0022, ADR-0024).
        crate::auth::clean_working_copies(&cx.dirs);
        cx.store.write(|c| {
            let _ = vv_persistence::auth::reconcile_secrets(c, cx.secrets.as_ref());
        });
        clean_staging(cx);
    }

    /// Jobs blocked for missing tools go back to the queue once the tools
    /// passed their check.
    pub fn unblock_tools(&self) {
        let cx = &self.cx;
        if cx.tools().report.ready_for_downloads().is_err() {
            return;
        }
        let now = cx.now();
        cx.store.write(|c| {
            let ids: Vec<String> = c
                .prepare("SELECT id FROM job WHERE status = 'blocked' AND blocked_reason = 'tool'")
                .and_then(|mut st| {
                    st.query_map([], |r| r.get(0))
                        .map(|r| r.flatten().collect())
                })
                .unwrap_or_default();
            for id in ids {
                let _ = jobs::unblock(c, &id, now);
            }
        });
    }

    /// Starts the scheduler thread.
    pub fn start(&self) {
        let cx = Arc::clone(&self.cx);
        let wake = Arc::clone(&self.wake);
        let stop = Arc::clone(&self.stop);
        let h = std::thread::Builder::new()
            .name("vv-scheduler".into())
            .spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    tick(&cx);
                    let (m, cv) = &*wake;
                    let g = m.lock().unwrap_or_else(|e| e.into_inner());
                    let (mut g, _) = cv
                        .wait_timeout_while(g, TICK, |woken| !*woken)
                        .unwrap_or_else(|e| e.into_inner());
                    *g = false;
                }
            })
            .ok();
        *self.thread.lock().unwrap_or_else(|e| e.into_inner()) = h;
    }

    /// Something was queued or unblocked: schedule now.
    pub fn wake(&self) {
        let (m, cv) = &*self.wake;
        *m.lock().unwrap_or_else(|e| e.into_inner()) = true;
        cv.notify_all();
    }

    /// Offline Mode changed (DATA-05). Waiting network jobs are blocked or
    /// released; running ones see the flag and stop themselves.
    pub fn set_offline(&self, offline: bool) {
        self.cx.offline.store(offline, Ordering::SeqCst);
        let now = self.cx.now();
        self.cx.store.write(|c| {
            let _ = jobs::set_offline(c, offline, now);
        });
        self.wake();
    }

    /// Stops scheduling (running tools are killed when the process ends).
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.wake();
        if let Some(h) = self.thread.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = h.join();
        }
    }

    /// Runs one scheduling pass on the calling thread and waits for the jobs
    /// it started (tests).
    pub fn run_once_blocking(&self) -> usize {
        let handles = claim_all(&self.cx);
        let n = handles.len();
        for h in handles {
            let _ = h.join();
        }
        n
    }
}

/// Runs `handler` for a claimed job. A handler bug (a panic) must end the
/// job with `INTERNAL_ERROR`, not leave it running until its lease runs out
/// and it is retried into the same bug (F6.2: a panic looked like a hang).
pub fn run_guarded<S: Store>(
    cx: &Context<S>,
    lease: jobs::Lease,
    handler: impl FnOnce(&Context<S>, jobs::Lease),
) {
    let (id, g, item) = (
        lease.job.id.clone(),
        lease.generation,
        lease.job.item_id.clone(),
    );
    let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(cx, lease)));
    if ran.is_err() {
        let now = cx.now();
        cx.store.write(|c| {
            let _ = jobs::fail(
                c,
                &id,
                g,
                &jobs::Failure::Permanent,
                "INTERNAL_ERROR",
                Some("the job stopped on an internal error"),
                now,
            );
        });
        cx.events.job_changed(&id, item.as_deref());
    }
}

/// Runs the handler of one claimed job.
pub fn dispatch<S: Store>(cx: &Context<S>, kind: JobKind, lease: jobs::Lease) {
    match kind {
        JobKind::Metadata => metadata::run(cx, lease),
        JobKind::Download => download::run(cx, lease),
        JobKind::StorageMove => storage_move::run(cx, lease),
        JobKind::ModelDownload => models::run(cx, lease),
        JobKind::FetchCaptions => captions::run(cx, lease),
        _ => {}
    }
}

fn tick<S: Store>(cx: &Arc<Context<S>>) {
    let now = cx.now();
    cx.store.write(|c| {
        let _ = jobs::reclaim_expired(c, now);
    });
    // Handles are detached: each job thread ends on its own.
    drop(claim_all(cx));
}

fn claim_all<S: Store>(cx: &Arc<Context<S>>) -> Vec<std::thread::JoinHandle<()>> {
    let mut out = Vec::new();
    for (kinds, max) in POOLS {
        loop {
            let now = cx.now();
            let offline = cx.is_offline();
            let lease = cx
                .store
                .write(|c| jobs::claim(c, &cx.owner, kinds, *max, offline, now));
            let Ok(Some(lease)) = lease else { break };
            cx.events
                .job_changed(&lease.job.id, lease.job.item_id.as_deref());
            let cx2 = Arc::clone(cx);
            let h = std::thread::Builder::new()
                .name(format!("vv-job-{}", lease.job.kind))
                .spawn(move || {
                    let kind = lease.job.kind;
                    run_guarded(&cx2, lease, |cx, lease| dispatch(cx, kind, lease));
                });
            match h {
                Ok(h) => out.push(h),
                Err(_) => break,
            }
        }
    }
    out
}

/// Staging folders of jobs that no longer exist or succeeded are ours and
/// unpublished: they go. Folders of failed or cancelled jobs stay (partial
/// files the user may retry or delete, §13.3).
fn clean_staging<S: Store>(cx: &Context<S>) {
    let Ok(rd) = std::fs::read_dir(&cx.dirs.staging) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let job_id = name.strip_prefix("meta-").map_or(name.as_str(), |rest| {
            // `meta-<job-id>-<generation>`
            rest.rsplit_once('-').map_or(rest, |(id, _)| id)
        });
        let status = cx
            .store
            .read(|c| jobs::get(c, job_id).ok().map(|j| j.status));
        let keep = match status {
            None => false,
            Some(JobStatus::Succeeded) => false,
            Some(_) => !name.starts_with("meta-"),
        };
        if !keep {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}
