import { useCallback, useEffect, useState } from "react";
import { errorCode } from "../ipc/errors";
import type { AiJobDto, AiProcessing, CpuUse, DownloadClient, MachineDto, ModelClient, Settings, SettingsPatch } from "../ipc/types";
import { formatBytes, formatDuration } from "../format";
import { formatDateTime, t, type MessageKey } from "../i18n";
import { errorMessage } from "../labels";
import { TranscriptionBlock } from "./Transcription";
import { languageName } from "./Transcripts";

type Notify = (text: string, tone?: "info" | "error") => void;

const CPU_USES: CpuUse[] = ["light", "balanced", "full"];
const isOpen = (a: AiJobDto) => !["succeeded", "failed", "canceled"].includes(a.job.status);

/**
 * The AI Processing screen (SPEC §9.3, STATE.md F6.6; AI-A05): what this
 * PC has (memory, processor, free space, the transcriber), the CPU limit
 * for transcription, the models with whether each fits, and the queue of
 * transcriptions with cancel. Transcription runs on the CPU only.
 */
export function AiProcessingView({
  client,
  notify,
  onSettings,
}: {
  client: ModelClient & Pick<DownloadClient, "cancelJob" | "retryJob">;
  notify: Notify;
  onSettings: (patch: SettingsPatch) => Promise<Settings | void> | void;
}) {
  const [view, setView] = useState<AiProcessing | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const fail = useCallback((e: unknown) => notify(errorMessage(errorCode(e)), "error"), [notify]);
  const reload = useCallback(
    () =>
      client.getAiProcessing().then(
        (v) => (setView(v), setError(null)),
        (e) => setError(errorMessage(errorCode(e))),
      ),
    [client],
  );
  useEffect(() => {
    void reload();
  }, [reload]);

  // While a transcription runs, refresh once a second for its progress.
  const active = view?.jobs.some(isOpen) ?? false;
  useEffect(() => {
    if (!active) return;
    const id = setInterval(() => void reload(), 1000);
    return () => clearInterval(id);
  }, [active, reload]);

  const setCpu = async (cpu: CpuUse) => {
    setBusy(true);
    try {
      await onSettings({ transcriptionCpu: cpu });
      await reload();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };

  const cancel = async (a: AiJobDto) => {
    setBusy(true);
    try {
      await client.cancelJob(a.job.id);
      await reload();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };

  if (error && !view) return <p role="alert">{error}</p>;
  if (!view) return <p role="status">{t("ai.loading")}</p>;
  const open = view.jobs.filter(isOpen);
  const done = view.jobs.filter((a) => !isOpen(a));
  return (
    <div className="downloads ai-processing" data-testid="ai-processing">
      <section aria-labelledby="ai-machine">
        <h2 id="ai-machine" className="downloads-heading">
          {t("ai.machine.title")}
        </h2>
        <MachineTable m={view.machine} />
        <fieldset className="ai-cpu" disabled={busy}>
          <legend>{t("ai.cpu.title")}</legend>
          {CPU_USES.map((u) => (
            <label key={u} className="small">
              <input type="radio" name="ai-cpu" value={u} checked={view.machine.cpuUse === u} onChange={() => void setCpu(u)} data-testid={`ai-cpu-${u}`} /> {t(`ai.cpu.${u}` as MessageKey)}
            </label>
          ))}
          <p className="muted small" data-testid="ai-threads">
            {t("ai.cpu.threads", { threads: view.machine.threads, cores: view.machine.cores })}
          </p>
        </fieldset>
      </section>

      <section aria-labelledby="ai-queue">
        <h2 id="ai-queue" className="downloads-heading">
          {t("ai.queue.title")}
        </h2>
        {view.jobs.length === 0 && <p className="muted small">{t("ai.queue.empty")}</p>}
        {(open.length > 0 || done.length > 0) && (
          <ul className="job-list" data-testid="ai-jobs">
            {[...open, ...done].map((a) => (
              <AiJobRow key={a.job.id} a={a} busy={busy} onCancel={() => void cancel(a)} />
            ))}
          </ul>
        )}
      </section>

      <section aria-labelledby="ai-models">
        <h2 id="ai-models" className="downloads-heading">
          {t("ai.models.title")}
        </h2>
        <TranscriptionBlock client={client} notify={notify} onChanged={() => void reload()} />
      </section>
    </div>
  );
}

function MachineTable({ m }: { m: MachineDto }) {
  const mem = m.memory;
  return (
    <table className="summary-table" data-testid="ai-machine">
      <tbody>
        <tr>
          <th scope="row">{t("ai.machine.memory")}</th>
          <td data-testid="ai-memory">{mem ? t("ai.machine.memory_value", { free: formatBytes(mem.availableBytes), total: formatBytes(mem.totalBytes) }) : t("ai.machine.unknown")}</td>
        </tr>
        <tr>
          <th scope="row">{t("ai.machine.processor")}</th>
          <td>{t("ai.machine.processor_value", { cores: m.cores })}</td>
        </tr>
        <tr>
          <th scope="row">{t("ai.machine.device")}</th>
          <td>{t("ai.machine.cpu_only")}</td>
        </tr>
        <tr>
          <th scope="row">{t("ai.machine.disk")}</th>
          <td>{m.stagingFreeBytes === null ? t("ai.machine.unknown") : formatBytes(m.stagingFreeBytes)}</td>
        </tr>
        <tr>
          <th scope="row">{t("ai.machine.transcriber")}</th>
          <td data-testid="ai-transcriber" data-state={m.transcriber}>
            {t(`ai.transcriber.${m.transcriber}` as MessageKey)}
          </td>
        </tr>
      </tbody>
    </table>
  );
}

function jobText(a: AiJobDto): string {
  const j = a.job;
  const model = a.model ?? "?";
  if (j.status === "queued") return t("transcripts.local.queued", { model });
  if (j.status === "running" || j.status === "canceling") {
    return j.phase === "transcribe" && j.unitsDone !== null ? t("transcripts.local.running_pct", { pct: j.unitsDone, model }) : t("transcripts.local.preparing", { model });
  }
  if (j.status === "succeeded") return t("ai.job.done", { model, elapsed: formatDuration(a.elapsedMs ?? 0) });
  if (j.status === "canceled") return t("ai.job.canceled", { model });
  return t("transcripts.local.failed", { reason: errorMessage(j.errorCode ?? "INTERNAL_ERROR") });
}

function AiJobRow({ a, busy, onCancel }: { a: AiJobDto; busy: boolean; onCancel: () => void }) {
  const j = a.job;
  const lang = a.language && a.language !== "auto" ? languageName(a.language) : t("transcripts.local.auto");
  const warn = j.status === "failed" || j.status === "blocked";
  return (
    <li className="job-row" data-testid="ai-job" data-status={j.status}>
      <div className="job-main">
        <strong>{j.itemTitle ?? t("ai.job.untitled")}</strong>
        <span className="muted small">
          {" "}
          · {lang} · {formatDateTime(j.createdAt)}
        </span>
      </div>
      <p className={warn ? "warning small" : "muted small"} data-testid="ai-job-status">
        {jobText(a)}
      </p>
      {j.canCancel && isOpen(a) && (
        <div className="actions">
          <button type="button" className="button" disabled={busy} onClick={onCancel} data-testid="ai-job-cancel">
            {t("transcripts.local.cancel")}
          </button>
        </div>
      )}
    </li>
  );
}
