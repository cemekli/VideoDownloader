import { useCallback, useEffect, useState } from "react";
import { errorCode } from "../ipc/errors";
import type { OpResult, ProposalDto, SuggestClient, TagSuggestDto } from "../ipc/types";
import { t } from "../i18n";
import { errorMessage } from "../labels";
import { ConfirmDialog } from "./Dialogs";

type Notify = (text: string, tone?: "info" | "error") => void;

export const DEFAULT_CHAT_URL = "http://localhost:11434/v1";
export const DEFAULT_CHAT_MODEL = "qwen3.5:4b";

/** What a failed request means here; the general texts talk about video sites. */
const API_ERRORS = ["INVALID_URL", "URL_HAS_CREDENTIALS", "NETWORK_UNAVAILABLE", "ACCESS_DENIED", "MODEL_MISSING", "RATE_LIMITED", "OFFLINE_MODE", "EXTRACTOR_FAILED", "TOOL_MISSING"] as const;
export function suggestError(code: string): string {
  const c = API_ERRORS.find((x) => x === code);
  return c ? t(`suggest.error.${c}`) : errorMessage(code);
}

/**
 * Tag suggestions in AI Processing (STATE.md F7.3, SPEC AI-B01…B04,
 * ADR-0037). VideoVault hosts no model: the user gives the address of an
 * OpenAI-compatible chat API (Ollama, LM Studio, a server of their own).
 * "Check and save" asks it one short question; an address other than this
 * PC is asked about first, because item text goes there (AI-C07).
 * "Suggest for all" walks the library one item at a time and skips items
 * whose text was already asked; suggestions wait in each item's details.
 */
export function TagSuggestBlock({ client, notify }: { client: SuggestClient; notify: Notify }) {
  const [view, setView] = useState<TagSuggestDto | null>(null);
  const [busy, setBusy] = useState(false);
  const [editing, setEditing] = useState(false);
  const [url, setUrl] = useState(DEFAULT_CHAT_URL);
  const [model, setModel] = useState(DEFAULT_CHAT_MODEL);
  /** Empty: keep the saved key (or none). */
  const [keyText, setKeyText] = useState("");
  const [dropKey, setDropKey] = useState(false);
  const [checkError, setCheckError] = useState<string | null>(null);
  const [askRemote, setAskRemote] = useState<string | null>(null);
  const fail = useCallback((e: unknown) => notify(suggestError(errorCode(e)), "error"), [notify]);
  const reload = useCallback(() => client.getTagSuggest().then(setView, fail), [client, fail]);
  useEffect(() => {
    void reload();
  }, [reload]);

  const running = view?.run?.running ?? false;
  useEffect(() => {
    if (!running) return;
    const id = setInterval(() => void reload(), 1000);
    return () => clearInterval(id);
  }, [running, reload]);

  const act = async (f: () => Promise<TagSuggestDto>) => {
    setBusy(true);
    try {
      setView(await f());
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
      setView(await client.setSuggestApi(url.trim(), model.trim(), key, allowRemote));
      setEditing(false);
      setKeyText("");
      setDropKey(false);
      notify(t("suggest.api_saved"));
    } catch (e) {
      const code = errorCode(e);
      if (code === "URL_BLOCKED" && !allowRemote) setAskRemote(url.trim());
      else setCheckError(suggestError(code));
    } finally {
      setBusy(false);
    }
  };

  if (!view) return null;
  const api = view.api;
  const run = view.run;
  const showForm = editing || !api;
  const startEdit = () => {
    setUrl(api?.url ?? DEFAULT_CHAT_URL);
    setModel(api?.model ?? DEFAULT_CHAT_MODEL);
    setKeyText("");
    setDropKey(false);
    setCheckError(null);
    setEditing(true);
  };
  return (
    <div data-testid="tag-suggest">
      <p className="muted small">{t("suggest.intro")}</p>
      <div className="model-row" data-testid="suggest-api" data-state={api ? "set" : "none"}>
        {api && !showForm && (
          <>
            <p className="small" data-testid="suggest-api-summary">
              {t("suggest.api_summary", { model: api.model, url: api.url })}
            </p>
            <p className="muted small">
              {api.local ? t("suggest.api_local") : t("suggest.api_remote")}
              {api.hasKey && <> {t("search_index.api_has_key")}</>}
            </p>
            <div className="actions">
              <button type="button" className="button" disabled={busy} onClick={startEdit} data-testid="suggest-api-edit">
                {t("search_index.api_change")}
              </button>
              <button type="button" className="link-button" disabled={busy || running} onClick={() => void act(() => client.clearSuggestApi())} data-testid="suggest-api-clear">
                {t("search_index.api_forget")}
              </button>
            </div>
          </>
        )}
        {showForm && (
          <form
            className="api-form"
            data-testid="suggest-api-form"
            onSubmit={(e) => {
              e.preventDefault();
              void save(false);
            }}
          >
            <p className="muted small">{t("suggest.api_intro")}</p>
            <label className="field">
              <span>{t("search_index.api_url")}</span>
              <input type="url" value={url} onChange={(e) => setUrl(e.target.value)} placeholder={DEFAULT_CHAT_URL} spellCheck={false} data-testid="suggest-api-url" required />
            </label>
            <label className="field">
              <span>{t("search_index.api_model")}</span>
              <input type="text" value={model} onChange={(e) => setModel(e.target.value)} placeholder={DEFAULT_CHAT_MODEL} spellCheck={false} data-testid="suggest-api-model" required />
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
                data-testid="suggest-api-key"
              />
            </label>
            {api?.hasKey && (
              <label className="check">
                <input type="checkbox" checked={dropKey} onChange={(e) => setDropKey(e.target.checked)} />
                {t("search_index.api_key_drop")}
              </label>
            )}
            {checkError && (
              <p className="warning small" role="alert" data-testid="suggest-api-error">
                {checkError}
              </p>
            )}
            <div className="actions">
              <button type="submit" className="button button-primary" disabled={busy || !url.trim() || !model.trim()} data-testid="suggest-api-save">
                {busy ? t("search_index.api_checking") : t("search_index.api_check")}
              </button>
              {api && (
                <button type="button" className="link-button" disabled={busy} onClick={() => setEditing(false)}>
                  {t("common.cancel")}
                </button>
              )}
            </div>
          </form>
        )}
      </div>

      <div className="model-row" data-testid="suggest-run" data-running={running ? "yes" : "no"}>
        {!api && <p className="muted small">{t("suggest.need_api")}</p>}
        {view.pending > 0 && (
          <p className="small" data-testid="suggest-pending">
            {t("suggest.pending", { count: view.pending })}
          </p>
        )}
        {run && (
          <p className={run.errorCode ? "warning small" : "muted small"} data-testid="suggest-run-status">
            {running
              ? t("suggest.run.running", { done: run.done, total: run.total })
              : run.errorCode
                ? t("suggest.run.failed", { done: run.done, total: run.total, reason: suggestError(run.errorCode) })
                : run.canceled
                  ? t("suggest.run.canceled", { done: run.done, total: run.total, suggested: run.suggested })
                  : t("suggest.run.done", { asked: run.asked, suggested: run.suggested })}
          </p>
        )}
        {running && run && <progress max={Math.max(1, run.total)} value={run.done} aria-label={t("suggest.run.title")} />}
        <div className="actions">
          {api && !running && (
            <>
              <button type="button" className="button" disabled={busy} onClick={() => void act(() => client.suggestAll(false))} data-testid="suggest-all">
                {t("suggest.all")}
              </button>
              <button type="button" className="link-button" disabled={busy} onClick={() => void act(() => client.suggestAll(true))} data-testid="suggest-all-again">
                {t("suggest.all_again")}
              </button>
            </>
          )}
          {running && (
            <button type="button" className="button" disabled={busy} onClick={() => void act(() => client.cancelSuggestAll())} data-testid="suggest-all-cancel">
              {t("suggest.all_cancel")}
            </button>
          )}
        </div>
      </div>
      {askRemote && (
        <ConfirmDialog
          title={t("suggest.remote.title")}
          body={
            <>
              <p>{t("suggest.remote.body", { url: askRemote })}</p>
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

/**
 * Suggested tags in an item's details (F7.3, AI-B02): nothing changes
 * until the user accepts a suggestion (✓, or edit it first); ✕ rejects it
 * for good. "Suggest tags" asks the chat API for this item now.
 */
export function ItemTagSuggestions({ client, itemId, onAccepted }: { client: SuggestClient; itemId: string; onAccepted: (op: OpResult | null) => void }) {
  const [list, setList] = useState<ProposalDto[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<{ text: string; warn: boolean } | null>(null);
  const [edit, setEdit] = useState<{ id: string; text: string } | null>(null);

  useEffect(() => {
    let live = true;
    setList(null);
    setNote(null);
    setEdit(null);
    client.listAiProposals(itemId).then(
      (l) => live && setList(l),
      () => live && setList([]),
    );
    return () => {
      live = false;
    };
  }, [client, itemId]);

  const ask = async (force: boolean) => {
    setBusy(true);
    setNote(null);
    try {
      const r = await client.suggestTags(itemId, force);
      setList(r.proposals);
      if (r.outcome === "no_text") setNote({ text: t("suggest.item.no_text"), warn: false });
      else if (r.outcome === "current") setNote({ text: t("suggest.item.current"), warn: false });
      else if (r.proposals.length === 0) setNote({ text: t("suggest.item.none"), warn: false });
    } catch (e) {
      setNote({ text: suggestError(errorCode(e)), warn: true });
    } finally {
      setBusy(false);
    }
  };

  const decide = async (p: ProposalDto, accept: boolean, name: string | null = null) => {
    setBusy(true);
    try {
      const r = await client.decideAiProposal(p.id, accept, name);
      setList(r.proposals);
      setEdit(null);
      if (accept) onAccepted(r.op);
    } catch (e) {
      setNote({ text: errorMessage(errorCode(e)), warn: true });
    } finally {
      setBusy(false);
    }
  };

  const pending = list ?? [];
  return (
    <section data-testid="inspector-suggestions" aria-busy={busy}>
      <h4>{t("suggest.item.title")}</h4>
      {pending.length > 0 && (
        <ul className="chips" data-testid="suggestion-list">
          {pending.map((p) =>
            edit?.id === p.id ? (
              <li key={p.id} className="tag-chip suggestion-chip">
                <form
                  onSubmit={(e) => {
                    e.preventDefault();
                    if (edit.text.trim()) void decide(p, true, edit.text.trim());
                  }}
                >
                  <input
                    className="chip-input"
                    value={edit.text}
                    onChange={(e) => setEdit({ id: p.id, text: e.target.value })}
                    onKeyDown={(e) => e.key === "Escape" && setEdit(null)}
                    aria-label={t("suggest.item.edit_label", { value: p.value })}
                    maxLength={100}
                    autoFocus
                    data-testid="suggestion-edit-input"
                  />
                </form>
              </li>
            ) : (
              <li key={p.id} className="tag-chip suggestion-chip" data-testid="suggestion" title={t("suggest.item.by", { model: p.modelId })}>
                <button type="button" className="chip-text" disabled={busy} onClick={() => setEdit({ id: p.id, text: p.value })} aria-label={t("suggest.item.edit_label", { value: p.value })}>
                  #{p.value}
                </button>
                <button type="button" className="chip-x" disabled={busy} aria-label={t("suggest.item.accept_label", { value: p.value })} onClick={() => void decide(p, true)} data-testid="suggestion-accept">
                  ✓
                </button>
                <button type="button" className="chip-x" disabled={busy} aria-label={t("suggest.item.reject_label", { value: p.value })} onClick={() => void decide(p, false)} data-testid="suggestion-reject">
                  ×
                </button>
              </li>
            ),
          )}
        </ul>
      )}
      {note && (
        <p className={note.warn ? "warning small" : "muted small"} role={note.warn ? "alert" : "status"} data-testid="suggestion-note">
          {note.text}
        </p>
      )}
      <div className="actions">
        <button type="button" className="link-button" disabled={busy || list === null} onClick={() => void ask(pending.length > 0)} data-testid="suggestion-ask">
          {busy ? t("suggest.item.asking") : pending.length > 0 ? t("suggest.item.again") : t("suggest.item.ask")}
        </button>
      </div>
    </section>
  );
}
