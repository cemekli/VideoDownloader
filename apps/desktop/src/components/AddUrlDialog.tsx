import { useEffect, useId, useRef, useState } from "react";
import { errorCode } from "../ipc/errors";
import type { BackendClient, LibraryItemDto, SaveUrlResult, UrlCheck } from "../ipc/types";
import { PLATFORM_LABEL, urlErrorMessage, urlWarningMessage } from "../labels";

// Add URL (SPEC §8.1, §9.3; STATE.md F1.8). Paste, then Enter: the bookmark
// is saved locally without waiting for any network work (CAP-03). The text
// is checked offline as it changes; nothing is written until Save.

type Check = { state: "empty" } | { state: "checking" } | { state: "ok"; data: UrlCheck } | { state: "error"; code: string };

interface Props {
  client: BackendClient;
  initialUrl: string;
  online: boolean;
  onClose: () => void;
  /** A new bookmark was saved. */
  onSaved: (r: SaveUrlResult) => void;
  /** Go to an item that already holds this link (CAP-06). */
  onOpenExisting: (item: LibraryItemDto) => void;
  /** An item came back from Recently removed (AC-45). */
  onRestored: (item: LibraryItemDto) => void;
}

const CHECK_DELAY_MS = 120;

export function AddUrlDialog({ client, initialUrl, online, onClose, onSaved, onOpenExisting, onRestored }: Props) {
  const [text, setText] = useState(initialUrl);
  const [check, setCheck] = useState<Check>(initialUrl.trim() ? { state: "checking" } : { state: "empty" });
  const [busy, setBusy] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const seq = useRef(0);
  const dialog = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const ids = { title: useId(), help: useId(), status: useId() };

  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, []);

  // Offline check as the text changes; a stale answer never wins.
  useEffect(() => {
    const n = ++seq.current;
    setSaveError(null);
    if (!text.trim()) {
      setCheck({ state: "empty" });
      return;
    }
    setCheck({ state: "checking" });
    const t = setTimeout(() => {
      client.checkUrl(text).then(
        (data) => n === seq.current && setCheck({ state: "ok", data }),
        (e) => n === seq.current && setCheck({ state: "error", code: errorCode(e) }),
      );
    }, CHECK_DELAY_MS);
    return () => clearTimeout(t);
  }, [client, text]);

  const existing = check.state === "ok" ? check.data.existing : null;

  const save = async () => {
    if (busy || !text.trim()) return;
    setBusy(true);
    try {
      const r = await client.saveUrl(text);
      if (r.outcome === "created") onSaved(r);
      else if (r.removed) {
        // Shown in the dialog: Restore or Cancel.
        seq.current++;
        const existing = { item: r.item, removed: true, matchedBy: r.matchedBy ?? "canonical_url" } as const;
        setCheck({ state: "ok", data: { platform: r.item.platform, shape: "unknown", canonicalUrl: "", title: r.item.title, warnings: r.warnings, existing } });
      } else onOpenExisting(r.item);
    } catch (e) {
      setSaveError(errorCode(e));
    } finally {
      setBusy(false);
    }
  };

  const restore = async (item: LibraryItemDto) => {
    setBusy(true);
    try {
      onRestored(await client.restoreItem(item.id));
    } catch (e) {
      setSaveError(errorCode(e));
      setBusy(false);
    }
  };

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (existing?.removed) void restore(existing.item);
    else if (existing) onOpenExisting(existing.item);
    else if (check.state !== "error") void save();
  };

  // Esc closes; Tab stays inside the dialog (§9.4).
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onClose();
      return;
    }
    if (e.key !== "Tab") return;
    const f = [...(dialog.current?.querySelectorAll<HTMLElement>("input, button:not(:disabled)") ?? [])];
    const i = f.indexOf(document.activeElement as HTMLElement);
    if (e.shiftKey && i <= 0) {
      e.preventDefault();
      f[f.length - 1]?.focus();
    } else if (!e.shiftKey && i === f.length - 1) {
      e.preventDefault();
      f[0]?.focus();
    }
  };

  const fieldError = check.state === "error" ? check.code : saveError;
  // Enabled while the check is still running: paste + Enter saves at once,
  // and the backend applies the same checks (§8.1, NFR-02).
  const canSave = text.trim() !== "" && check.state !== "error" && !existing && !busy;

  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div ref={dialog} className="modal" role="dialog" aria-modal="true" aria-labelledby={ids.title} onKeyDown={onKeyDown} data-testid="add-url-dialog">
        <form onSubmit={submit} noValidate>
          <h2 id={ids.title}>Add URL</h2>
          <label className="field">
            <span>Link</span>
            <input
              ref={input}
              type="text"
              inputMode="url"
              autoComplete="off"
              spellCheck={false}
              placeholder="https://…"
              value={text}
              onChange={(e) => setText(e.target.value)}
              aria-invalid={fieldError ? true : undefined}
              aria-describedby={`${ids.help} ${ids.status}`}
              data-testid="add-url-input"
            />
          </label>
          <p id={ids.help} className="muted small">
            Paste a video link. It is saved as a bookmark on this computer right away; nothing is downloaded.
          </p>
          <div id={ids.status} className="add-url-status" aria-live="polite" data-testid="add-url-status" data-state={fieldError ? "error" : check.state === "ok" ? (existing ? (existing.removed ? "removed" : "existing") : "ready") : check.state}>
            {fieldError ? (
              <p className="field-error" data-testid="add-url-error">
                {urlErrorMessage(fieldError)}
              </p>
            ) : check.state === "ok" ? (
              <Preview data={check.data} online={online} />
            ) : null}
          </div>
          <div className="modal-actions">
            {existing?.removed ? (
              <>
                <button type="button" className="button" onClick={onClose} data-testid="add-url-cancel">
                  Cancel
                </button>
                <button type="submit" className="button button-primary" disabled={busy} data-testid="add-url-restore">
                  Restore
                </button>
              </>
            ) : existing ? (
              <>
                <button type="button" className="button" onClick={onClose} data-testid="add-url-cancel">
                  Cancel
                </button>
                <button type="submit" className="button button-primary" data-testid="add-url-open">
                  Open item
                </button>
              </>
            ) : (
              <>
                <button type="button" className="button" onClick={onClose} data-testid="add-url-cancel">
                  Cancel
                </button>
                <button type="submit" className="button button-primary" disabled={!canSave} data-testid="add-url-save">
                  {busy ? "Saving…" : "Save Bookmark"}
                </button>
              </>
            )}
          </div>
        </form>
      </div>
    </div>
  );
}

function Preview({ data, online }: { data: UrlCheck; online: boolean }) {
  if (data.existing?.removed) {
    return (
      <div className="add-url-preview" data-testid="add-url-removed">
        <p>
          <strong>This item is in Recently removed.</strong> Restore it with its notes, or cancel to leave it there.
        </p>
        <p className="muted">{data.existing.item.title}</p>
      </div>
    );
  }
  if (data.existing) {
    return (
      <div className="add-url-preview" data-testid="add-url-existing">
        <p>
          <strong>Already in your library.</strong> Nothing new will be saved.
        </p>
        <p className="muted">{data.existing.item.title}</p>
      </div>
    );
  }
  return (
    <div className="add-url-preview" data-testid="add-url-preview">
      <p>
        <span className="platform-chip" data-testid="add-url-platform">
          {PLATFORM_LABEL[data.platform]}
        </span>{" "}
        <span data-testid="add-url-title">{data.title}</span>
      </p>
      <p className="muted small">
        {online
          ? "The title comes from the link for now; details can be fetched later."
          : "You are offline. The bookmark is saved now; details wait until you reconnect."}
      </p>
      {data.warnings.map((w) => (
        <p key={w} className="warning small" data-testid="add-url-warning">
          {urlWarningMessage(w)}
        </p>
      ))}
    </div>
  );
}
