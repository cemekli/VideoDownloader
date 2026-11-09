import { useEffect, useState } from "react";
import type { LibraryItemDto, Snippet } from "../ipc/types";
import { formatDate, formatDuration } from "../format";
import { LOCAL_STATE, PLATFORM_LABEL } from "../labels";

export function StateBadge({ state }: { state: LibraryItemDto["localState"] }) {
  const s = LOCAL_STATE[state];
  return (
    <span className={`badge badge-${state}`} data-testid="local-state" title={s.hint}>
      <span aria-hidden="true" className="badge-icon">
        {s.icon}
      </span>
      {s.label}
    </span>
  );
}

/** Snippet parts rendered as text; `<mark>` only for matched parts (SEA-03). */
export function SnippetText({ snippet }: { snippet: Snippet }) {
  return (
    <p className="snippet" data-testid="snippet" data-field={snippet.field}>
      {snippet.cutStart && "…"}
      {snippet.parts.map((p, i) => (p.highlight ? <mark key={i}>{p.text}</mark> : <span key={i}>{p.text}</span>))}
      {snippet.cutEnd && "…"}
    </p>
  );
}

/** Loads per thumbnail before the placeholder stays; retries wait 0.5, 1, 2 s. */
const THUMB_TRIES = 4;

export function Thumbnail({ item, large = false }: { item: LibraryItemDto; large?: boolean }) {
  // A failed load is tried again a few times: on Windows the first loads
  // after start-up could fail although the file was there, and one failure
  // used to leave the placeholder until the card was mounted again.
  const [state, setState] = useState({ url: item.thumbnailUrl, tries: 0, waiting: false });
  const cur = state.url === item.thumbnailUrl ? state : { url: item.thumbnailUrl, tries: 0, waiting: false };
  useEffect(() => {
    if (!cur.waiting) return;
    const t = setTimeout(() => setState({ url: cur.url, tries: cur.tries, waiting: false }), 250 * 2 ** cur.tries);
    return () => clearTimeout(t);
  }, [cur.url, cur.tries, cur.waiting]);
  const url = !cur.url || cur.waiting || cur.tries >= THUMB_TRIES ? null : cur.tries === 0 ? cur.url : `${cur.url}?try=${cur.tries}`;
  const onError = () => setState({ url: cur.url, tries: cur.tries + 1, waiting: cur.tries + 1 < THUMB_TRIES });
  return (
    <div className={`thumb${large ? " thumb-large" : ""}`} data-thumb={url ? "ready" : "missing"}>
      {url ? (
        <img key={url} src={url} alt="" loading="lazy" decoding="async" onError={onError} draggable={false} />
      ) : (
        <span className="thumb-placeholder" aria-hidden="true">
          {PLATFORM_LABEL[item.platform].slice(0, 2)}
        </span>
      )}
      {!large && (
        <span className="thumb-duration" data-testid="duration">
          {formatDuration(item.durationMs)}
        </span>
      )}
    </div>
  );
}

interface CardProps {
  item: LibraryItemDto;
  layout: "grid" | "list";
  index: number;
  total: number;
  selected: boolean;
  focused: boolean;
  style: React.CSSProperties;
  onClick: (e: React.MouseEvent) => void;
  onDoubleClick: () => void;
}

/** One library entry; an `option` inside the results listbox. */
export function ItemCard({ item, layout, index, total, selected, focused, style, onClick, onDoubleClick }: CardProps) {
  return (
    <div
      role="option"
      id={`item-${item.id}`}
      aria-selected={selected}
      aria-posinset={index + 1}
      aria-setsize={total}
      tabIndex={focused ? 0 : -1}
      className={`card card-${layout}`}
      data-testid="library-card"
      data-id={item.id}
      data-local-state={item.localState}
      style={style}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
    >
      <Thumbnail item={item} />
      <div className="card-body">
        <h3 className="card-title" title={item.title}>
          {item.snippet?.field === "title" ? item.snippet.parts.map((p, i) => (p.highlight ? <mark key={i}>{p.text}</mark> : <span key={i}>{p.text}</span>)) : item.title}
        </h3>
        {item.snippet && item.snippet.field !== "title" && <SnippetText snippet={item.snippet} />}
        <div className="card-meta">
          <span className="platform">{PLATFORM_LABEL[item.platform]}</span>
          <StateBadge state={item.localState} />
          {item.isFavorite && (
            <span className="favorite" title="Favorite">
              <span aria-hidden="true">★</span>
              <span className="sr-only">Favorite</span>
            </span>
          )}
          {layout === "list" && <span className="added">{formatDate(item.addedAt)}</span>}
        </div>
      </div>
    </div>
  );
}
