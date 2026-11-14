import { useCallback, useEffect, useState } from "react";
import { errorCode } from "../ipc/errors";
import type { DownloadClient, JobDto, ModelClient, ModelDto, SearchIndexDto } from "../ipc/types";
import { formatBytes } from "../format";
import { t } from "../i18n";
import { errorMessage } from "../labels";
import { ProgressBar } from "./DownloadsView";
import { ModelConsentDialog, statusText } from "./Transcription";

type Notify = (text: string, tone?: "info" | "error") => void;

const openJob = (j: JobDto | null) => j !== null && !["succeeded", "failed", "canceled"].includes(j.status);

/**
 * Search by meaning (STATE.md F7.1, SPEC §16.3, AC-24): the embedding model
 * (downloaded only after the consent dialog, like a transcription model) and
 * the index of transcript passages. A rebuild runs beside the current index,
 * which keeps answering until the new one is complete; cancel keeps it.
 */
export function SearchIndexBlock({ client, notify, onChanged }: { client: ModelClient & Pick<DownloadClient, "cancelJob" | "retryJob">; notify: Notify; onChanged?: () => void }) {
  const [view, setView] = useState<SearchIndexDto | null>(null);
  const [asking, setAsking] = useState<ModelDto | null>(null);
  const [busy, setBusy] = useState(false);
  const [freeBytes, setFreeBytes] = useState<number | null>(null);
  const fail = useCallback((e: unknown) => notify(errorMessage(errorCode(e)), "error"), [notify]);
  const reload = useCallback(() => client.getSearchIndex().then(setView, fail), [client, fail]);
  useEffect(() => {
    void reload();
    void client.listModels().then((o) => setFreeBytes(o.freeBytes), () => undefined);
  }, [reload, client]);

  const running = view !== null && (openJob(view.job) || view.models.some((m) => openJob(m.job)));
  useEffect(() => {
    if (!running) return;
    const id = setInterval(() => void reload(), 1000);
    return () => clearInterval(id);
  }, [running, reload]);

  const run = async (f: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await f();
      await reload();
      onChanged?.();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };

  if (!view) return null;
  const model = view.models[0];
  const ready = view.models.some((m) => m.status === "ready");
  const job = view.job;
  const jobOpen = openJob(job);
  const a = view.active;
  const b = view.building;
  return (
    <div data-testid="search-index">
      <p className="muted small">{t("search_index.intro")}</p>
      {model && (
        <div className="model-row" data-testid="search-model" data-status={model.status}>
          <div className="model-head">
            <strong>{model.label}</strong>
            {model.sizeBytes !== null && <span className="muted small"> · {formatBytes(model.sizeBytes)}</span>}
          </div>
          <p className="muted small">{t("search_index.model_intro")}</p>
          <p className={model.status === "missing" || model.status === "corrupt" ? "warning small" : "muted small"} data-testid="search-model-status">
            {statusText(model)}
          </p>
          {model.job && openJob(model.job) && <ProgressBar job={model.job} />}
          <div className="actions">
            {!openJob(model.job) && model.status !== "ready" && (
              <button type="button" className="button" disabled={busy || model.sizeBytes === null} onClick={() => setAsking(model)} data-testid="search-model-download">
                {model.status === "not_installed" ? t("models.download") : t("models.download_again")}
              </button>
            )}
            {openJob(model.job) && model.job?.canCancel && (
              <button type="button" className="button" disabled={busy} onClick={() => void run(() => client.cancelJob(model.job!.id))} data-testid="search-model-cancel">
                {t("common.cancel")}
              </button>
            )}
            {model.status !== "not_installed" && !openJob(model.job) && (
              <button type="button" className="link-button" disabled={busy} onClick={() => void run(() => client.removeModel(model.id))} data-testid="search-model-remove">
                {t("models.remove")}
              </button>
            )}
          </div>
        </div>
      )}

      <div className="model-row" data-testid="search-index-state" data-active={a ? "yes" : "no"} data-building={b ? "yes" : "no"}>
        {a ? (
          <p className="small" data-testid="search-index-active">
            {t("search_index.active", { done: a.done, total: a.total, model: a.modelLabel ?? a.modelId })}
            {a.done < a.total && !jobOpen && <> {t("search_index.active_behind", { missing: a.total - a.done })}</>}
          </p>
        ) : (
          <p className="muted small">{t("search_index.none")}</p>
        )}
        {a && !view.searchable && ready === false && (
          <p className="warning small" data-testid="search-index-model-gone">
            {t("search_index.model_gone", { model: a.modelLabel ?? a.modelId })}
          </p>
        )}
        {!view.toolReady && <p className="warning small">{t("search_index.need_tool")}</p>}
        {!ready && !a && <p className="muted small">{t("search_index.need_model")}</p>}
        {jobOpen && job && (
          <>
            <p className="muted small" data-testid="search-index-progress">
              {job.status === "queued"
                ? t("search_index.queued")
                : b
                  ? t("search_index.building", { done: b.done, total: b.total })
                  : t("search_index.updating", { done: job.unitsDone ?? 0, total: job.unitsTotal ?? 0 })}
            </p>
            <ProgressBar job={job} />
          </>
        )}
        {!jobOpen && job?.status === "failed" && (
          <p className="warning small" data-testid="search-index-failed">
            {t("search_index.failed", { reason: errorMessage(job.errorCode ?? "INTERNAL_ERROR") })}
          </p>
        )}
        {!jobOpen && job?.status === "canceled" && job.kind === "reindex" && <p className="muted small">{t("search_index.canceled")}</p>}
        <div className="actions">
          {!jobOpen && ready && view.toolReady && (!a || a.done < a.total || !view.searchable) && (
            <button type="button" className="button" disabled={busy} onClick={() => void run(() => client.buildSearchIndex(false))} data-testid="search-index-build">
              {a && view.searchable ? t("search_index.update") : t("search_index.build")}
            </button>
          )}
          {!jobOpen && ready && view.toolReady && a && view.searchable && (
            <button type="button" className="link-button" disabled={busy} onClick={() => void run(() => client.buildSearchIndex(true))} data-testid="search-index-rebuild">
              {t("search_index.rebuild")}
            </button>
          )}
          {jobOpen && b && (
            <button type="button" className="button" disabled={busy} onClick={() => void run(() => client.cancelSearchIndex())} data-testid="search-index-cancel">
              {t("search_index.cancel")}
            </button>
          )}
        </div>
      </div>
      {asking && (
        <ModelConsentDialog
          model={asking}
          freeBytes={freeBytes}
          purpose="search"
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
