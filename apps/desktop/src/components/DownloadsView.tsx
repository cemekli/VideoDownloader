import { useCallback, useEffect, useState } from "react";
import { errorCode } from "../ipc/errors";
import type { DownloadClient, JobDto } from "../ipc/types";
import { formatBytes } from "../format";
import { errorMessage } from "../labels";
import { formatDateTime, t, type MessageKey } from "../i18n";

const ACTIVE = new Set(["queued", "blocked", "running", "retry_wait", "pausing", "paused", "canceling"]);
const PHASES = new Set(["prepare", "download", "download_video", "download_audio", "merge", "postprocess", "verify", "copy"]);
const BLOCKED = new Set(["offline", "disk", "auth", "tool", "path"]);

export const statusLabel = (s: string) => t(`downloads.status.${s}` as MessageKey);
export const presetLabel = (p: string | null) => (p ? t(`downloads.preset.${p}` as MessageKey) : "");
const phaseLabel = (p: string | null) => (p ? t((PHASES.has(p) ? `downloads.phase.${p}` : "downloads.phase.other") as MessageKey) : null);

/** Real progress when the total is known; an indeterminate bar otherwise (AC-09). */
function ProgressBar({ job }: { job: JobDto }) {
  if (job.status !== "running") return null;
  const units = job.kind === "storage_move" && job.unitsTotal;
  const total = units ? job.unitsTotal : job.bytesTotal;
  const done = units ? job.unitsDone : job.bytesDone;
  const known = total !== null && total > 0 && done !== null;
  const pct = known ? Math.min(100, Math.round((done! / total!) * 100)) : null;
  const text = units
    ? t("downloads.move.units", { done: done ?? 0, total: total ?? 0 })
    : known
      ? t("downloads.progress", { done: formatBytes(done), total: formatBytes(total) })
      : t("downloads.progress_unknown", { done: formatBytes(done ?? 0) });
  return (
    <div className="job-progress">
      <div
        className={`bar${known ? "" : " bar-indeterminate"}`}
        role="progressbar"
        aria-label={job.itemTitle ?? t("downloads.move")}
        aria-valuemin={known ? 0 : undefined}
        aria-valuemax={known ? 100 : undefined}
        aria-valuenow={pct ?? undefined}
        aria-valuetext={text}
        data-testid="job-progress"
        data-indeterminate={known ? undefined : "true"}
      >
        <span style={known ? { width: `${pct}%` } : undefined} />
      </div>
      <span className="muted small">{text}</span>
    </div>
  );
}

function JobRow({ job, busy, onAction }: { job: JobDto; busy: boolean; onAction: (a: "retry" | "cancel" | "remove", j: JobDto) => void }) {
  const title = job.kind === "storage_move" ? t("downloads.move") : (job.itemTitle ?? t("downloads.kind.download"));
  const phase = job.status === "running" ? phaseLabel(job.phase) : null;
  return (
    <li className="job" data-testid="job" data-status={job.status} data-job-id={job.id}>
      <div className="job-head">
        <strong className="job-title">{title}</strong>
        <span className={`job-status job-status-${job.status}`} data-testid="job-status">
          {statusLabel(job.status)}
        </span>
      </div>
      <p className="muted small">
        {[presetLabel(job.preset), phase].filter(Boolean).join(" · ")}
      </p>
      <ProgressBar job={job} />
      {job.errorCode && (job.status === "failed" || job.status === "retry_wait") && (
        <p className="job-error small" role={job.status === "failed" ? "alert" : undefined} data-testid="job-error" data-code={job.errorCode}>
          {errorMessage(job.errorCode)}
        </p>
      )}
      {job.status === "blocked" && (
        <p className="small" data-testid="job-blocked">
          {t((BLOCKED.has(job.blockedReason ?? "") ? `downloads.blocked.${job.blockedReason}` : "downloads.blocked.other") as MessageKey)}
        </p>
      )}
      {job.status === "retry_wait" && job.nextRetryAt && <p className="muted small">{t("downloads.retry_at", { time: formatDateTime(job.nextRetryAt) })}</p>}
      {job.partialBytes > 0 && job.status !== "running" && job.status !== "succeeded" && (
        <p className="muted small" data-testid="job-partial">
          {t("downloads.partial", { size: formatBytes(job.partialBytes) })}
        </p>
      )}
      <div className="actions">
        {job.canRetry && (
          <button type="button" className="button button-primary" disabled={busy} onClick={() => onAction("retry", job)} data-testid="job-retry">
            {t("downloads.retry")}
          </button>
        )}
        {job.canCancel && (
          <button type="button" className="button" disabled={busy} onClick={() => onAction("cancel", job)} data-testid="job-cancel">
            {t("downloads.cancel")}
          </button>
        )}
        {job.canRemovePartial && (
          <button type="button" className="button button-quiet" disabled={busy} onClick={() => onAction("remove", job)} data-testid="job-remove-partial">
            {t("downloads.remove_partial")}
          </button>
        )}
      </div>
    </li>
  );
}

/** The Downloads screen (SPEC §9.3): active, failed and completed jobs. */
export function DownloadsView({ client, notify }: { client: DownloadClient; notify: (text: string, tone?: "info" | "error") => void }) {
  const [state, setState] = useState<{ status: "loading" } | { status: "error"; code: string } | { status: "ready"; jobs: JobDto[] }>({ status: "loading" });
  const [busy, setBusy] = useState(false);
  const load = useCallback(
    () =>
      client.listJobs().then(
        (jobs) => setState({ status: "ready", jobs }),
        (e) => setState({ status: "error", code: errorCode(e) }),
      ),
    [client],
  );
  useEffect(() => {
    let live = true;
    let unlisten: (() => void) | null = null;
    void load();
    client.onJobChanged(() => live && void load()).then(
      (u) => (live ? (unlisten = u) : u()),
      () => {},
    );
    return () => {
      live = false;
      unlisten?.();
    };
  }, [client, load]);

  const act = async (a: "retry" | "cancel" | "remove", j: JobDto) => {
    setBusy(true);
    try {
      if (a === "retry") await client.retryJob(j.id);
      else if (a === "cancel") await client.cancelJob(j.id);
      else notify(t("downloads.removed_partial", { size: formatBytes((await client.removePartial(j.id)).freedBytes) }));
      await load();
    } catch (e) {
      notify(errorMessage(errorCode(e)), "error");
    } finally {
      setBusy(false);
    }
  };

  if (state.status === "loading") return <p role="status">Loading…</p>;
  if (state.status === "error") return <p role="alert">{errorMessage(state.code)}</p>;
  if (state.jobs.length === 0)
    return (
      <div className="results-state" data-testid="downloads-empty">
        <h2>{t("downloads.empty.title")}</h2>
        <p>{t("downloads.empty.body")}</p>
      </div>
    );
  const groups: [string, string, JobDto[]][] = [
    ["active", t("downloads.active"), state.jobs.filter((j) => ACTIVE.has(j.status))],
    ["failed", t("downloads.failed"), state.jobs.filter((j) => j.status === "failed" || j.status === "canceled")],
    ["done", t("downloads.done"), state.jobs.filter((j) => j.status === "succeeded")],
  ];
  return (
    <div className="downloads" data-testid="downloads">
      {groups.map(([key, title, list]) =>
        list.length === 0 ? null : (
          <section key={key} aria-labelledby={`dl-${key}`} data-testid={`downloads-${key}`}>
            <h2 id={`dl-${key}`} className="downloads-heading">
              {title} <span className="muted">({list.length})</span>
            </h2>
            <ul className="job-list">
              {list.map((j) => (
                <JobRow key={j.id} job={j} busy={busy} onAction={(a, job) => void act(a, job)} />
              ))}
            </ul>
          </section>
        ),
      )}
    </div>
  );
}
