import { useCallback, useEffect, useState } from "react";
import { errorCode } from "../ipc/errors";
import type { JobDto, ModelClient, ModelDto, ModelOverview, DownloadClient } from "../ipc/types";
import { formatBytes } from "../format";
import { formatDateTime, t, type MessageKey } from "../i18n";
import { errorMessage } from "../labels";
import { Modal } from "./Modal";
import { blockedText, ProgressBar } from "./DownloadsView";

type Notify = (text: string, tone?: "info" | "error") => void;

/** A download is still open (queued, running, waiting to retry, blocked, ...). */
const openJob = (j: JobDto | null) => j !== null && !["succeeded", "failed", "canceled"].includes(j.status);

/** Room for the file plus a little headroom, as the backend checks (models.rs). */
const HEADROOM = 64 * 1024 * 1024;

/**
 * Settings → Transcription (STATE.md F6.1, SPEC §17.1, AC-20). Transcription
 * is off until the user downloads a model; each download asks first and shows
 * size, free space, source, licence and the memory the model needs.
 */
export function TranscriptionBlock({ client, notify }: { client: ModelClient & Pick<DownloadClient, "cancelJob" | "retryJob">; notify: Notify }) {
  const [view, setView] = useState<ModelOverview | null>(null);
  const [asking, setAsking] = useState<ModelDto | null>(null);
  const [busy, setBusy] = useState(false);
  const fail = useCallback((e: unknown) => notify(errorMessage(errorCode(e)), "error"), [notify]);
  const reload = useCallback(() => client.listModels().then(setView, fail), [client, fail]);

  useEffect(() => {
    void reload();
  }, [reload]);

  // While a download is open, refresh once a second for its progress.
  const active = view?.models.some((m) => openJob(m.job)) ?? false;
  useEffect(() => {
    if (!active) return;
    const id = setInterval(() => void reload(), 1000);
    return () => clearInterval(id);
  }, [active, reload]);

  const run = async (f: () => Promise<ModelOverview | void>) => {
    setBusy(true);
    try {
      const v = await f();
      if (v) setView(v);
      else await reload();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };

  if (!view) return null;
  return (
    <div data-testid="transcription">
      <p className="muted small">{t("models.intro")}</p>
      <ul className="model-list">
        {view.models.map((m) => (
          <ModelRow
            key={m.id}
            model={m}
            busy={busy}
            onDownload={() => setAsking(m)}
            onCancel={(j) => void run(async () => void (await client.cancelJob(j.id)))}
            onRetry={(j) => void run(async () => void (await client.retryJob(j.id)))}
            onVerify={() => void run(() => client.verifyModel(m.id))}
            onRemove={() => void run(() => client.removeModel(m.id))}
          />
        ))}
      </ul>
      {asking && (
        <ModelConsentDialog
          model={asking}
          freeBytes={view.freeBytes}
          onCancel={() => setAsking(null)}
          onAgree={() => {
            const id = asking.id;
            setAsking(null);
            void run(() => client.downloadModel(id, true));
          }}
        />
      )}
    </div>
  );
}

function statusText(m: ModelDto): string {
  if (m.job && openJob(m.job)) {
    if (m.job.status === "blocked") return blockedText(m.job);
    if (m.job.status === "retry_wait") return t("models.status.retry_wait");
    return m.job.phase === "verify" ? t("models.status.verifying") : t("models.status.downloading");
  }
  if (m.status === "pending" && m.job?.status === "failed") return t("models.status.failed", { reason: errorMessage(m.job.errorCode ?? "INTERNAL_ERROR") });
  if (m.status === "ready" && m.verifiedAt !== null) return t("models.status.ready", { date: formatDateTime(m.verifiedAt) });
  return t(`models.status.${m.status}` as MessageKey);
}

function ModelRow({
  model: m,
  busy,
  onDownload,
  onCancel,
  onRetry,
  onVerify,
  onRemove,
}: {
  model: ModelDto;
  busy: boolean;
  onDownload: () => void;
  onCancel: (j: JobDto) => void;
  onRetry: (j: JobDto) => void;
  onVerify: () => void;
  onRemove: () => void;
}) {
  const open = openJob(m.job);
  const installed = m.status === "ready" || m.status === "missing" || m.status === "corrupt";
  const warn = m.status === "missing" || m.status === "corrupt" || m.job?.status === "failed";
  return (
    <li className="model-row" data-testid="model-row" data-model={m.id} data-status={m.status}>
      <div className="model-head">
        <strong>{m.label}</strong>
        <span className="muted small">
          {" "}
          · {t(`models.languages.${m.languages}` as MessageKey)}
          {m.sizeBytes !== null && <> · {formatBytes(m.sizeBytes)}</>}
        </span>
      </div>
      <p className={warn ? "warning small" : "muted small"} data-testid="model-status">
        {statusText(m)}
      </p>
      {m.job && <ProgressBar job={m.job} />}
      <div className="actions">
        {!open && (m.status === "not_installed" || m.status === "pending" || m.status === "missing" || m.status === "corrupt") && (
          <button type="button" className="button" disabled={busy || m.sizeBytes === null} onClick={onDownload} aria-label={`${t("models.download")} ${m.label}`} data-testid="model-download">
            {m.status === "not_installed" ? t("models.download") : t("models.download_again")}
          </button>
        )}
        {open && m.job?.canCancel && (
          <button type="button" className="button" disabled={busy} onClick={() => onCancel(m.job!)} data-testid="model-cancel">
            {t("common.cancel")}
          </button>
        )}
        {open && m.job?.status === "blocked" && m.job.canRetry && (
          <button type="button" className="button" disabled={busy} onClick={() => onRetry(m.job!)} data-testid="model-retry">
            {t("models.retry")}
          </button>
        )}
        {installed && !open && (
          <button type="button" className="button" disabled={busy} onClick={onVerify} data-testid="model-verify">
            {t("models.verify")}
          </button>
        )}
        {(installed || m.status === "pending") && (
          <button type="button" className="link-button" disabled={busy} onClick={onRemove} aria-label={`${t("models.remove")} ${m.label}`} data-testid="model-remove">
            {t("models.remove")}
          </button>
        )}
      </div>
      {m.sizeBytes === null && <p className="muted small">{t("models.unpinned")}</p>}
    </li>
  );
}

/** §17.1: size, disk need, source and licence, CPU expectation; nothing starts before "Download". */
export function ModelConsentDialog({ model: m, freeBytes, onCancel, onAgree }: { model: ModelDto; freeBytes: number | null; onCancel: () => void; onAgree: () => void }) {
  const size = m.sizeBytes ?? 0;
  const tooBig = freeBytes !== null && freeBytes < size + HEADROOM;
  return (
    <Modal title={t("models.consent.title", { model: m.label })} onClose={onCancel} testId="model-consent">
      <p>{t("models.consent.intro")}</p>
      <table className="summary-table">
        <tbody>
          <tr>
            <th scope="row">{t("models.consent.size")}</th>
            <td data-testid="consent-size">{formatBytes(size)}</td>
          </tr>
          <tr>
            <th scope="row">{t("models.consent.free")}</th>
            <td>{freeBytes === null ? t("models.consent.free_unknown") : formatBytes(freeBytes)}</td>
          </tr>
          <tr>
            <th scope="row">{t("models.consent.memory")}</th>
            <td>{t("models.consent.memory_value", { size: formatBytes(m.upstreamMemoryMb * 1024 * 1024) })}</td>
          </tr>
          <tr>
            <th scope="row">{t("models.consent.source")}</th>
            <td data-testid="consent-source">{m.source}</td>
          </tr>
          <tr>
            <th scope="row">{t("models.consent.license")}</th>
            <td data-testid="consent-license">{m.license}</td>
          </tr>
        </tbody>
      </table>
      <p className="muted small">{t("models.consent.cpu")}</p>
      <p className="muted small">{t("models.consent.check")}</p>
      {tooBig && (
        <p className="warning small" role="alert" data-testid="consent-no-space">
          {t("models.consent.no_space")}
        </p>
      )}
      <div className="modal-actions">
        <button type="button" className="button" onClick={onCancel} data-testid="consent-cancel">
          {t("common.cancel")}
        </button>
        <button type="button" className="button button-primary" disabled={tooBig} onClick={onAgree} data-testid="consent-agree">
          {t("models.consent.agree")}
        </button>
      </div>
    </Modal>
  );
}
