import { useEffect, useMemo, useState } from "react";
import { errorCode } from "../ipc/errors";
import type { OpResult, ProposalGroupDto, SuggestClient } from "../ipc/types";
import { t, tn } from "../i18n";
import { errorMessage } from "../labels";
import { ConfirmDialog } from "./Dialogs";

/** Groups shown at first and per "Show more". */
export const GROUP_PAGE = 60;

type Load = { status: "loading" } | { status: "error"; code: string } | { status: "ready"; groups: ProposalGroupDto[] };

const norm = (s: string) => s.trim().toLocaleLowerCase("tr");

/**
 * Suggested tags across the library (F7.3 follow-up, ADR-0038). The
 * suggestions "Suggest for all" made are grouped by value, the values
 * suggested for the most videos first, so one click can add a tag to all
 * of them, or reject it for all of them. A group opens to its videos for
 * one-by-one decisions. Nothing is tagged until the user accepts (AI-B02);
 * an accept is one undoable "add tag".
 */
export function SuggestionReview({
  client,
  reloadKey,
  onDecided,
  onOpenItem,
}: {
  client: SuggestClient;
  reloadKey: number;
  /** After a decision: the message for a toast, and the "add tag" an accept made. */
  onDecided: (text: string, op: OpResult | null) => void;
  onOpenItem: (itemId: string) => void;
}) {
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [filter, setFilter] = useState("");
  const [shown, setShown] = useState(GROUP_PAGE);
  const [open, setOpen] = useState<string | null>(null);
  const [edit, setEdit] = useState<{ key: string; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [askReject, setAskReject] = useState<ProposalGroupDto | null>(null);

  useEffect(() => {
    let live = true;
    client.listAiProposalGroups().then(
      (groups) => live && setLoad({ status: "ready", groups }),
      (e) => live && setLoad({ status: "error", code: errorCode(e) }),
    );
    return () => {
      live = false;
    };
  }, [client, reloadKey]);

  const groups = load.status === "ready" ? load.groups : [];
  const visible = useMemo(() => {
    const f = norm(filter);
    return f ? groups.filter((g) => g.normalizedValue.includes(f) || norm(g.value).includes(f)) : groups;
  }, [groups, filter]);

  /** Drops decided suggestions from the list without asking the backend again. */
  const drop = (ids: string[]) => {
    const gone = new Set(ids);
    setLoad((l) =>
      l.status === "ready"
        ? { status: "ready", groups: l.groups.map((g) => ({ ...g, items: g.items.filter((i) => !gone.has(i.proposalId)) })).filter((g) => g.items.length > 0) }
        : l,
    );
  };

  const decide = async (g: ProposalGroupDto, ids: string[], accept: boolean, name: string | null) => {
    setBusy(true);
    try {
      const r = await client.decideAiProposals(ids, accept, name);
      drop(ids);
      setEdit(null);
      const tag = name ?? g.value;
      onDecided(accept ? tn("review.added", r.op?.changed ?? r.decided, { tag }) : tn("review.rejected", r.decided, { tag }), r.op);
    } catch (e) {
      onDecided(errorMessage(errorCode(e)), null);
      // Something changed elsewhere (the item's details): start over.
      client.listAiProposalGroups().then((groups) => setLoad({ status: "ready", groups }), () => undefined);
    } finally {
      setBusy(false);
    }
  };

  if (load.status === "loading") return <p role="status">{t("review.loading")}</p>;
  if (load.status === "error")
    return (
      <p role="alert" className="warning">
        {errorMessage(load.code)}
      </p>
    );
  if (groups.length === 0)
    return (
      <div className="results-state" data-testid="review-empty">
        <h2>{t("review.empty_title")}</h2>
        <p>{t("review.empty_body")}</p>
      </div>
    );

  const total = groups.reduce((n, g) => n + g.items.length, 0);
  return (
    <div className="review" data-testid="suggestion-review" aria-busy={busy}>
      <p className="muted small" data-testid="review-summary">
        {tn("review.summary", total, { tags: groups.length })}
      </p>
      <label className="field review-filter">
        <span>{t("review.filter")}</span>
        <input type="search" value={filter} onChange={(e) => (setFilter(e.target.value), setShown(GROUP_PAGE))} placeholder={t("review.filter_placeholder")} data-testid="review-filter" />
      </label>
      {visible.length === 0 && <p className="muted small">{t("review.no_match")}</p>}
      <ul className="review-list">
        {visible.slice(0, shown).map((g) => {
          const ids = g.items.map((i) => i.proposalId);
          const n = g.items.length;
          const editing = edit?.key === g.normalizedValue;
          const isOpen = open === g.normalizedValue;
          return (
            <li key={g.normalizedValue} className="review-group" data-testid="review-group" data-value={g.value}>
              <div className="review-head">
                {editing ? (
                  <form
                    className="review-edit"
                    onSubmit={(e) => {
                      e.preventDefault();
                      if (edit.text.trim()) void decide(g, ids, true, edit.text.trim());
                    }}
                  >
                    <input
                      className="chip-input"
                      value={edit.text}
                      onChange={(e) => setEdit({ key: g.normalizedValue, text: e.target.value })}
                      onKeyDown={(e) => e.key === "Escape" && setEdit(null)}
                      aria-label={t("suggest.item.edit_label", { value: g.value })}
                      maxLength={100}
                      autoFocus
                      data-testid="review-edit-input"
                    />
                    <button type="submit" className="button button-primary" disabled={busy || !edit.text.trim()}>
                      {tn("review.add_named", n)}
                    </button>
                    <button type="button" className="link-button" onClick={() => setEdit(null)}>
                      {t("common.cancel")}
                    </button>
                  </form>
                ) : (
                  <>
                    <button type="button" className="review-name" disabled={busy} onClick={() => setEdit({ key: g.normalizedValue, text: g.value })} title={t("review.edit_hint")} data-testid="review-name">
                      #{g.value}
                    </button>
                    {g.tagExists && <span className="badge small">{t("review.existing")}</span>}
                    <span className="muted small">{tn("review.videos", n)}</span>
                    <span className="review-actions">
                      <button type="button" className="button" disabled={busy} onClick={() => void decide(g, ids, true, null)} data-testid="review-accept">
                        {n === 1 ? t("review.add_one") : t("review.add_all", { count: n })}
                      </button>
                      <button type="button" className="link-button" disabled={busy} onClick={() => (n === 1 ? void decide(g, ids, false, null) : setAskReject(g))} data-testid="review-reject">
                        {t("review.reject")}
                      </button>
                      <button type="button" className="link-button" aria-expanded={isOpen} onClick={() => setOpen(isOpen ? null : g.normalizedValue)} data-testid="review-toggle">
                        {isOpen ? t("review.hide_videos") : t("review.show_videos")}
                      </button>
                    </span>
                  </>
                )}
              </div>
              {isOpen && (
                <ul className="review-items" data-testid="review-items">
                  {g.items.map((i) => (
                    <li key={i.proposalId} data-testid="review-item">
                      <button type="button" className="link-button review-title" onClick={() => onOpenItem(i.itemId)}>
                        {i.title || t("review.untitled")}
                      </button>
                      <button type="button" className="chip-x" disabled={busy} aria-label={t("suggest.item.accept_label", { value: g.value })} onClick={() => void decide(g, [i.proposalId], true, g.value)} data-testid="review-item-accept">
                        ✓
                      </button>
                      <button type="button" className="chip-x" disabled={busy} aria-label={t("suggest.item.reject_label", { value: g.value })} onClick={() => void decide(g, [i.proposalId], false, null)} data-testid="review-item-reject">
                        ×
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </li>
          );
        })}
      </ul>
      {visible.length > shown && (
        <button type="button" className="button" onClick={() => setShown((s) => s + GROUP_PAGE)} data-testid="review-more">
          {t("review.more", { count: visible.length - shown })}
        </button>
      )}
      {askReject && (
        <ConfirmDialog
          title={t("review.reject_title", { tag: askReject.value })}
          body={<p>{t("review.reject_body", { count: askReject.items.length })}</p>}
          confirm={t("review.reject_confirm", { count: askReject.items.length })}
          onClose={() => setAskReject(null)}
          onSubmit={async () => {
            const g = askReject;
            setAskReject(null);
            await decide(
              g,
              g.items.map((i) => i.proposalId),
              false,
              null,
            );
          }}
        />
      )}
    </div>
  );
}
