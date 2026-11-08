import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AddUrlDialog } from "./components/AddUrlDialog";
import { BackendStatus } from "./components/BackendStatus";
import { FilterBar } from "./components/FilterBar";
import { Inspector } from "./components/Inspector";
import { Results } from "./components/Results";
import { Sidebar } from "./components/Sidebar";
import { TopBar, type SortChoice } from "./components/TopBar";
import { useDebounced, useMediaQuery, useOnline } from "./hooks/useEnv";
import { useLibrary } from "./hooks/useLibrary";
import { usePrefs } from "./hooks/usePrefs";
import { createClient } from "./ipc/client";
import { errorCode } from "./ipc/errors";
import { looksLikeUrl, withScheme } from "./url";
import type { BackendClient, ImportSource, ItemKind, LibraryItemDto, LibraryView, ListQuery, Platform } from "./ipc/types";
import { importErrorMessage, importMessage, PLATFORM_LABEL, VIEW_LABEL } from "./labels";
import { EMPTY_SELECTION, type Selection } from "./selection";

/** Below this width the inspector becomes a drawer and navigation hides (§9.2). */
export const COMPACT_QUERY = "(max-width: 899px)";

function sortOf(choice: SortChoice, hasText: boolean): Pick<ListQuery, "sort" | "descending"> {
  switch (choice) {
    case "oldest":
      return { sort: "added", descending: false };
    case "title":
      return { sort: "title", descending: false };
    case "duration":
      return { sort: "duration", descending: true };
    case "published":
      return { sort: "published", descending: true };
    default:
      return hasText ? { sort: "relevance", descending: true } : { sort: "added", descending: true };
  }
}

interface Toast {
  id: number;
  text: string;
  tone: "info" | "error";
}

export function App({ client }: { client?: BackendClient }) {
  const c = useMemo(() => client ?? createClient(), [client]);
  const [prefs, setPrefs] = usePrefs();
  const compact = useMediaQuery(COMPACT_QUERY);
  const online = useOnline();

  const [view, setView] = useState<LibraryView>("all");
  const [text, setText] = useState("");
  const [platforms, setPlatforms] = useState<Platform[]>([]);
  const [kinds, setKinds] = useState<ItemKind[]>([]);
  const [sort, setSort] = useState<SortChoice>("auto");
  const [reloadKey, setReloadKey] = useState(0);
  const [selection, setSelection] = useState<Selection>(EMPTY_SELECTION);
  const [navOpen, setNavOpen] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [importing, setImporting] = useState(false);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const searchRef = useRef<HTMLInputElement>(null);
  const lastFocus = useRef<HTMLElement | null>(null);
  /** Add URL dialog: the text it opens with, or null when closed. */
  const [addUrl, setAddUrl] = useState<string | null>(null);
  const addUrlOpener = useRef<HTMLElement | null>(null);
  /** Item to select (and maybe inspect) once the list shows it. */
  const [reveal, setReveal] = useState<{ id: string; inspect: boolean } | null>(null);

  // A link typed or pasted into search is offered to Add URL instead of
  // being searched for (§9.4 Ctrl+K capture).
  const urlInSearch = looksLikeUrl(text);
  const debounced = useDebounced(text, 150);
  const searchText = looksLikeUrl(debounced) ? "" : debounced;
  const query = useMemo(
    () => ({ text: searchText, view, platforms, kinds, ...sortOf(sort, searchText.trim() !== "") }),
    [searchText, view, platforms, kinds, sort],
  );
  const { data, loadMore } = useLibrary(c, query, reloadKey);
  // An empty result with a search or filter is "no match", not "empty library".
  const narrowed = searchText.trim() !== "" || platforms.length > 0 || kinds.length > 0;

  // A new query starts with nothing selected.
  useEffect(() => setSelection(EMPTY_SELECTION), [query]);

  const items = data.status === "ready" ? data.items : [];
  const activeId = selection.ids.size === 1 ? [...selection.ids][0]! : null;
  const inspectorVisible = compact ? drawerOpen : prefs.inspectorOpen;

  const toast = useCallback((text: string, tone: Toast["tone"] = "info") => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t.slice(-2), { id, text, tone }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 8000);
  }, []);

  // Imports started by a drop on the window (backend event).
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let live = true;
    c.onImportStatus((s) => {
      if (s.phase === "started") setImporting(true);
      else {
        setImporting(false);
        if (s.phase === "finished") toast(importMessage(s.summary));
        else toast(importErrorMessage(s.error.code), "error");
        setReloadKey((k) => k + 1);
      }
    })
      .then((u) => (live ? (unlisten = u) : u()))
      .catch(() => {});
    return () => {
      live = false;
      unlisten?.();
    };
  }, [c, toast]);

  const runImport = async (source: ImportSource) => {
    setImporting(true);
    try {
      const s = await c.importMedia(source);
      if (s.picked) {
        toast(importMessage(s));
        setReloadKey((k) => k + 1);
      }
    } catch (e) {
      toast(importErrorMessage(errorCode(e)), "error");
    } finally {
      setImporting(false);
    }
  };

  const openAddUrl = useCallback((initial = "") => {
    addUrlOpener.current = document.activeElement as HTMLElement | null;
    setAddUrl(initial);
  }, []);

  const closeAddUrl = () => {
    setAddUrl(null);
    const back = addUrlOpener.current;
    addUrlOpener.current = null;
    if (back?.isConnected && back !== document.body) back.focus();
  };

  /** After Save / Open / Restore: show the item in All Items, selected (§8.1 step 4). */
  const showItem = (item: LibraryItemDto, message: string, inspect: boolean) => {
    setAddUrl(null);
    addUrlOpener.current = null;
    setView("all");
    setText("");
    setPlatforms([]);
    setKinds([]);
    setReveal({ id: item.id, inspect });
    setReloadKey((k) => k + 1);
    toast(message);
  };

  // Ctrl+V outside a text field: a pasted link opens Add URL (CAP-01). The
  // clipboard is read only from this paste event, never in the background.
  useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      if (addUrl !== null) return;
      const t = e.target as HTMLElement | null;
      if (t?.closest("input, textarea, select, [contenteditable]")) return;
      const pasted = e.clipboardData?.getData("text/plain") ?? "";
      if (!looksLikeUrl(pasted)) return;
      e.preventDefault();
      openAddUrl(withScheme(pasted));
    };
    document.addEventListener("paste", onPaste);
    return () => document.removeEventListener("paste", onPaste);
  }, [addUrl, openAddUrl]);

  const openInspector = (index: number) => {
    const item = items[index];
    if (!item) return;
    setSelection((s) => (s.ids.size === 1 && s.ids.has(item.id) ? s : { ids: new Set([item.id]), anchor: index, focus: index }));
    lastFocus.current = document.activeElement as HTMLElement | null;
    if (compact) setDrawerOpen(true);
    else setPrefs({ inspectorOpen: true });
  };

  useEffect(() => {
    if (!reveal || data.status !== "ready") return;
    const index = data.items.findIndex((i) => i.id === reveal.id);
    if (index < 0) return;
    setReveal(null);
    setSelection({ ids: new Set([reveal.id]), anchor: index, focus: index });
    if (reveal.inspect) {
      if (compact) setDrawerOpen(true);
      else setPrefs({ inspectorOpen: true });
    }
    requestAnimationFrame(() => document.querySelector<HTMLElement>(`[data-testid="library-card"][data-id="${reveal.id}"]`)?.focus());
  }, [reveal, data, compact, setPrefs]);

  const closeInspector = useCallback(() => {
    if (compact) setDrawerOpen(false);
    else setPrefs({ inspectorOpen: false });
    const back = lastFocus.current;
    lastFocus.current = null;
    if (back?.isConnected) back.focus();
    else document.querySelector<HTMLElement>('[data-testid="library-card"][tabindex="0"]')?.focus();
  }, [compact, setPrefs]);

  // Global keys (§9.4): Ctrl+K focuses search; Esc closes the topmost surface.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
        return;
      }
      if (addUrl !== null) return; // the dialog handles its own keys
      if (e.key !== "Escape" || e.defaultPrevented) return;
      if (navOpen) setNavOpen(false);
      else if (inspectorVisible && (compact || document.activeElement?.closest('[data-testid="inspector"]'))) closeInspector();
      else if (document.activeElement === searchRef.current && text) setText("");
      else if (selection.ids.size > 1) setSelection((s) => ({ ...s, ids: new Set(s.focus !== null && items[s.focus] ? [items[s.focus]!.id] : []) }));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [navOpen, inspectorVisible, compact, closeInspector, text, selection.ids.size, items, addUrl]);

  const goView = (v: LibraryView) => {
    setView(v);
    setPlatforms([]);
    setNavOpen(false);
  };
  const goPlatform = (p: Platform | null) => {
    setView("all");
    setPlatforms(p ? [p] : []);
    setNavOpen(false);
  };
  const clearFilters = () => {
    setPlatforms([]);
    setKinds([]);
  };

  const heading = platforms.length === 1 && view === "all" ? PLATFORM_LABEL[platforms[0]!] : VIEW_LABEL[view];
  const total = data.status === "ready" ? data.total : null;

  const importButtons = (
    <div className="actions">
      <button type="button" className="button" disabled={importing} onClick={() => runImport("files")} data-testid="empty-import-files">
        Import files…
      </button>
      <button type="button" className="button" disabled={importing} onClick={() => runImport("folder")} data-testid="empty-import-folder">
        Import a folder…
      </button>
    </div>
  );

  const emptyState =
    view === "all" ? (
      <div className="results-state" data-testid="library-empty">
        <h2>Your library is empty</h2>
        <p>Save a video link as a bookmark, or import videos from this computer. Imported files stay where they are; VideoVault only keeps a reference. You can also paste a link with Ctrl+V or drop files on this window.</p>
        <div className="actions">
          <button type="button" className="button button-primary" onClick={() => openAddUrl()} data-testid="empty-add-url">
            Add URL…
          </button>
        </div>
        {importButtons}
      </div>
    ) : (
      <div className="results-state" data-testid="view-empty">
        <h2>Nothing in {VIEW_LABEL[view]} yet</h2>
        <p>
          {view === "favorites"
            ? "Items you mark as favorite appear here."
            : view === "recent"
              ? "Items added in the last 30 days appear here."
              : "Archived items are kept here, out of the main list."}
        </p>
        <button type="button" className="button" onClick={() => goView("all")}>
          Show all items
        </button>
      </div>
    );

  const noMatch = (
    <div className="results-state" data-testid="library-no-match">
      <h2>{searchText.trim() ? `No results for “${searchText.trim()}”` : "No items match these filters"}</h2>
      <p>Try fewer words or remove a filter.</p>
      <div className="actions">
        {searchText.trim() && (
          <button type="button" className="button" onClick={() => setText("")}>
            Clear search
          </button>
        )}
        {(platforms.length > 0 || kinds.length > 0) && (
          <button type="button" className="button" onClick={clearFilters}>
            Clear filters
          </button>
        )}
      </div>
    </div>
  );

  return (
    <div className={`app${prefs.inspectorOpen && !compact ? " with-inspector" : ""}`} data-compact={compact}>
      <TopBar
        ref={searchRef}
        text={text}
        onText={setText}
        sort={sort}
        onSort={setSort}
        layout={prefs.layout}
        onLayout={(layout) => setPrefs({ layout })}
        inspectorOpen={inspectorVisible}
        onToggleInspector={() => (inspectorVisible ? closeInspector() : compact ? setDrawerOpen(true) : setPrefs({ inspectorOpen: true }))}
        importing={importing}
        onImport={runImport}
        onAddUrl={() => openAddUrl()}
        urlSuggestion={urlInSearch ? text.trim() : null}
        onAcceptSuggestion={() => {
          const url = withScheme(text);
          setText("");
          openAddUrl(url);
          addUrlOpener.current = searchRef.current;
        }}
        showMenuButton={compact}
        onMenu={() => setNavOpen(true)}
        status={<BackendStatus client={c} />}
      />
      <div className="workspace">
        {(!compact || navOpen) && (
          <>
            {compact && <div className="drawer-backdrop" onClick={() => setNavOpen(false)} aria-hidden="true" />}
            <div className={compact ? "nav-drawer" : "nav-column"}>
              <Sidebar view={view} platform={platforms.length === 1 ? platforms[0]! : null} onView={goView} onPlatform={goPlatform} />
            </div>
          </>
        )}
        <main className="content" aria-labelledby="library-heading">
          {!online && (
            <div className="banner" role="status" data-testid="offline-banner">
              You are offline. Your library, search and imports keep working; online features wait until you reconnect.
            </div>
          )}
          <div className="content-header">
            <h1 id="library-heading">{heading}</h1>
            <span className="count" data-testid="result-count" aria-live="polite">
              {total === null ? "" : `${total} ${total === 1 ? "item" : "items"}`}
              {selection.ids.size > 1 ? ` · ${selection.ids.size} selected` : ""}
            </span>
            {selection.ids.size > 1 && (
              <button type="button" className="link-button" onClick={() => setSelection(EMPTY_SELECTION)} data-testid="clear-selection">
                Clear selection
              </button>
            )}
          </div>
          <FilterBar
            kinds={kinds}
            platforms={platforms}
            onToggleKind={(k) => setKinds((ks) => (ks.includes(k) ? ks.filter((x) => x !== k) : [...ks, k]))}
            onRemovePlatform={(p) => setPlatforms((ps) => ps.filter((x) => x !== p))}
            onClear={clearFilters}
          />
          {data.status === "ready" && data.notice === "no_searchable_terms" && (
            <p className="muted" data-testid="search-notice">
              Nothing to search for in “{searchText.trim()}”; showing the whole list.
            </p>
          )}
          <Results
            data={data}
            layout={prefs.layout}
            selection={selection}
            onSelection={setSelection}
            onOpen={openInspector}
            onLoadMore={loadMore}
            onRetry={() => setReloadKey((k) => k + 1)}
            emptyState={emptyState}
            noMatchState={noMatch}
            narrowed={narrowed}
          />
        </main>
        {inspectorVisible && <Inspector client={c} itemId={activeId} selectedCount={selection.ids.size} drawer={compact} onClose={closeInspector} />}
      </div>
      {addUrl !== null && (
        <AddUrlDialog
          client={c}
          initialUrl={addUrl}
          online={online}
          onClose={closeAddUrl}
          onSaved={(r) => showItem(r.item, `Saved “${r.item.title}” to your library.`, false)}
          onOpenExisting={(item) => showItem(item, `“${item.title}” is already in your library.`, true)}
          onRestored={(item) => showItem(item, `Restored “${item.title}”.`, true)}
        />
      )}
      <div className="toasts" role="status" aria-live="polite" data-testid="toasts">
        {toasts.map((t) => (
          <div key={t.id} className={`toast toast-${t.tone}`} data-testid="toast">
            {t.text}
            <button type="button" className="icon-button" aria-label="Dismiss" onClick={() => setToasts((all) => all.filter((x) => x.id !== t.id))}>
              ×
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
