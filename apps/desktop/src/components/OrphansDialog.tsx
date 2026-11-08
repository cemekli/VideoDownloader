import { useEffect, useState } from "react";
import { errorCode } from "../ipc/errors";
import type { LibraryClient, OrphanDto } from "../ipc/types";
import { formatBytes, formatDate } from "../format";
import { Modal } from "./Modal";

/**
 * Orphaned media (§7.3, AC-53): downloaded files whose item was deleted
 * for good while the user chose to keep the media.
 */
export function OrphansDialog({ client, onChanged, onClose }: { client: LibraryClient; onChanged: (msg: string) => void; onClose: () => void }) {
  const [list, setList] = useState<OrphanDto[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const load = () => client.listOrphans().then(setList, (e) => setError(errorCode(e)));
  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client]);
  const act = async (f: () => Promise<string>) => {
    try {
      onChanged(await f());
      await load();
    } catch (e) {
      setError(errorCode(e));
    }
  };
  return (
    <Modal title="Orphaned media" onClose={onClose} testId="orphans-dialog" wide>
      <p className="muted small">These files were kept when their items were deleted. Re-import makes a new local item; the file stays where it is.</p>
      {error && <p role="alert">Something went wrong ({error}).</p>}
      {list?.length === 0 && <p data-testid="orphans-empty">No orphaned media.</p>}
      <ul className="orphan-list">
        {list?.map((o) => (
          <li key={o.id} className="orphan" data-testid="orphan">
            <div>
              <strong>{o.fileName}</strong>
              <p className="muted small">
                {o.formerTitle ? `Was “${o.formerTitle}” · ` : ""}
                {o.rootLabel} · {formatBytes(o.sizeBytes)} · {formatDate(o.orphanedAt)}
              </p>
            </div>
            {confirmDelete === o.id ? (
              <span className="actions">
                <span className="small">Delete the file from disk?</span>
                <button type="button" className="button" onClick={() => setConfirmDelete(null)}>
                  Keep
                </button>
                <button
                  type="button"
                  className="button button-danger"
                  onClick={() =>
                    act(async () => {
                      const r = await client.deleteOrphan(o.id);
                      setConfirmDelete(null);
                      return r.failed ? "The file could not be deleted; its drive may be away." : `Deleted ${o.fileName}.`;
                    })
                  }
                  data-testid="orphan-delete-confirm"
                >
                  Delete file
                </button>
              </span>
            ) : (
              <span className="actions">
                <button type="button" className="button" onClick={() => act(async () => `Re-imported “${(await client.reimportOrphan(o.id)).title}”.`)} data-testid="orphan-reimport">
                  Re-import
                </button>
                <button type="button" className="button" disabled={o.rootStatus === "disconnected"} onClick={() => setConfirmDelete(o.id)} data-testid="orphan-delete">
                  Delete file…
                </button>
              </span>
            )}
          </li>
        ))}
      </ul>
      <div className="modal-actions">
        <button type="button" className="button" onClick={onClose}>
          Close
        </button>
      </div>
    </Modal>
  );
}
