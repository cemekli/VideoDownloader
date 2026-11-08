import { useEffect, useState } from "react";
import { errorCode } from "../ipc/errors";
import type { DuplicateDto, LibraryClient, LibraryItemDto, MergePreview } from "../ipc/types";
import { formatDate, formatDuration } from "../format";
import { PLATFORM_LABEL } from "../labels";
import { StateBadge, Thumbnail } from "./ItemCard";
import { Modal } from "./Modal";

const REASON: Record<string, string> = {
  similar_url: "The links look alike",
  source_id_conflict: "Same video ID on the platform",
  same_file: "Same file",
  similar_title: "Similar title",
};

/** Duplicate candidates (ORG-11, §11.7). Nothing merges without a preview. */
export function DuplicatesView({
  client,
  reloadKey,
  onMerge,
  onChanged,
}: {
  client: LibraryClient;
  reloadKey: number;
  onMerge: (candidateId: string, target: LibraryItemDto) => void;
  onChanged: (msg: string) => void;
}) {
  const [state, setState] = useState<{ status: "loading" } | { status: "error"; code: string } | { status: "ready"; list: DuplicateDto[] }>({ status: "loading" });
  useEffect(() => {
    let live = true;
    client.listDuplicates().then(
      (list) => live && setState({ status: "ready", list }),
      (e) => live && setState({ status: "error", code: errorCode(e) }),
    );
    return () => {
      live = false;
    };
  }, [client, reloadKey]);
  if (state.status === "loading") return <p role="status">Loading…</p>;
  if (state.status === "error") return <p role="alert">Could not load duplicates ({state.code}).</p>;
  if (state.list.length === 0)
    return (
      <div className="results-state" data-testid="duplicates-empty">
        <h2>No possible duplicates</h2>
        <p>When two items look like the same video, they are listed here so you can merge them or keep both.</p>
      </div>
    );
  return (
    <ul className="dup-list" data-testid="duplicates">
      {state.list.map((d) => (
        <li key={d.id} className="dup" data-testid="duplicate">
          <p className="muted small">{REASON[d.reason] ?? d.reason}</p>
          <div className="dup-pair">
            {[d.a, d.b].map((it) => (
              <div key={it.id} className="dup-item">
                <Thumbnail item={it} />
                <div>
                  <strong className="dup-title">{it.title}</strong>
                  <p className="muted small">
                    {PLATFORM_LABEL[it.platform]} · {formatDuration(it.durationMs)} · added {formatDate(it.addedAt)}
                  </p>
                  <StateBadge state={it.localState} />
                  <div>
                    <button type="button" className="button" onClick={() => onMerge(d.id, it)} data-testid="dup-keep">
                      Keep this one…
                    </button>
                  </div>
                </div>
              </div>
            ))}
          </div>
          <button
            type="button"
            className="link-button"
            onClick={async () => {
              try {
                await client.dismissDuplicate(d.id);
                onChanged("Marked as not duplicates.");
              } catch (e) {
                onChanged(`Could not update (${errorCode(e)}).`);
              }
            }}
            data-testid="dup-dismiss"
          >
            Not duplicates
          </button>
        </li>
      ))}
    </ul>
  );
}

/** Shows exactly what moves before the merge runs (ORG-11). */
export function MergeDialog({
  client,
  candidateId,
  target,
  onMerged,
  onClose,
}: {
  client: LibraryClient;
  candidateId: string;
  target: LibraryItemDto;
  onMerged: (p: MergePreview) => Promise<void>;
  onClose: () => void;
}) {
  const [p, setP] = useState<MergePreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    client.previewMerge(candidateId, target.id).then(setP, (e) => setError(errorCode(e)));
  }, [client, candidateId, target.id]);
  return (
    <Modal title="Merge duplicates" onClose={onClose} testId="merge-dialog" wide>
      {error && <p role="alert">Could not prepare the merge ({error}).</p>}
      {!p && !error && <p role="status">Preparing preview…</p>}
      {p && (
        <div data-testid="merge-preview">
          <p>
            <strong>{p.target.title}</strong> is kept. <strong>{p.source.title}</strong> moves to Recently removed; Undo brings it back.
          </p>
          <ul className="merge-list">
            <li>Notes: {p.notesAfter.trim() ? <span className="prewrap">{p.notesAfter}</span> : <span className="muted">none</span>}</li>
            <li data-testid="merge-tags">Tags added: {p.tagsToAdd.length ? p.tagsToAdd.map((t) => `#${t.name}`).join(", ") : <span className="muted">none</span>}</li>
            <li>Collections added: {p.collectionsToAdd.length ? p.collectionsToAdd.map((c) => c.name).join(", ") : <span className="muted">none</span>}</li>
            <li>Media moved: {p.mediaToMove.length ? p.mediaToMove.join(", ") : <span className="muted">none</span>}</li>
            {p.aliasUrl && (
              <li>
                The other link stays as a second address: <span className="url">{p.aliasUrl}</span>
              </li>
            )}
            {p.becomesFavorite && <li>Becomes a favorite.</li>}
          </ul>
        </div>
      )}
      <div className="modal-actions">
        <button type="button" className="button" onClick={onClose}>
          Cancel
        </button>
        <button
          type="button"
          className="button button-primary"
          disabled={!p || busy}
          onClick={async () => {
            if (!p) return;
            setBusy(true);
            try {
              await onMerged(p);
            } catch (e) {
              setError(errorCode(e));
              setBusy(false);
            }
          }}
          data-testid="merge-confirm"
        >
          Merge
        </button>
      </div>
    </Modal>
  );
}
