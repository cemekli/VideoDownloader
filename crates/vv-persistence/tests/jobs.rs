//! STATE.md F3.1: the persistent job queue (SPEC §12.3, §13; ADR-0019).
//! Workers and process handling come in later F3 steps; these tests drive
//! the queue the way a worker would.

use rusqlite::Connection;
use std::cell::Cell;
use std::path::Path;
use vv_persistence::jobs::{
    self, CancelReason, Failure, JobError, JobKind, JobStatus, NewJob, Signal,
};
use vv_persistence::library::repo::{self, LibraryItem};
use vv_persistence::library::*;
use vv_persistence::{Database, OpenOptions};

const W: &str = "worker-1";
const ALL: &[JobKind] = JobKind::ALL;

fn open(dir: &Path) -> Database {
    Database::open(&dir.join("library.sqlite"), &OpenOptions::default()).unwrap()
}

fn add(c: &mut Connection, kind: JobKind, key: &str, now: i64) -> String {
    jobs::enqueue(c, &NewJob::new(kind, key), now).unwrap().id
}

fn child(c: &mut Connection, kind: JobKind, key: &str, parent: &str, now: i64) -> String {
    let mut n = NewJob::new(kind, key);
    n.parent_job_id = Some(parent.into());
    jobs::enqueue(c, &n, now).unwrap().id
}

fn status(c: &Connection, id: &str) -> JobStatus {
    jobs::get(c, id).unwrap().status
}

fn item(c: &Connection) -> LibraryItem {
    let mut it = LibraryItem::new(ItemKind::Bookmark, SourcePlatform::Generic, 1);
    it.canonical_url = Some("https://example.com/v/1".into());
    repo::insert_item(c, &it).unwrap();
    it
}

#[test]
fn the_same_work_is_queued_once_while_open() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let a = jobs::enqueue(c, &NewJob::new(JobKind::Download, "dl:x"), 1).unwrap();
    let b = jobs::enqueue(c, &NewJob::new(JobKind::Download, "dl:x"), 2).unwrap();
    assert!(!a.existing);
    assert!(b.existing);
    assert_eq!(a.id, b.id);
    assert_eq!(jobs::list(c, &[], 10).unwrap().len(), 1);

    let l = jobs::claim(c, W, ALL, 4, false, 3).unwrap().unwrap();
    jobs::succeed(c, &l.job.id, l.generation, 4).unwrap();
    // Finished work may be asked for again.
    let again = jobs::enqueue(c, &NewJob::new(JobKind::Download, "dl:x"), 5).unwrap();
    assert!(!again.existing);
    assert_ne!(again.id, a.id);
}

#[test]
fn bad_payloads_and_states_are_refused() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let mut n = NewJob::new(JobKind::Metadata, "m");
    n.payload = serde_json::json!([1]);
    assert!(matches!(jobs::enqueue(c, &n, 1), Err(JobError::Invalid(_))));
    // The schema itself keeps states consistent.
    let bad = c.execute(
        "INSERT INTO job (id, type, status, payload, idempotency_key, created_at, updated_at) VALUES ('j', 'metadata', 'blocked', '{}', 'k', 1, 1)",
        [],
    );
    assert!(bad.is_err(), "blocked without a reason");
    let bad = c.execute(
        "INSERT INTO job (id, type, status, payload, idempotency_key, created_at, updated_at) VALUES ('j', 'metadata', 'running', '{}', 'k', 1, 1)",
        [],
    );
    assert!(bad.is_err(), "running without a lease");
    let id = add(c, JobKind::Metadata, "m2", 1);
    let dup = c.execute(
        "INSERT INTO job (id, type, status, payload, idempotency_key, created_at, updated_at) VALUES ('j2', 'metadata', 'queued', '{}', 'm2', 1, 1)",
        [],
    );
    assert!(dup.is_err(), "two open jobs with one key");
    assert!(matches!(
        jobs::resume(c, &id, 2),
        Err(JobError::InvalidTransition { .. })
    ));
}

#[test]
fn claim_takes_priority_then_age_and_respects_the_cap() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let old = add(c, JobKind::Download, "a", 1);
    let mut n = NewJob::new(JobKind::Download, "b");
    n.priority = 5;
    let urgent = jobs::enqueue(c, &n, 2).unwrap().id;
    let newer = add(c, JobKind::Download, "c", 3);
    let thumb = add(c, JobKind::Thumbnail, "t", 0);

    let first = jobs::claim(c, W, &[JobKind::Download], 2, false, 10)
        .unwrap()
        .unwrap();
    assert_eq!(first.job.id, urgent);
    assert_eq!(first.job.attempts, 1);
    assert_eq!(first.generation, 1);
    assert_eq!(first.job.lease_expires_at, Some(10 + jobs::LEASE_MS));
    let second = jobs::claim(c, W, &[JobKind::Download], 2, false, 10)
        .unwrap()
        .unwrap();
    assert_eq!(second.job.id, old);
    // Two downloads run: the cap holds the third back, other kinds still run.
    assert!(jobs::claim(c, W, &[JobKind::Download], 2, false, 10)
        .unwrap()
        .is_none());
    assert_eq!(
        jobs::claim(c, W, &[JobKind::Thumbnail], 1, false, 10)
            .unwrap()
            .unwrap()
            .job
            .id,
        thumb
    );
    assert_eq!(status(c, &newer), JobStatus::Queued);
}

#[test]
fn transient_errors_back_off_and_fail_after_four_attempts() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let id = add(c, JobKind::Download, "dl", 0);
    let mut now = 100;
    for attempt in 1..=3 {
        let l = jobs::claim(c, W, ALL, 1, false, now).unwrap().unwrap();
        assert_eq!(l.job.attempts, attempt);
        let st = jobs::fail(
            c,
            &id,
            l.generation,
            &Failure::Transient {
                retry_after_ms: None,
            },
            "network",
            Some("connection reset"),
            now,
        )
        .unwrap();
        assert_eq!(st, JobStatus::RetryWait);
        let j = jobs::get(c, &id).unwrap();
        let wait = j.next_retry_at.unwrap() - now;
        let base = jobs::BACKOFF_MS[attempt as usize - 1];
        assert!(
            wait >= base && wait <= base + base / 5,
            "attempt {attempt}: {wait}"
        );
        assert_eq!(
            wait,
            jobs::backoff_ms(&id, attempt, None),
            "jitter is repeatable"
        );
        assert_eq!(j.error_code.as_deref(), Some("network"));
        assert!(
            jobs::claim(c, W, ALL, 1, false, now + wait - 1)
                .unwrap()
                .is_none(),
            "not before its time"
        );
        now += wait;
    }
    let l = jobs::claim(c, W, ALL, 1, false, now).unwrap().unwrap();
    assert_eq!(l.job.attempts, 4);
    let st = jobs::fail(
        c,
        &id,
        l.generation,
        &Failure::Transient {
            retry_after_ms: None,
        },
        "network",
        None,
        now,
    )
    .unwrap();
    assert_eq!(st, JobStatus::Failed);
    let outcomes: Vec<_> = jobs::attempts(c, &id)
        .unwrap()
        .into_iter()
        .map(|a| a.2.unwrap())
        .collect();
    assert_eq!(outcomes, ["retry", "retry", "retry", "failed"]);
}

#[test]
fn retry_after_and_permanent_errors() {
    assert_eq!(jobs::backoff_ms("x", 1, Some(600_000)), 600_000);
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let id = add(c, JobKind::Download, "drm", 0);
    let l = jobs::claim(c, W, ALL, 1, false, 1).unwrap().unwrap();
    let st = jobs::fail(c, &id, l.generation, &Failure::Permanent, "drm", None, 2).unwrap();
    assert_eq!(st, JobStatus::Failed);
    assert_eq!(jobs::get(c, &id).unwrap().finished_at, Some(2));
}

#[test]
fn a_reclaimed_worker_cannot_publish() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let id = add(c, JobKind::Download, "dl", 0);
    let stale = jobs::claim(c, W, ALL, 1, false, 0).unwrap().unwrap();
    assert_eq!(
        jobs::heartbeat(c, &id, stale.generation, 5_000).unwrap(),
        Signal::Continue
    );
    // Heartbeat moved the lease; nothing has expired yet.
    assert!(jobs::reclaim_expired(c, 5_000 + jobs::LEASE_MS - 1)
        .unwrap()
        .requeued
        .is_empty());
    let r = jobs::reclaim_expired(c, 5_000 + jobs::LEASE_MS + 1).unwrap();
    assert_eq!(r.requeued, vec![id.clone()]);

    let fresh = jobs::claim(c, "worker-2", ALL, 1, false, 40_000)
        .unwrap()
        .unwrap();
    assert_eq!(fresh.generation, stale.generation + 2);
    assert_eq!(fresh.job.attempts, 1, "the hung attempt is not counted");
    let published = Cell::new(0);
    let late = jobs::succeed_with(c, &id, stale.generation, 41_000, |_| {
        published.set(published.get() + 1);
        Ok(())
    });
    assert!(matches!(late, Err(JobError::Stale)));
    assert!(matches!(
        jobs::heartbeat(c, &id, stale.generation, 41_000),
        Err(JobError::Stale)
    ));
    jobs::succeed_with(c, &id, fresh.generation, 42_000, |_| {
        published.set(published.get() + 1);
        Ok(())
    })
    .unwrap();
    assert_eq!(published.get(), 1);
    assert_eq!(status(c, &id), JobStatus::Succeeded);
}

#[test]
fn ac10_cancel_ends_waiting_and_running_jobs_canceled() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let waiting = add(c, JobKind::Download, "w", 0);
    let running = add(c, JobKind::Download, "r", 1);
    jobs::cancel(c, &waiting, CancelReason::User, 2).unwrap();
    let l = jobs::claim(c, W, ALL, 1, false, 3).unwrap().unwrap();
    assert_eq!(l.job.id, running);
    assert_eq!(status(c, &waiting), JobStatus::Canceled);

    assert_eq!(
        jobs::cancel(c, &running, CancelReason::User, 4).unwrap(),
        JobStatus::Canceling
    );
    // The worker learns of it at its next heartbeat and must not finish.
    assert_eq!(
        jobs::heartbeat(c, &running, l.generation, 5).unwrap(),
        Signal::Cancel
    );
    assert!(matches!(
        jobs::succeed(c, &running, l.generation, 6),
        Err(JobError::InvalidTransition { .. })
    ));
    jobs::confirm_canceled(c, &running, l.generation, 7).unwrap();
    let j = jobs::get(c, &running).unwrap();
    assert_eq!(j.status, JobStatus::Canceled);
    assert_eq!(j.cancel_reason, Some(CancelReason::User));
    assert!(j.lease_owner.is_none());
    assert_eq!(
        jobs::attempts(c, &running).unwrap()[0].2.as_deref(),
        Some("canceled")
    );
    assert!(matches!(
        jobs::cancel(c, &running, CancelReason::User, 8),
        Err(JobError::InvalidTransition { .. })
    ));
    assert!(
        !jobs::enqueue(c, &NewJob::new(JobKind::Download, "r"), 9)
            .unwrap()
            .existing,
        "a cancelled job does not hold its key"
    );
}

#[test]
fn ac11_a_job_interrupted_by_a_crash_runs_again_and_finishes_once() {
    let d = tempfile::tempdir().unwrap();
    let id;
    let first_gen;
    {
        let mut db = open(d.path());
        let c = db.writer();
        id = add(c, JobKind::Download, "dl", 0);
        let other = add(c, JobKind::Thumbnail, "th", 0);
        let l = jobs::claim(c, W, &[JobKind::Download], 1, false, 1)
            .unwrap()
            .unwrap();
        first_gen = l.generation;
        jobs::set_checkpoint(c, &id, l.generation, Some("part-1"), 2).unwrap();
        jobs::claim(c, W, &[JobKind::Thumbnail], 1, false, 1)
            .unwrap()
            .unwrap();
        jobs::cancel(c, &other, CancelReason::Shutdown, 3).unwrap();
        // The app dies here: no worker ever reports back.
    }
    let mut db = open(d.path());
    let c = db.writer();
    let r = jobs::recover(c, 10).unwrap();
    assert_eq!(r.requeued, vec![id.clone()]);
    assert_eq!(r.canceled.len(), 1, "a cancel in flight is finished");
    let j = jobs::get(c, &id).unwrap();
    assert_eq!(j.status, JobStatus::Queued);
    assert_eq!(j.attempts, 0, "the crash does not use up an attempt");
    assert_eq!(
        j.checkpoint_ref.as_deref(),
        Some("part-1"),
        "a resumable download keeps its checkpoint"
    );

    let l = jobs::claim(c, W, ALL, 1, false, 11).unwrap().unwrap();
    assert!(l.generation > first_gen);
    let finals = Cell::new(0);
    for g in [first_gen, l.generation] {
        let _ = jobs::succeed_with(c, &id, g, 12, |tx| {
            tx.execute(
                "CREATE TABLE IF NOT EXISTS final_ref (job TEXT PRIMARY KEY)",
                [],
            )?;
            tx.execute("INSERT INTO final_ref VALUES (?1)", [&id])?;
            finals.set(finals.get() + 1);
            Ok(())
        });
    }
    assert_eq!(finals.get(), 1);
    let n: i64 = c
        .query_row("SELECT count(*) FROM final_ref", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1, "one final media reference");
    let outcomes: Vec<_> = jobs::attempts(c, &id)
        .unwrap()
        .into_iter()
        .map(|a| a.2.unwrap())
        .collect();
    assert_eq!(outcomes, ["interrupted", "succeeded"]);
}

#[test]
fn ac58_dependency_cancels_come_back_on_retry_user_cancels_stay() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let parent = add(c, JobKind::Download, "dl", 0);
    let mine = child(c, JobKind::Thumbnail, "th", &parent, 1);
    let dep = child(c, JobKind::ProbeMedia, "probe", &parent, 1);
    let grandchild = child(c, JobKind::HashMedia, "hash", &dep, 1);
    for k in [&mine, &dep, &grandchild] {
        let j = jobs::get(c, k).unwrap();
        assert_eq!(
            (j.status, j.blocked_reason.as_deref()),
            (JobStatus::Blocked, Some(jobs::WAITING_FOR_PARENT))
        );
    }
    assert!(
        matches!(
            jobs::unblock(c, &dep, 2),
            Err(JobError::InvalidTransition { .. })
        ),
        "only the parent releases it"
    );
    jobs::cancel(c, &mine, CancelReason::User, 2).unwrap();

    let l = jobs::claim(c, W, ALL, 4, false, 3).unwrap().unwrap();
    assert_eq!(l.job.id, parent, "children wait");
    jobs::fail(
        c,
        &parent,
        l.generation,
        &Failure::Permanent,
        "unavailable",
        None,
        4,
    )
    .unwrap();
    for k in [&dep, &grandchild] {
        let j = jobs::get(c, k).unwrap();
        assert_eq!(
            (j.status, j.cancel_reason),
            (JobStatus::Canceled, Some(CancelReason::DependencyFailed))
        );
    }
    // A child added after the parent failed starts cancelled.
    let late = child(c, JobKind::FetchCaptions, "cap", &parent, 5);
    assert_eq!(
        jobs::get(c, &late).unwrap().cancel_reason,
        Some(CancelReason::DependencyFailed)
    );

    assert_eq!(jobs::retry(c, &parent, 6).unwrap(), JobStatus::Queued);
    assert_eq!(status(c, &dep), JobStatus::Blocked);
    assert_eq!(status(c, &grandchild), JobStatus::Blocked);
    assert_eq!(status(c, &late), JobStatus::Blocked);
    let m = jobs::get(c, &mine).unwrap();
    assert_eq!(
        (m.status, m.cancel_reason),
        (JobStatus::Canceled, Some(CancelReason::User))
    );

    let l = jobs::claim(c, W, ALL, 4, false, 7).unwrap().unwrap();
    assert_eq!(l.job.attempts, 1, "a user retry gets a fresh budget");
    jobs::succeed(c, &parent, l.generation, 8).unwrap();
    assert_eq!(status(c, &dep), JobStatus::Queued);
    assert_eq!(
        status(c, &grandchild),
        JobStatus::Blocked,
        "still waits for its own parent"
    );
    assert_eq!(status(c, &mine), JobStatus::Canceled);
}

#[test]
fn a_running_child_is_stopped_when_its_parent_is_cancelled() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let parent = add(c, JobKind::Metadata, "meta", 0);
    let l = jobs::claim(c, W, ALL, 4, false, 1).unwrap().unwrap();
    jobs::succeed(c, &parent, l.generation, 2).unwrap();
    let kid = child(c, JobKind::Download, "dl", &parent, 3);
    let grandkid = child(c, JobKind::Thumbnail, "th", &kid, 3);
    assert_eq!(
        status(c, &kid),
        JobStatus::Queued,
        "the parent already succeeded"
    );
    let k = jobs::claim(c, W, ALL, 4, false, 4).unwrap().unwrap();
    assert_eq!(k.job.id, kid);
    jobs::cancel(c, &kid, CancelReason::User, 5).unwrap();
    assert_eq!(
        status(c, &grandkid),
        JobStatus::Blocked,
        "not until the worker stops"
    );
    jobs::confirm_canceled(c, &kid, k.generation, 6).unwrap();
    assert_eq!(
        jobs::get(c, &grandkid).unwrap().cancel_reason,
        Some(CancelReason::DependencyFailed)
    );
}

#[test]
fn offline_mode_blocks_network_jobs_without_spending_attempts() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let running = add(c, JobKind::Download, "dl1", 0);
    let waiting = add(c, JobKind::Metadata, "meta", 1);
    let local = add(c, JobKind::Thumbnail, "th", 2);
    let l = jobs::claim(c, W, &[JobKind::Download], 1, false, 3)
        .unwrap()
        .unwrap();

    assert_eq!(jobs::set_offline(c, true, 4).unwrap(), 1);
    assert_eq!(
        jobs::get(c, &waiting).unwrap().blocked_reason.as_deref(),
        Some(jobs::OFFLINE)
    );
    assert_eq!(status(c, &local), JobStatus::Queued);
    let stop: Vec<_> = jobs::running_network_jobs(c)
        .unwrap()
        .into_iter()
        .map(|j| j.id)
        .collect();
    assert_eq!(stop, vec![running.clone()]);
    jobs::block(c, &running, Some(l.generation), jobs::OFFLINE, 5).unwrap();
    assert_eq!(jobs::get(c, &running).unwrap().attempts, 0);
    // Even a stray network job is never handed out while offline.
    add(c, JobKind::ModelDownload, "model", 6);
    assert_eq!(
        jobs::claim(c, W, ALL, 4, true, 7).unwrap().unwrap().job.id,
        local
    );
    assert!(jobs::claim(c, W, ALL, 4, true, 7).unwrap().is_none());

    assert_eq!(jobs::set_offline(c, false, 8).unwrap(), 2);
    assert_eq!(status(c, &running), JobStatus::Queued);
    assert_eq!(status(c, &waiting), JobStatus::Queued);
}

#[test]
fn blocked_for_the_user_and_unblocked() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let id = add(c, JobKind::Download, "dl", 0);
    let l = jobs::claim(c, W, ALL, 1, false, 1).unwrap().unwrap();
    assert!(matches!(
        jobs::block(c, &id, Some(l.generation), jobs::WAITING_FOR_PARENT, 2),
        Err(JobError::Invalid(_))
    ));
    assert!(matches!(
        jobs::block(c, &id, Some(l.generation + 1), "disk_full", 2),
        Err(JobError::Stale)
    ));
    jobs::block(c, &id, Some(l.generation), "disk_full", 2).unwrap();
    let j = jobs::get(c, &id).unwrap();
    assert_eq!((j.status, j.attempts), (JobStatus::Blocked, 0));
    assert!(jobs::claim(c, W, ALL, 1, false, 3).unwrap().is_none());
    jobs::unblock(c, &id, 4).unwrap();
    assert_eq!(
        jobs::claim(c, W, ALL, 1, false, 5)
            .unwrap()
            .unwrap()
            .job
            .attempts,
        1
    );
}

#[test]
fn pause_and_resume_keep_the_attempt_budget() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let id = add(c, JobKind::Download, "dl", 0);
    let thumb = add(c, JobKind::Thumbnail, "th", 0);
    assert!(
        matches!(
            jobs::pause(c, &thumb, 1),
            Err(JobError::InvalidTransition { .. })
        ),
        "only resumable kinds pause"
    );

    let l = jobs::claim(c, W, &[JobKind::Download], 1, false, 1)
        .unwrap()
        .unwrap();
    let p = jobs::Progress {
        phase: Some("downloading".into()),
        bytes_done: Some(10),
        bytes_total: Some(100),
        ..Default::default()
    };
    assert_eq!(
        jobs::report_progress(c, &id, l.generation, &p, 2).unwrap(),
        Signal::Continue
    );
    assert_eq!(jobs::pause(c, &id, 3).unwrap(), JobStatus::Pausing);
    assert_eq!(
        jobs::report_progress(c, &id, l.generation, &p, 4).unwrap(),
        Signal::Pause
    );
    jobs::confirm_paused(c, &id, l.generation, 5).unwrap();
    let j = jobs::get(c, &id).unwrap();
    assert_eq!((j.status, j.attempts), (JobStatus::Paused, 0));
    assert_eq!(j.progress.bytes_done, Some(10));
    assert!(jobs::claim(c, W, &[JobKind::Download], 1, false, 6)
        .unwrap()
        .is_none());
    jobs::resume(c, &id, 7).unwrap();
    let l = jobs::claim(c, W, &[JobKind::Download], 1, false, 8)
        .unwrap()
        .unwrap();
    assert_eq!(l.job.attempts, 1);

    // A pause that was in flight when the app stopped ends paused.
    jobs::pause(c, &id, 9).unwrap();
    let r = jobs::recover(c, 10).unwrap();
    assert_eq!(r.paused, vec![id.clone()]);
    assert_eq!(status(c, &id), JobStatus::Paused);
}

#[test]
fn removing_an_item_cancels_its_jobs() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let it = item(c);
    let mut ids = vec![];
    for (kind, key) in [(JobKind::Download, "dl"), (JobKind::Thumbnail, "th")] {
        let mut n = NewJob::new(kind, key);
        n.item_id = Some(it.id.clone());
        ids.push(jobs::enqueue(c, &n, 0).unwrap().id);
    }
    let unrelated = add(c, JobKind::Reindex, "reindex", 0);
    let l = jobs::claim(c, W, &[JobKind::Download], 1, false, 1)
        .unwrap()
        .unwrap();
    assert_eq!(jobs::cancel_for_item(c, &it.id, 2).unwrap(), 2);
    assert_eq!(status(c, &ids[0]), JobStatus::Canceling);
    assert_eq!(status(c, &ids[1]), JobStatus::Canceled);
    jobs::confirm_canceled(c, &ids[0], l.generation, 3).unwrap();
    for id in &ids {
        assert_eq!(
            jobs::get(c, id).unwrap().cancel_reason,
            Some(CancelReason::ItemRemoved)
        );
    }
    assert!(matches!(
        jobs::retry(c, &unrelated, 4),
        Err(JobError::InvalidTransition { .. })
    ));
    assert_eq!(status(c, &unrelated), JobStatus::Queued);
    // Purging the item drops its job history with it.
    c.execute("DELETE FROM library_item WHERE id = ?1", [&it.id])
        .unwrap();
    assert!(matches!(jobs::get(c, &ids[0]), Err(JobError::NotFound)));
}

#[test]
fn a_retry_is_refused_while_another_job_does_the_same_work() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let a = add(c, JobKind::Metadata, "meta", 0);
    jobs::cancel(c, &a, CancelReason::User, 1).unwrap();
    let b = add(c, JobKind::Metadata, "meta", 2);
    assert_ne!(a, b);
    assert!(matches!(
        jobs::retry(c, &a, 3),
        Err(JobError::InvalidTransition { .. })
    ));
    jobs::cancel(c, &b, CancelReason::User, 4).unwrap();
    assert_eq!(jobs::retry(c, &a, 5).unwrap(), JobStatus::Queued);
}

#[test]
fn every_kind_and_state_round_trips() {
    for k in JobKind::ALL {
        assert_eq!(JobKind::parse(k.as_str()), Some(*k));
    }
    for s in JobStatus::ALL {
        assert_eq!(JobStatus::parse(s.as_str()), Some(*s));
    }
    assert_eq!(JobKind::ALL.len(), 16);
    assert_eq!(JobStatus::ALL.len(), 10);
}
