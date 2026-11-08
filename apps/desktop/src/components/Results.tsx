import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { LibraryItemDto } from "../ipc/types";
import type { LibraryData } from "../hooks/useLibrary";
import { clickSelect, moveFocus, selectAll, toggleFocused, type Selection } from "../selection";
import { ItemCard } from "./ItemCard";

// Card sizes (SPEC §9.2: 220–280 px minimum card width, 16:9 thumbnails).
export const MIN_CARD = 232;
export const GAP = 12;
const CARD_META = 96;
const CARD_META_SNIPPET = 118;
const LIST_ROW = 64;
const OVERSCAN = 3;

export interface Geometry {
  cols: number;
  cardWidth: number;
  rowHeight: number;
}

/** Columns and row height for a content width (pure, unit-tested). */
export function geometry(width: number, layout: "grid" | "list", withSnippets: boolean): Geometry {
  if (layout === "list") return { cols: 1, cardWidth: width, rowHeight: LIST_ROW };
  const cols = Math.max(1, Math.floor((width + GAP) / (MIN_CARD + GAP)));
  const cardWidth = (width - GAP * (cols - 1)) / cols;
  return { cols, cardWidth, rowHeight: Math.round((cardWidth * 9) / 16 + (withSnippets ? CARD_META_SNIPPET : CARD_META)) + GAP };
}

interface Props {
  data: LibraryData;
  layout: "grid" | "list";
  selection: Selection;
  onSelection: (s: Selection) => void;
  onOpen: (index: number) => void;
  onLoadMore: () => void;
  onRetry: () => void;
  emptyState: React.ReactNode;
  noMatchState: React.ReactNode;
  /** True when a search text or filter is active (empty means "no match", not "first use"). */
  narrowed: boolean;
}

export function Results(props: Props) {
  const { data } = props;
  if (data.status === "loading") {
    return (
      <div className="results-state" data-testid="library-loading">
        <p role="status">Loading library…</p>
        <div className="skeleton-grid" aria-hidden="true">
          {Array.from({ length: 8 }, (_, i) => (
            <div key={i} className="skeleton-card" />
          ))}
        </div>
      </div>
    );
  }
  if (data.status === "error") {
    return (
      <div className="results-state" role="alert" data-testid="library-error">
        <h2>Could not load the library</h2>
        <p>Your items are safe; the library could not be read just now.</p>
        <p className="muted">Error code: {data.code}</p>
        <button type="button" className="button" onClick={props.onRetry}>
          Try again
        </button>
      </div>
    );
  }
  if (data.items.length === 0) return <>{props.narrowed ? props.noMatchState : props.emptyState}</>;
  return <VirtualResults {...props} items={data.items} total={data.total} />;
}

function VirtualResults({ items, total, layout, selection, onSelection, onOpen, onLoadMore }: Props & { items: LibraryItemDto[]; total: number }) {
  const scroller = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  const [scrollTop, setScrollTop] = useState(0);
  const wantFocus = useRef(false);

  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const measure = () => setSize({ w: el.clientWidth, h: el.clientHeight });
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // jsdom and the first frame have no layout; assume a desktop-sized area.
  const width = size.w || 1024;
  const height = size.h || 800;
  const withSnippets = items.some((i) => i.snippet && i.snippet.field !== "title");
  const g = geometry(width, layout, withSnippets);
  const rows = Math.ceil(items.length / g.cols);
  const first = Math.max(0, Math.floor(scrollTop / g.rowHeight) - OVERSCAN);
  const last = Math.min(rows, Math.ceil((scrollTop + height) / g.rowHeight) + OVERSCAN);
  const visible = items.slice(first * g.cols, last * g.cols);

  useEffect(() => {
    if (last * g.cols >= items.length - g.cols * 2 && items.length < total) onLoadMore();
  }, [last, g.cols, items.length, total, onLoadMore]);

  const ids = items.map((i) => i.id);
  const focusIndex = selection.focus !== null && selection.focus < items.length ? selection.focus : 0;

  const reveal = useCallback(
    (index: number) => {
      const el = scroller.current;
      if (!el) return;
      const top = Math.floor(index / g.cols) * g.rowHeight;
      if (top < el.scrollTop) el.scrollTop = top;
      else if (top + g.rowHeight > el.scrollTop + el.clientHeight) el.scrollTop = top + g.rowHeight - el.clientHeight;
      setScrollTop(el.scrollTop);
    },
    [g.cols, g.rowHeight],
  );

  useEffect(() => {
    if (!wantFocus.current) return;
    wantFocus.current = false;
    document.getElementById(`item-${items[focusIndex]?.id}`)?.focus({ preventScroll: true });
  });

  const onKeyDown = (e: React.KeyboardEvent) => {
    const ctrl = e.ctrlKey || e.metaKey;
    const perPage = Math.max(1, Math.floor(height / g.rowHeight)) * g.cols;
    let to: number | null = null;
    switch (e.key) {
      case "ArrowRight":
        if (layout === "grid") to = focusIndex + 1;
        break;
      case "ArrowLeft":
        if (layout === "grid") to = focusIndex - 1;
        break;
      case "ArrowDown":
        to = focusIndex + g.cols;
        break;
      case "ArrowUp":
        to = focusIndex - g.cols;
        break;
      case "Home":
        to = 0;
        break;
      case "End":
        to = items.length - 1;
        break;
      case "PageDown":
        to = focusIndex + perPage;
        break;
      case "PageUp":
        to = focusIndex - perPage;
        break;
      case "Enter":
        e.preventDefault();
        onOpen(focusIndex);
        return;
      case " ":
        if (ctrl) {
          e.preventDefault();
          onSelection(toggleFocused({ ...selection, focus: focusIndex }, ids));
        }
        return;
      case "a":
      case "A":
        if (ctrl) {
          e.preventDefault();
          onSelection(selectAll(ids, selection));
        }
        return;
      default:
        return;
    }
    if (to === null) return;
    e.preventDefault();
    const clamped = Math.max(0, Math.min(items.length - 1, to));
    onSelection(moveFocus({ ...selection, focus: focusIndex }, ids, clamped, { shift: e.shiftKey, ctrl }));
    wantFocus.current = true;
    reveal(clamped);
  };

  return (
    <div
      ref={scroller}
      className={`results results-${layout}`}
      role="listbox"
      aria-label="Library items"
      aria-multiselectable="true"
      data-testid="library-results"
      data-layout={layout}
      data-total={total}
      onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
      onKeyDown={onKeyDown}
    >
      <div className="results-canvas" style={{ height: rows * g.rowHeight }}>
        {visible.map((item, k) => {
          const index = first * g.cols + k;
          const row = Math.floor(index / g.cols);
          const col = index % g.cols;
          return (
            <ItemCard
              key={item.id}
              item={item}
              layout={layout}
              index={index}
              total={total}
              selected={selection.ids.has(item.id)}
              focused={index === focusIndex}
              style={{ transform: `translate(${col * (g.cardWidth + GAP)}px, ${row * g.rowHeight}px)`, width: g.cardWidth, height: g.rowHeight - GAP }}
              onClick={(e) => onSelection(clickSelect(selection, ids, index, { shift: e.shiftKey, toggle: e.ctrlKey || e.metaKey }))}
              onDoubleClick={() => onOpen(index)}
            />
          );
        })}
      </div>
    </div>
  );
}
