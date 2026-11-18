import { useCallback, useEffect, useState } from "react";
import { errorCode } from "../ipc/errors";
import type { DownloadClient, JobDto, ModelClient, SearchIndexDto } from "../ipc/types";
import { t } from "../i18n";
import { errorMessage } from "../labels";
import { ProgressBar } from "./DownloadsView";
import { ConfirmDialog } from "./Dialogs";

type Notify = (text: string, tone?: "info" | "error") => void;

const openJob = (j: JobDto | null) => j !== null && !["succeeded", "failed", "canceled"].includes(j.status);

export const DEFAULT_URL = "http://localhost:11434/v1";
export const DEFAULT_MODEL = "bge-m3";

/** What a failed API check means here; the general texts talk about video sites. */
const API_ERRORS = ["INVALID_URL", "URL_HAS_CREDENTIALS", "NETWORK_UNAVAILABLE", "ACCESS_DENIED", "MODEL_MISSING", "RATE_LIMITED", "OFFLINE_MODE", "EXTRACTOR_FAILED"] as const;
export function apiError(code: string): string {
  const c = API_ERRORS.find((x) => x === code);
  return c ? t(`search_index.error.${c}`) : errorMessage(code);
}

/**
 * Search by meaning (STATE.md F7.1, SPEC §16.3, AC-24, ADR-0035).
 * VideoVault hosts no model: the user gives the address of an
 * OpenAI-compatible embedding API (Ollama, LM Studio, a server of their
 * own). "Check and save" sends one request; an address other than this PC
 * is asked about first, because transcript text goes there (AI-C07). The
 * key is sent once and kept in the OS credential store. A rebuild runs
 * beside the current index, which keeps answering until the new one is
 * complete; cancel keeps it.
 */
export function SearchIndexBlock({ client, notify, onChanged }: { client: ModelClient & Pick<DownloadClient, "cancelJob" | "retryJob">; notify: Notify; onChanged?: () => void }) {
  const [view, setView] = useState<SearchIndexDto | null>(null);
  const [busy, setBusy] = useState(false);
  const [editing, setEditing] = useState(false);
  const [url, setUrl] = useState(DEFAULT_URL);
  const [model, setModel] = useState(DEFAULT_MODEL);
  /** Empty: keep the saved key (or none). */
  const [keyText, setKeyText] = useState("");
  const [dropKey, setDropKey] = useState(false);
  const [checkError, setCheckError] = useState<string | null>(null);
  const [askRemote, setAskRemote] = useState<string | null>(null);
  const fail = useCallback((e: unknown) => notify(errorMessage(errorCode(e)), "error"), [notify]);
  const reload = useCallback(() => client.getSearchIndex().then(setView, fail), [client, fail]);
  useEffect(() => {
    void reload();
  }, [reload]);

  const running = view !== null && openJob(view.job);
  useEffect(() => {
    if (!running) return;
    const id = setInterval(() => void reload(), 1000);
    return () => clearInterval(id);
  }, [running, reload]);

  const run = async (f: () => Promise<SearchIndexDto>) => {
    setBusy(true);
    try {
      setView(await f());
      onChanged?.();
    } catch (e) {
      fail(e);
      await reload();
    } finally {
      setBusy(false);
    }
  };

  const save = async (allowRemote: boolean) => {
    setBusy(true);
    setCheckError(null);
    try {
      const key = dropKey ? "" : keyText !== "" ? keyText : null;
      const v = await client.setEmbeddingApi(url.trim(), model.trim(), key, allowRemote);
      setView(v);
      setEditing(false);
      setKeyText("");
      setDropKey(false);
      notify(t("search_index.api_saved", { dims: v.api?.dims ?? 0 }));
      onChanged?.();
    } catch (e) {
      const code = errorCode(e);
      if (code === "URL_BLOCKED" && !allowRemote) setAskRemote(url.trim());
      else setCheckError(apiError(code));
    } finally {
      setBusy(false);
    }
  };

  if (!view) return null;
  const api = view.api;
  const job = view.job;
  const jobOpen = openJob(job);
  const a = view.active;
  const b = view.building;
  const showForm = editing || !api;
  const startEdit = () => {
    setUrl(api?.url ?? DEFAULT_URL);
    setModel(api?.model ?? DEFAULT_MODEL);
    setKeyText("");
    setDropKey(false);
    setCheckError(null);
    setEditing(true);
  };
  return (
    <div data-testid="search-index">
      <p className="muted small">{t("search_index.intro")}</p>
      <div className="model-row" data-testid="search-api" data-state={api ? "set" : "none"}>
        {api && !showForm && (
          <>
            <p className="small" data-testid="search-api-summary">
              {t("search_index.api_summary", { model: api.model, url: api.url, dims: api.dims })}
            </p>
            <p className="muted small" data-testid="search-api-where">
              {api.local ? t("search_index.api_local") : t("search_index.api_remote")}
              {api.hasKey && <> {t("search_index.api_has_key")}</>}
            </p>
            <div className="actions">
              <button type="button" className="button" disabled={busy} onClick={startEdit} data-testid="search-api-edit">
                {t("search_index.api_change")}
              </button>
              <button type="button" className="link-button" disabled={busy} onClick={() => void run(() => client.clearEmbeddingApi())} data-testid="search-api-clear">
                {t("search_index.api_forget")}
              </button>
            </div>
          </>
        )}
        {showForm && (
          <form
            className="api-form"
            data-testid="search-api-form"
            onSubmit={(e) => {
              e.preventDefault();
              void save(false);
            }}
          >
            <p className="muted small">{t("search_index.api_intro")}</p>
            <label className="field">
              <span>{t("search_index.api_url")}</span>
              <input type="url" value={url} onChange={(e) => setUrl(e.target.value)} placeholder={DEFAULT_URL} spellCheck={false} data-testid="search-api-url" required />
            </label>
            <label className="field">
              <span>{t("search_index.api_model")}</span>
              <input type="text" value={model} onChange={(e) => setModel(e.target.value)} placeholder={DEFAULT_MODEL} spellCheck={false} data-testid="search-api-model" required />
            </label>
            <label className="field">
              <span>{t("search_index.api_key")}</span>
              <input
                type="password"
                value={keyText}
                onChange={(e) => setKeyText(e.target.value)}
                placeholder={api?.hasKey ? t("search_index.api_key_saved") : t("search_index.api_key_none")}
                autoComplete="off"
                disabled={dropKey}
                data-testid="search-api-key"
              />
            </label>
            {api?.hasKey && (
              <label className="check">
                <input type="checkbox" checked={dropKey} onChange={(e) => setDropKey(e.target.checked)} data-testid="search-api-drop-key" />
                {t("search_index.api_key_drop")}
              </label>
            )}
            {checkError && (
              <p className="warning small" role="alert" data-testid="search-api-error">
                {checkError}
              </p>
            )}
            <div className="actions">
              <button type="submit" className="button button-primary" disabled={busy || !url.trim() || !model.trim()} data-testid="search-api-save">
                {busy ? t("search_index.api_checking") : t("search_index.api_check")}
              </button>
              {api && (
                <button type="button" className="link-button" disabled={busy} onClick={() => setEditing(false)} data-testid="search-api-cancel">
                  {t("common.cancel")}
                </button>
              )}
            </div>
          </form>
        )}
      </div>

      <div className="model-row" data-testid="search-index-state" data-active={a ? "yes" : "no"} data-building={b ? "yes" : "no"}>
        {a ? (
          <p className="small" data-testid="search-index-active">
            {t("search_index.active", { done: a.done, total: a.total, model: a.modelId })}
            {a.done < a.total && !jobOpen && view.searchable && <> {t("search_index.active_behind", { missing: a.total - a.done })}</>}
          </p>
        ) : (
          <p className="muted small">{t("search_index.none")}</p>
        )}
        {a && api && !view.searchable && (
          <p className="warning small" data-testid="search-index-other">
            {t("search_index.other_api", { model: a.modelId, url: a.url })}
          </p>
        )}
        {!api && <p className="muted small">{t("search_index.need_api")}</p>}
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
            {t("search_index.failed", { reason: apiError(job.errorCode ?? "INTERNAL_ERROR") })}
          </p>
        )}
        {!jobOpen && job?.status === "canceled" && job.kind === "reindex" && <p className="muted small">{t("search_index.canceled")}</p>}
        <div className="actions">
          {!jobOpen && api && (!a || a.done < a.total || !view.searchable) && (
            <button type="button" className="button" disabled={busy} onClick={() => void run(() => client.buildSearchIndex(false))} data-testid="search-index-build">
              {a && view.searchable ? t("search_index.update") : t("search_index.build")}
            </button>
          )}
          {!jobOpen && api && a && view.searchable && (
            <button type="button" className="link-button" disabled={busy} onClick={() => void run(() => client.buildSearchIndex(true))} data-testid="search-index-rebuild">
              {t("search_index.rebuild")}
            </button>
          )}
          {jobOpen && (
            <button type="button" className="button" disabled={busy} onClick={() => void run(() => client.cancelSearchIndex())} data-testid="search-index-cancel">
              {t("search_index.cancel")}
            </button>
          )}
        </div>
      </div>
      {askRemote && (
        <ConfirmDialog
          title={t("search_index.remote.title")}
          body={
            <>
              <p data-testid="search-remote-host">{t("search_index.remote.body", { url: askRemote })}</p>
              <p className="muted small">{t("search_index.remote.offline")}</p>
            </>
          }
          confirm={t("search_index.remote.agree")}
          onClose={() => setAskRemote(null)}
          onSubmit={async () => {
            setAskRemote(null);
            await save(true);
          }}
        />
      )}
    </div>
  );
}
