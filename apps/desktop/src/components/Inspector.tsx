import { useEffect, useRef, useState } from "react";
import type { ItemDetail, LibraryClient } from "../ipc/types";
import { errorCode } from "../ipc/errors";
import { formatBytes, formatDate, formatDuration } from "../format";
import { KIND_LABEL, LOCAL_STATE, PLATFORM_LABEL } from "../labels";
import { StateBadge, Thumbnail } from "./ItemCard";

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
}: {
  client: LibraryClient;
  itemId: string | null;
  selectedCount: number;
  drawer: boolean;
  onClose: () => void;
}) {
  const [state, setState] = useState<State>({ status: "idle" });
  const panel = useRef<HTMLElement>(null);

  useEffect(() => {
    if (!itemId) {
      setState({ status: "idle" });
      return;
    }
    let live = true;
    setState({ status: "loading", id: itemId });
    client
      .getItem(itemId)
      .then((detail) => live && setState({ status: "ready", detail }))
      .catch((e: unknown) => live && setState({ status: "error", id: itemId, code: errorCode(e) }));
    return () => {
      live = false;
    };
  }, [client, itemId]);

  useEffect(() => {
    if (drawer) panel.current?.focus();
  }, [drawer]);

  const body = (() => {
    if (selectedCount > 1) return <p className="muted">{selectedCount} items selected.</p>;
    if (state.status === "idle") return <p className="muted">Select an item to see its details.</p>;
    if (state.status === "loading") return <p role="status">Loading details…</p>;
    if (state.status === "error")
      return (
        <p role="alert">
          Could not load the details ({state.code}).
        </p>
      );
    return <Details detail={state.detail} />;
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

function Details({ detail }: { detail: ItemDetail }) {
  const { item } = detail;
  return (
    <div data-testid="inspector-details">
      <Thumbnail item={item} large />
      <h3 className="inspector-title" data-testid="inspector-title">
        {item.title}
      </h3>
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
        {item.kind !== "local_video" && detail.metadataStatus !== "ready" && <Row label="Metadata">{detail.metadataStatus === "pending" ? "Not fetched yet" : detail.metadataStatus}</Row>}
      </dl>
      {detail.description && (
        <section>
          <h4>Description</h4>
          <p className="prewrap">{detail.description}</p>
        </section>
      )}
      {detail.notes && (
        <section>
          <h4>Notes</h4>
          <p className="prewrap" data-testid="inspector-notes">
            {detail.notes}
          </p>
        </section>
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
