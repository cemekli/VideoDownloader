import { useEffect, useRef, useState } from "react";
import type { CollectionDto, ItemDetail, JobDto, LibraryClient, TagRef } from "../ipc/types";
import { ItemDownload } from "./DownloadsView";
import { errorCode } from "../ipc/errors";
import { formatBytes, formatDate, formatDuration } from "../format";
import { KIND_LABEL, LOCAL_STATE, PLATFORM_LABEL, orgErrorMessage } from "../labels";
import { StateBadge, Thumbnail } from "./ItemCard";
import { useOnline } from "../hooks/useEnv";
import { t } from "../i18n";

type State = { status: "idle" } | { status: "loading"; id: string } | { status: "error"; id: string; code: string } | { status: "ready"; detail: ItemDetail };

const KIND_SINGULAR = { bookmark: "Bookmark", remote_video: "Online video", local_video: "Local file" } as const;

const ROOT_STATUS: Record<string, string> = {
  available: "Connected",
  disconnected: "Not connected",
  inaccessible: "No access",
  unknown: "Not checked yet",
};

const INTEGRITY: Record<string, string> = {
  verified: "Found",
  unknown: "Not checked yet",
  missing: "Missing",
  corrupt: "Unreadable",
  modified: "Changed since import",
};

/** Edits from the details panel go through the same actions as the bulk bar. */
export interface InspectorActions {
  favorite(id: string, v: boolean): void;
  archive(id: string, v: boolean): void;
  tag(id: string): void;
  untag(id: string, tag: TagRef): void;
  addToCollection(id: string): void;
  removeFromCollection(id: string, collectionId: string): void;
  /** Opens the player (F2.4). */
  play?(id: string): void;
  /** Opens the source page in the browser (PLAY-03). */
  openSource?(id: string): void;
  /** Opens the Download dialog (F3.8). */
  download?(id: string, title: string): void;
  /** Opens the Downloads screen. */
  openDownloads?(): void;
}

/**
 * Item details beside the results (SPEC §9.2, 300–380 px). Below 900 px it
 * is a drawer: a modal dialog that Esc closes, returning focus (§9.4).
 */
export function Inspector({
  client,
  itemId,
  selectedCount,
  drawer,
  onClose,
  refreshKey = 0,
  collections = [],
  actions,
  emptyHint,
  downloadJob = null,
}: {
  client: LibraryClient;
  itemId: string | null;
  selectedCount: number;
  drawer: boolean;
  onClose: () => void;
  /** Changes after an edit elsewhere: the details load again. */
  refreshKey?: number;
  collections?: CollectionDto[];
  actions?: InspectorActions;
  emptyHint?: string;
  /** The item's latest open or failed download (F3.8). */
  downloadJob?: JobDto | null;
}) {
  const [state, setState] = useState<State>({ status: "idle" });
  const panel = useRef<HTMLElement>(null);

  useEffect(() => {
    if (!itemId) {
      setState({ status: "idle" });
      return;
    }
    let live = true;
    setState((s) => (s.status === "ready" && s.detail.item.id === itemId ? s : { status: "loading", id: itemId }));
    client
      .getItem(itemId)
      .then((detail) => live && setState({ status: "ready", detail }))
      .catch((e: unknown) => live && setState({ status: "error", id: itemId, code: errorCode(e) }));
    return () => {
      live = false;
    };
  }, [client, itemId, refreshKey]);

  useEffect(() => {
    if (drawer) panel.current?.focus();
  }, [drawer]);

  const body = (() => {
    if (selectedCount > 1) return <p className="muted">{selectedCount} items selected.</p>;
    if (state.status === "idle") return <p className="muted">{emptyHint ?? "Select an item to see its details."}</p>;
    if (state.status === "loading") return <p role="status">Loading details…</p>;
    if (state.status === "error")
      return (
        <p role="alert">
          Could not load the details ({state.code}).
        </p>
      );
    return <Details detail={state.detail} client={client} collections={collections} actions={actions} downloadJob={downloadJob} onSaved={(detail) => setState({ status: "ready", detail })} />;
  })();

  return (
    <>
      {drawer && <div className="drawer-backdrop" onClick={onClose} aria-hidden="true" />}
      <aside
        ref={panel}
        className={`inspector${drawer ? " inspector-drawer" : ""}`}
        aria-label="Details"
        role={drawer ? "dialog" : "complementary"}
        aria-modal={drawer ? true : undefined}
        tabIndex={-1}
        data-testid="inspector"
      >
        <div className="inspector-header">
          <h2>Details</h2>
          <button type="button" className="icon-button" onClick={onClose} aria-label="Close details" data-testid="inspector-close">
            ×
          </button>
        </div>
        <div className="inspector-body">{body}</div>
      </aside>
    </>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <>
      <dt>{label}</dt>
      <dd>{children}</dd>
    </>
  );
}

function Details({
  detail,
  client,
  collections,
  actions,
  downloadJob,
  onSaved,
}: {
  detail: ItemDetail;
  client: LibraryClient;
  collections: CollectionDto[];
  actions?: InspectorActions;
  downloadJob: JobDto | null;
  onSaved: (d: ItemDetail) => void;
}) {
  const { item } = detail;
  const online = useOnline();
  const collectionName = (id: string) => collections.find((c) => c.id === id)?.name ?? "…";
  return (
    <div data-testid="inspector-details">
      <Thumbnail item={item} large />
      <h3 className="inspector-title" data-testid="inspector-title">
        {item.title}
      </h3>
      {downloadJob && detail.assets.length === 0 && <ItemDownload job={downloadJob} onOpen={() => actions?.openDownloads?.()} />}
      {actions && (
        <div className="inspector-actions">
          {actions.play && detail.assets.length > 0 && (
            <button type="button" className="button button-primary" onClick={() => actions.play!(item.id)} data-testid="inspector-play">
              ▶ {t("player.play")}
            </button>
          )}
          {actions.download && detail.canonicalUrl && detail.assets.length === 0 && !(downloadJob && downloadJob.status !== "failed" && downloadJob.status !== "canceled") && (
            <button type="button" className="button button-primary" onClick={() => actions.download!(item.id, item.title)} data-testid="inspector-download">
              {t("download.open")}
            </button>
          )}
          {actions.openSource && detail.canonicalUrl && detail.assets.length === 0 && (
            <button type="button" className="button" onClick={() => actions.openSource!(item.id)} data-testid="inspector-open-source">
              {t("player.open_source")}
            </button>
          )}
          <button type="button" className="button button-quiet" aria-pressed={item.isFavorite} onClick={() => actions.favorite(item.id, !item.isFavorite)} data-testid="inspector-favorite">
            {item.isFavorite ? "★ Favorite" : "☆ Favorite"}
          </button>
          <button type="button" className="button button-quiet" aria-pressed={detail.archived} onClick={() => actions.archive(item.id, !detail.archived)} data-testid="inspector-archive">
            {detail.archived ? "Archived" : "Archive"}
          </button>
        </div>
      )}
      <p className="inspector-state">
        <StateBadge state={item.localState} />
        <span className="muted"> {LOCAL_STATE[item.localState].hint}</span>
      </p>
      <dl className="facts">
        <Row label="Type">{KIND_SINGULAR[item.kind] ?? KIND_LABEL[item.kind]}</Row>
        <Row label="Platform">{PLATFORM_LABEL[item.platform]}</Row>
        <Row label="Duration">{formatDuration(item.durationMs)}</Row>
        <Row label="Added">{formatDate(item.addedAt)}</Row>
        {detail.creator && <Row label="Creator">{detail.creator}</Row>}
        {(detail.publishedDate || detail.publishedAt !== null) && <Row label="Published">{detail.publishedDate ?? formatDate(detail.publishedAt)}</Row>}
        {detail.canonicalUrl && (
          <Row label="Source">
            <span className="url" data-testid="inspector-url">
              {detail.canonicalUrl}
            </span>
          </Row>
        )}
        {item.kind !== "local_video" && detail.metadataStatus !== "ready" && <Row label="Metadata">{detail.metadataStatus === "pending" ? (online ? "Not fetched yet" : "Offline") : detail.metadataStatus}</Row>}
      </dl>
      {detail.description && (
        <section>
          <h4>Description</h4>
          <p className="prewrap">{detail.description}</p>
        </section>
      )}
      {actions ? (
        <>
          <section data-testid="inspector-tags">
            <h4>Tags</h4>
            <ul className="chips">
              {detail.tagRefs.map((t) => (
                <li key={t.id} className="tag-chip">
                  #{t.name}
                  <button type="button" className="chip-x" aria-label={`Remove tag ${t.name}`} onClick={() => actions.untag(item.id, t)} data-testid="inspector-untag">
                    ×
                  </button>
                </li>
              ))}
              <li>
                <button type="button" className="link-button" onClick={() => actions.tag(item.id)} data-testid="inspector-add-tag">
                  + Add tag
                </button>
              </li>
            </ul>
          </section>
          <section data-testid="inspector-collections">
            <h4>Collections</h4>
            <ul className="chips">
              {detail.collectionIds.map((cid) => (
                <li key={cid} className="tag-chip">
                  {collectionName(cid)}
                  <button type="button" className="chip-x" aria-label={`Remove from ${collectionName(cid)}`} onClick={() => actions.removeFromCollection(item.id, cid)}>
                    ×
                  </button>
                </li>
              ))}
              <li>
                <button type="button" className="link-button" onClick={() => actions.addToCollection(item.id)} data-testid="inspector-add-collection">
                  + Add to collection
                </button>
              </li>
            </ul>
          </section>
          <Notes key={item.id} detail={detail} client={client} onSaved={onSaved} />
        </>
      ) : (
        detail.notes && (
          <section>
            <h4>Notes</h4>
            <p className="prewrap" data-testid="inspector-notes">
              {detail.notes}
            </p>
          </section>
        )
      )}
      {detail.assets.length > 0 && (
        <section data-testid="inspector-media">
          <h4>Media</h4>
          {detail.assets.map((a) => (
            <dl className="facts" key={a.id}>
              <Row label="File">{a.fileName}</Row>
              <Row label="Folder">
                {a.rootLabel} · {ROOT_STATUS[a.rootStatus] ?? a.rootStatus}
              </Row>
              <Row label="State">{a.rootStatus === "available" ? (INTEGRITY[a.integrityState] ?? a.integrityState) : "Not checked while the drive is away"}</Row>
              {a.width && a.height && (
                <Row label="Video">
                  {a.width}×{a.height}
                  {a.videoCodec ? ` · ${a.videoCodec}` : ""}
                  {a.container ? ` · ${a.container}` : ""}
                </Row>
              )}
              <Row label="Size">{formatBytes(a.sizeBytes)}</Row>
            </dl>
          ))}
        </section>
      )}
    </div>
  );
}

/** The user's note (ORG-03). Saved with the revision it was read at. */
function Notes({ detail, client, onSaved }: { detail: ItemDetail; client: LibraryClient; onSaved: (d: ItemDetail) => void }) {
  const [text, setText] = useState(detail.notes);
  const [status, setStatus] = useState<{ kind: "idle" | "saving" | "saved" } | { kind: "error"; message: string }>({ kind: "idle" });
  // A reload from elsewhere (Undo, merge) replaces an unedited note.
  const base = useRef(detail.notes);
  useEffect(() => {
    if (text === base.current) setText(detail.notes);
    base.current = detail.notes;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [detail.notes]);
  const dirty = text !== detail.notes;
  const save = async () => {
    setStatus({ kind: "saving" });
    try {
      const d = await client.setNotes(detail.item.id, detail.revision, text);
      base.current = d.notes;
      onSaved(d);
      setStatus({ kind: "saved" });
    } catch (e) {
      setStatus({ kind: "error", message: orgErrorMessage(e) });
    }
  };
  return (
    <section>
      <h4>
        <label htmlFor="inspector-notes-input">Notes</label>
      </h4>
      <textarea
        id="inspector-notes-input"
        className="notes-input"
        rows={4}
        maxLength={100_000}
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          setStatus({ kind: "idle" });
        }}
        data-testid="inspector-notes-input"
      />
      <div className="notes-bar">
        <button type="button" className="button" disabled={!dirty || status.kind === "saving"} onClick={save} data-testid="inspector-notes-save">
          Save note
        </button>
        <span className="small muted" role="status">
          {status.kind === "saved" ? "Saved." : status.kind === "error" ? status.message : dirty ? "Not saved yet." : ""}
        </span>
      </div>
    </section>
  );
}
