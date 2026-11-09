//! A claimed job as its handler sees it: throttled progress and heartbeat,
//! why it must stop, and the one way to end it (SPEC §13.2–§13.4,
//! ADR-0019, ADR-0020).

use crate::context::{Context, Store};
use std::cell::Cell;
use std::time::{Duration, Instant};
use vv_domain::error::ErrorCode;
use vv_persistence::classify;
use vv_persistence::jobs::{self, JobError, JobStatus, Lease, Progress, Signal};

/// §13.3: at most about four progress events per second per job.
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);
/// How often a running handler renews its lease and asks for signals.
pub const POLL_INTERVAL: Duration = Duration::from_millis(1000);

/// Why a handler stopped its tool before the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    Cancel,
    Pause,
    Offline,
    /// The lease is gone (reclaimed or retried elsewhere): publish nothing.
    Lost,
}

pub struct Work<'a, S: Store> {
    pub cx: &'a Context<S>,
    pub lease: Lease,
    last_progress: Cell<Option<Instant>>,
    last_poll: Cell<Instant>,
    stop: Cell<Option<Stop>>,
}

impl<'a, S: Store> Work<'a, S> {
    pub fn new(cx: &'a Context<S>, lease: Lease) -> Self {
        Work {
            cx,
            lease,
            last_progress: Cell::new(None),
            last_poll: Cell::new(Instant::now()),
            stop: Cell::new(None),
        }
    }

    pub fn id(&self) -> &str {
        &self.lease.job.id
    }

    pub fn generation(&self) -> i64 {
        self.lease.generation
    }

    pub fn item_id(&self) -> Option<&str> {
        self.lease.job.item_id.as_deref()
    }

    fn notify(&self) {
        self.cx.events.job_changed(self.id(), self.item_id());
    }

    fn signal(&self, s: Result<Signal, JobError>) {
        match s {
            Ok(Signal::Continue) => {}
            Ok(Signal::Cancel) => self.stop.set(Some(Stop::Cancel)),
            Ok(Signal::Pause) => self.stop.set(Some(Stop::Pause)),
            Err(_) => self.stop.set(Some(Stop::Lost)),
        }
    }

    /// Records progress, at most every [`PROGRESS_INTERVAL`] unless `force`
    /// (phase changes and the last update go through at once).
    pub fn progress(&self, p: &Progress, force: bool) {
        let due = self
            .last_progress
            .get()
            .is_none_or(|t| t.elapsed() >= PROGRESS_INTERVAL);
        if !(due || force) {
            return;
        }
        self.last_progress.set(Some(Instant::now()));
        self.last_poll.set(Instant::now());
        let now = self.cx.now();
        let r = self
            .cx
            .store
            .write(|c| jobs::report_progress(c, self.id(), self.generation(), p, now));
        self.signal(r);
        self.notify();
    }

    /// Asked by the tool loop: must the tool stop now? Renews the lease about
    /// once a second and sees cancel, pause and Offline Mode.
    pub fn should_stop(&self) -> bool {
        if self.stop.get().is_some() {
            return true;
        }
        if self.cx.is_offline() && self.lease.job.kind.uses_network() {
            self.stop.set(Some(Stop::Offline));
            return true;
        }
        if self.last_poll.get().elapsed() >= POLL_INTERVAL {
            self.last_poll.set(Instant::now());
            let now = self.cx.now();
            let r = self
                .cx
                .store
                .write(|c| jobs::heartbeat(c, self.id(), self.generation(), now));
            self.signal(r);
        }
        self.stop.get().is_some()
    }

    pub fn stopped(&self) -> Option<Stop> {
        self.stop.get()
    }

    /// Ends the attempt after the tool was stopped for `stop`.
    pub fn end_stopped(&self, stop: Stop) -> Option<JobStatus> {
        let now = self.cx.now();
        let (id, g) = (self.id().to_owned(), self.generation());
        let r = self.cx.store.write(|c| match stop {
            Stop::Cancel => jobs::confirm_canceled(c, &id, g, now).map(|_| JobStatus::Canceled),
            Stop::Pause => jobs::confirm_paused(c, &id, g, now).map(|_| JobStatus::Paused),
            Stop::Offline => {
                jobs::block(c, &id, Some(g), jobs::OFFLINE, now).map(|_| JobStatus::Blocked)
            }
            Stop::Lost => Err(JobError::Stale),
        });
        self.notify();
        r.ok()
    }

    /// Ends the attempt with `code` as SPEC §19 / ADR-0020 say (retry, block,
    /// fail, cancel). `message` must already be redacted.
    pub fn end_error(&self, code: ErrorCode, message: Option<&str>) -> Option<JobStatus> {
        if let Some(stop) = self.stop.get() {
            // A cancel or pause that came in while the tool failed wins.
            if matches!(stop, Stop::Cancel | Stop::Pause | Stop::Offline) {
                return self.end_stopped(stop);
            }
            return None;
        }
        let now = self.cx.now();
        let r = self.cx.store.write(|c| {
            classify::apply_to_job(c, self.id(), self.generation(), code, message, None, now)
        });
        self.notify();
        r.ok()
    }
}
