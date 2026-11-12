import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AddUrlDialog } from "./components/AddUrlDialog";
import { BackendStatus } from "./components/BackendStatus";
import { BulkBar } from "./components/BulkBar";
import { DownloadDialog } from "./components/DownloadDialog";
import { DownloadsView, QueueIndicator } from "./components/DownloadsView";
import { isOpen, useJobs } from "./hooks/useJobs";
import { DuplicatesView } from "./components/DuplicatesView";
import { FilterBar } from "./components/FilterBar";
import { Inspector } from "./components/Inspector";
import { Onboarding } from "./components/Onboarding";
import { Player } from "./components/Player";
import { SessionsView } from "./components/Sessions";
import { SettingsDialog } from "./components/SettingsDialog";
import { Results } from "./components/Results";
import { ResizeHandle } from "./components/ResizeHandle";
import { Sidebar, type Scope } from "./components/Sidebar";
import { TopBar, type SortChoice } from "./components/TopBar";
import { useDebounced, useMediaQuery, useOnline } from "./hooks/useEnv";
import { useLibrary } from "./hooks/useLibrary";
import { useOrganize } from "./hooks/useOrganize";
import { usePrefs } from "./hooks/usePrefs";
import { useSettings } from "./hooks/useSettings";
import { t } from "./i18n";
import { createClient } from "./ipc/client";
import { errorCode } from "./ipc/errors";
import { looksLikeUrl, withScheme } from "./url";
import type { BackendClient, ImportSource, ItemKind, LibraryItemDto, LibraryView, ListQuery, Platform } from "./ipc/types";
import { errorMessage, importErrorMessage, importMessage, PLATFORM_LABEL, VIEW_LABEL } from "./labels";
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
  /** Offers Undo (the operation is the newest undoable one). */
  undo?: boolean;
}

/** Ctrl+Z / Delete act on the list only, never inside a text field. */
function inTextField(t: EventTarget | null): boolean {
  return !!(t as HTMLElement | null)?.closest?.("input, textarea, select, [contenteditable]");
}

export function App({ client }: { client?: BackendClient }) {
  const c = useMemo(() => client ?? createClient(), [client]);
  const [prefs, setPrefs] = usePrefs();
  const compact = useMediaQuery(COMPACT_QUERY);
  const online = useOnline();
  const { settings, update: updateSettings, replace: replaceSettings } = useSettings(c);
  const offlineMode = !!settings?.offlineMode;
  /** Item open in the player (F2.4), and whether Settings is open. */
  // The item in the player, and where to start (a search moment, F6.4).
  const [playing, setPlayingState] = useState<{ id: string; atMs: number | null } | null>(null);
  const setPlaying = (id: string | null) => setPlayingState(id ? { id, atMs: null } : null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  /** Item whose Download dialog is open (F3.8). */
  const [downloading, setDownloading] = useState<{ id: string; title: string } | null>(null);

  const [view, setView] = useState<LibraryView>("all");
  const [scope, setScope] = useState<Scope>({ kind: "none" });
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
    () => ({
      text: searchText,
      view,
      platforms,
      kinds,
      tagIds: scope.kind === "tag" ? [scope.id] : [],
      collectionId: scope.kind === "collection" ? scope.id : null,
      smartCollectionId: scope.kind === "smart" ? scope.id : null,
      ...sortOf(sort, searchText.trim() !== ""),
    }),
    [searchText, view, platforms, kinds, sort, scope],
  );

  const toast = useCallback((text: string, tone: Toast["tone"] = "info", undo = false) => {
    const id = Date.now() + Math.random();
    // A newer undoable operation replaces the older one (§7.3).
    setToasts((t) => [...t.slice(-2).map((x) => (undo ? { ...x, undo: false } : x)), { id, text, tone, undo }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), undo ? 12000 : 8000);
  }, []);
  const notify = useCallback((text: string, o?: { tone?: Toast["tone"]; undo?: boolean }) => toast(text, o?.tone ?? "info", !!o?.undo), [toast]);
  const jobs = useJobs(c);
  const org = useOrganize({ client: c, notify, scope, onScope: (s) => setScope(s) });

  const { data, loadMore } = useLibrary(c, query, reloadKey + org.version * 1000);
  // An empty result with a search or filter is "no match", not "empty library".
  const narrowed = searchText.trim() !== "" || platforms.length > 0 || kinds.length > 0;
  const collectionName = scope.kind === "collection" ? (org.sidebar?.collections.find((x) => x.id === scope.id)?.name ?? "") : null;

  // A new query starts with nothing selected.
  useEffect(() => setSelection(EMPTY_SELECTION), [query]);

  const items = data.status === "ready" ? data.items : [];
  const activeId = selection.ids.size === 1 ? [...selection.ids][0]! : null;
  const inspectorVisible = compact ? drawerOpen : prefs.inspectorOpen;

  // Items an operation took out of this list leave the selection too.
  useEffect(() => {
    if (data.status !== "ready") return;
    setSelection((s) => {
      const keep = [...s.ids].filter((id) => data.items.some((i) => i.id === id));
      if (keep.length === s.ids.size) return s;
      const last = data.items.length - 1;
      const clamp = (n: number | null) => (n === null || last < 0 ? null : Math.min(n, last));
      return { ids: new Set(keep), anchor: clamp(s.anchor), focus: clamp(s.focus) };
    });
  }, [data]);

  const selectedItems = items.filter((i) => selection.ids.has(i.id));
  const selectedIds = selectedItems.map((i) => i.id);

  // A download or metadata fetch finished: titles, thumbnails and the
  // Downloaded state change, so the list and details load again.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let live = true;
    // Metadata for many bookmarks finishes in bursts: one reload per burst.
    let timer: ReturnType<typeof setTimeout> | null = null;
    c.onJobChanged((e) => {
      if (e.jobId !== "" || !e.itemId || timer) return;
      timer = setTimeout(() => {
        timer = null;
        if (live) setReloadKey((k) => k + 1);
      }, 300);
    }).then(
      (u) => (live ? (unlisten = u) : u()),
      () => {},
    );
    return () => {
      live = false;
      if (timer) clearTimeout(timer);
      unlisten?.();
    };
  }, [c]);

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
        org.refresh();
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

  /** After Save / Open / Restore: show the item, selected. A new bookmark lands in Inbox (§8.1 step 4). */
  const showItem = (item: LibraryItemDto, message: string, inspect: boolean, where: LibraryView = "all") => {
    setAddUrl(null);
    addUrlOpener.current = null;
    setView(where);
    setScope({ kind: "none" });
    org.refresh();
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
      if ((e.ctrlKey || e.metaKey) && e.key === "," && addUrl === null && !org.dialog && !playing) {
        e.preventDefault();
        setSettingsOpen(true);
        return;
      }
      if (addUrl !== null || org.dialog || playing || settingsOpen) return; // dialogs handle their own keys
      // Ctrl+Z: undo the last organize operation (§7.3), outside text fields.
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && e.key.toLowerCase() === "z" && !inTextField(e.target)) {
        e.preventDefault();
        void org.undo();
        return;
      }
      if (e.key === "Delete" && !inTextField(e.target) && selectedIds.length > 0 && view !== "removed" && scope.kind !== "duplicates" && scope.kind !== "downloads" && scope.kind !== "sessions") {
        e.preventDefault();
        void org.actions.remove(selectedIds);
        return;
      }
      if (e.key !== "Escape" || e.defaultPrevented) return;
      if (navOpen) setNavOpen(false);
      else if (inspectorVisible && (compact || document.activeElement?.closest('[data-testid="inspector"]'))) closeInspector();
      else if (document.activeElement === searchRef.current && text) setText("");
      else if (selection.ids.size > 1) setSelection((s) => ({ ...s, ids: new Set(s.focus !== null && items[s.focus] ? [items[s.focus]!.id] : []) }));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [navOpen, inspectorVisible, compact, closeInspector, text, selection.ids.size, items, addUrl, org, selectedIds, view, scope.kind, playing, settingsOpen]);

  const openSource = (id: string) =>
    c.openSource(id).then(
      () => toast(t("player.opened_source")),
      (e) => toast(errorMessage(errorCode(e)), "error"),
    );

  // A search moment is checked again before the player seeks (§16.2): a
  // transcript made again since the search has other segments.
  const openMoment = (segmentId: string) =>
    c.getMoment(segmentId).then(
      (m) => setPlayingState({ id: m.itemId, atMs: m.startMs }),
      (e) => {
        if (errorCode(e) === "INPUT_REJECTED") {
          toast(t("search.moment_gone"), "error");
          setReloadKey((k) => k + 1);
        } else toast(errorMessage(errorCode(e)), "error");
      },
    );

  const goView = (v: LibraryView) => {
    setView(v);
    setScope({ kind: "none" });
    setPlatforms([]);
    setNavOpen(false);
  };
  const goScope = (s: Scope) => {
    setScope(s);
    setView("all");
    setPlatforms([]);
    setNavOpen(false);
  };
  const goPlatform = (p: Platform | null) => {
    setScope({ kind: "none" });
    setView("all");
    setPlatforms(p ? [p] : []);
    setNavOpen(false);
  };
  const clearFilters = () => {
    setPlatforms([]);
    setKinds([]);
  };

  const sb = org.sidebar;
  const heading =
    scope.kind === "collection"
      ? (collectionName ?? "Collection")
      : scope.kind === "smart"
        ? (sb?.smartCollections.find((x) => x.id === scope.id)?.name ?? "Smart collection")
        : scope.kind === "tag"
          ? `#${sb?.tags.find((x) => x.id === scope.id)?.name ?? ""}`
          : scope.kind === "duplicates"
            ? "Duplicates"
            : scope.kind === "downloads"
              ? t("downloads.title")
            : scope.kind === "sessions"
              ? t("sessions.title")
            : platforms.length === 1 && view === "all"
              ? PLATFORM_LABEL[platforms[0]!]
              : VIEW_LABEL[view];
  const smartInvalid = scope.kind === "smart" && !!sb?.smartCollections.find((x) => x.id === scope.id)?.invalid;
  /** Screens that are not a list of library items. */
  const special = scope.kind === "duplicates" || scope.kind === "downloads" || scope.kind === "sessions";
  const total = data.status === "ready" && !special ? data.total : null;

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

  const VIEW_EMPTY: Partial<Record<LibraryView, string>> = {
    favorites: "Items you mark as favorite appear here.",
    recent: "Items added in the last 30 days appear here.",
    archive: "Archived items are kept here, out of the main list.",
    inbox: "New bookmarks land here until you put them in a collection or archive them.",
    removed: "Removed items wait here for 30 days, then they are deleted. Restore brings an item back with its notes, tags and collections.",
  };
  const emptyState =
    scope.kind !== "none" ? (
      <div className="results-state" data-testid="view-empty">
        <h2>Nothing here yet</h2>
        <p>
          {scope.kind === "collection"
            ? "Select items and use Add to collection to put them here. An item can be in many collections."
            : scope.kind === "tag"
              ? "No items carry this tag now."
              : smartInvalid
                ? "A tag or collection in these rules was deleted. Edit the rules to fix them."
                : "No items match these rules yet."}
        </p>
      </div>
    ) : view === "all" ? (
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
        <h2>{view === "removed" ? "Recently removed is empty" : `Nothing in ${VIEW_LABEL[view]} yet`}</h2>
        <p>{VIEW_EMPTY[view]}</p>
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
        onSettings={() => setSettingsOpen(true)}
        status={<BackendStatus client={c} />}
        queue={<QueueIndicator jobs={jobs} onOpen={() => setScope({ kind: "downloads" })} />}
      />
      <div className="workspace">
        {(!compact || navOpen) && (
          <>
            {compact && <div className="drawer-backdrop" onClick={() => setNavOpen(false)} aria-hidden="true" />}
            <div className={compact ? "nav-drawer" : "nav-column"} style={compact ? undefined : { width: prefs.navWidth }} data-testid="nav-column">
              <Sidebar
                view={view}
                platform={platforms.length === 1 ? platforms[0]! : null}
                scope={scope}
                data={sb}
                onView={goView}
                onPlatform={goPlatform}
                onScope={goScope}
                onAction={(action) => org.open({ kind: "sidebar", action })}
              />
            </div>
            {!compact && <ResizeHandle width={prefs.navWidth} onWidth={(navWidth) => setPrefs({ navWidth })} />}
          </>
        )}
        <main className="content" aria-labelledby="library-heading">
          {offlineMode ? (
            <div className="banner" role="status" data-testid="offline-mode-banner">
              {t("offline.mode")}{" "}
              <button type="button" className="link-button" onClick={() => void updateSettings({ offlineMode: false })} data-testid="offline-mode-off">
                {t("offline.turn_off")}
              </button>
            </div>
          ) : (
            !online && (
              <div className="banner" role="status" data-testid="offline-banner">
                {t("offline.network")}
              </div>
            )
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
            {view === "removed" && scope.kind === "none" && (sb?.counts.orphans ?? 0) > 0 && (
              <button type="button" className="link-button" onClick={() => org.open({ kind: "orphans" })} data-testid="open-orphans">
                Orphaned media ({sb!.counts.orphans})
              </button>
            )}
          </div>
          {smartInvalid && (
            <p className="warning small" role="alert" data-testid="smart-invalid">
              A tag or collection these rules use was deleted, so this list may be incomplete. Edit the rules from its ⋯ menu.
            </p>
          )}
          {!special && (
            <>
              <BulkBar
                selected={selectedItems}
                removedView={view === "removed"}
                collectionName={collectionName}
                onTag={() => org.open({ kind: "tag", ids: selectedIds })}
                onAddToCollection={() => org.open({ kind: "add-to-collection", ids: selectedIds })}
                onRemoveFromCollection={() => scope.kind === "collection" && org.actions.removeFromCollection(selectedIds, scope.id)}
                onFavorite={(v) => org.actions.favorite(selectedIds, v)}
                onArchive={(v) => org.actions.archive(selectedIds, v)}
                onRemove={() => org.actions.remove(selectedIds)}
                onRestore={() => org.actions.restore(selectedIds)}
                onDeleteForGood={() => org.open({ kind: "delete-permanently", ids: selectedIds })}
              />
              <FilterBar
                kinds={kinds}
                platforms={platforms}
                onToggleKind={(k) => setKinds((ks) => (ks.includes(k) ? ks.filter((x) => x !== k) : [...ks, k]))}
                onRemovePlatform={(p) => setPlatforms((ps) => ps.filter((x) => x !== p))}
                onClear={clearFilters}
              />
            </>
          )}
          {data.status === "ready" && data.notice === "no_searchable_terms" && (
            <p className="muted" data-testid="search-notice">
              Nothing to search for in “{searchText.trim()}”; showing the whole list.
            </p>
          )}
          {scope.kind === "sessions" ? (
            <SessionsView client={c} notify={(m, tone) => toast(m, tone)} />
          ) : scope.kind === "downloads" ? (
            <DownloadsView client={c} notify={(m, tone) => toast(m, tone)} />
          ) : scope.kind === "duplicates" ? (
            <DuplicatesView client={c} reloadKey={org.version} onMerge={(candidateId, target) => org.open({ kind: "merge", candidateId, target })} onChanged={(m) => (toast(m), org.refresh())} />
          ) : (
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
          )}
        </main>
        {inspectorVisible && (
          <Inspector
            client={c}
            itemId={view === "removed" || special ? null : activeId}
            selectedCount={selection.ids.size}
            drawer={compact}
            onClose={closeInspector}
            refreshKey={org.version + reloadKey * 1000}
            transcripts={c}
            moments={activeId ? (items.find((i) => i.id === activeId)?.transcript ?? null) : null}
            downloadJob={activeId ? (jobs.find((j) => j.kind === "download" && j.itemId === activeId && (isOpen(j) || j.status === "failed")) ?? null) : null}
            collections={sb?.collections ?? []}
            emptyHint={view === "removed" ? "Items in Recently removed can be restored or deleted from the bar above the list." : undefined}
            actions={{
              favorite: (id, v) => org.actions.favorite([id], v),
              archive: (id, v) => org.actions.archive([id], v),
              tag: (id) => org.open({ kind: "tag", ids: [id] }),
              untag: (id, t) => org.actions.untag([id], t.id, t.name),
              addToCollection: (id) => org.open({ kind: "add-to-collection", ids: [id] }),
              removeFromCollection: (id, cid) => org.actions.removeFromCollection([id], cid),
              play: (id) => setPlaying(id),
              openMoment,
              openSource,
              download: (id, title) => setDownloading({ id, title }),
              openDownloads: () => setScope({ kind: "downloads" }),
            }}
          />
        )}
      </div>
      {addUrl !== null && (
        <AddUrlDialog
          client={c}
          initialUrl={addUrl}
          online={online && !offlineMode}
          onClose={closeAddUrl}
          onSaved={(r) => showItem(r.item, `Saved “${r.item.title}” to your Inbox.`, false, "inbox")}
          onOpenExisting={(item) => showItem(item, `“${item.title}” is already in your library.`, true)}
          onRestored={(item) => showItem(item, `Restored “${item.title}”.`, true)}
        />
      )}
      {org.dialog}
      {playing && <Player client={c} itemId={playing.id} startAtMs={playing.atMs} onClose={() => setPlaying(null)} notify={(m, tone) => toast(m, tone)} />}
      {downloading && (
        <DownloadDialog
          client={c}
          itemId={downloading.id}
          title={downloading.title}
          onClose={() => setDownloading(null)}
          onStarted={(m) => {
            setDownloading(null);
            toast(m);
          }}
        />
      )}
      {settingsOpen && settings && (
        <SettingsDialog
          client={c}
          settings={settings}
          onSettings={updateSettings}
          onClose={() => setSettingsOpen(false)}
          notify={(m, tone) => toast(m, tone)}
          onLibraryChanged={() => {
            setReloadKey((k) => k + 1);
            org.refresh();
          }}
          onOpenSessions={() => {
            setSettingsOpen(false);
            setScope({ kind: "sessions" });
          }}
        />
      )}
      {settings && settings.onboardingCompletedAt === null && (
        <Onboarding client={c} settings={settings} onSettings={updateSettings} onDone={replaceSettings} notify={(m, tone) => toast(m, tone)} />
      )}
      <div className="toasts" role="status" aria-live="polite" data-testid="toasts">
        {toasts.map((t) => (
          <div key={t.id} className={`toast toast-${t.tone}`} data-testid="toast">
            <span>{t.text}</span>
            {t.undo && (
              <button
                type="button"
                className="link-button"
                onClick={() => {
                  setToasts((all) => all.filter((x) => x.id !== t.id));
                  void org.undo();
                }}
                data-testid="toast-undo"
              >
                Undo
              </button>
            )}
            <button type="button" className="icon-button" aria-label="Dismiss" onClick={() => setToasts((all) => all.filter((x) => x.id !== t.id))}>
              ×
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
