import { forwardRef, useEffect, useRef, useState } from "react";
import type { ImportSource } from "../ipc/types";
import { t } from "../i18n";

export type SortChoice = "auto" | "oldest" | "title" | "duration" | "published";

/** Top command bar (SPEC §9.2, 56–68 px): search, import, sort, layout, details. */
export const TopBar = forwardRef<
  HTMLInputElement,
  {
    text: string;
    onText: (t: string) => void;
    sort: SortChoice;
    onSort: (s: SortChoice) => void;
    layout: "grid" | "list";
    onLayout: (l: "grid" | "list") => void;
    inspectorOpen: boolean;
    onToggleInspector: () => void;
    importing: boolean;
    onImport: (s: ImportSource) => void;
    onAddUrl: () => void;
    /** Shown under the search box when the text is a link (Ctrl+K capture, §9.4). */
    urlSuggestion: string | null;
    onAcceptSuggestion: () => void;
    showMenuButton: boolean;
    onMenu: () => void;
    onSettings: () => void;
    status: React.ReactNode;
  }
>(function TopBar(p, searchRef) {
  return (
    <header className="topbar" data-testid="topbar">
      {p.showMenuButton && (
        <button type="button" className="icon-button" aria-label="Show navigation" onClick={p.onMenu} data-testid="menu-button">
          ☰
        </button>
      )}
      <span className="brand">VideoVault</span>
      <div className="search">
        <input
          ref={searchRef}
          type="search"
          aria-label="Search library"
          placeholder="Search titles, notes, creators…"
          value={p.text}
          onChange={(e) => p.onText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && p.urlSuggestion) {
              e.preventDefault();
              p.onAcceptSuggestion();
            }
          }}
          aria-describedby={p.urlSuggestion ? "url-suggestion" : undefined}
          data-testid="search-input"
        />
        {p.urlSuggestion && (
          <button type="button" id="url-suggestion" className="search-suggestion" onClick={p.onAcceptSuggestion} data-testid="url-suggestion">
            <span aria-hidden="true">＋</span> Save this link as a bookmark <kbd>Enter</kbd>
          </button>
        )}
      </div>
      <button type="button" className="button button-primary" onClick={p.onAddUrl} data-testid="add-url-button">
        Add URL
      </button>
      <ImportMenu importing={p.importing} onImport={p.onImport} />
      <label className="sort">
        <span className="sr-only">Sort by</span>
        <select value={p.sort} onChange={(e) => p.onSort(e.target.value as SortChoice)} data-testid="sort-select">
          <option value="auto">{p.text.trim() ? "Best match" : "Newest first"}</option>
          <option value="oldest">Oldest first</option>
          <option value="title">Title A–Z</option>
          <option value="duration">Longest first</option>
          <option value="published">Recently published</option>
        </select>
      </label>
      <div className="segmented" role="group" aria-label="Layout">
        <button type="button" aria-pressed={p.layout === "grid"} onClick={() => p.onLayout("grid")} data-testid="layout-grid">
          Grid
        </button>
        <button type="button" aria-pressed={p.layout === "list"} onClick={() => p.onLayout("list")} data-testid="layout-list">
          List
        </button>
      </div>
      <button type="button" className="button button-quiet" aria-pressed={p.inspectorOpen} onClick={p.onToggleInspector} data-testid="toggle-inspector">
        Details
      </button>
      <button type="button" className="icon-button" aria-label={t("settings.open")} title={`${t("settings.open")} (Ctrl+,)`} onClick={p.onSettings} data-testid="settings-button">
        ⚙
      </button>
      {p.status}
    </header>
  );
});

/** "Import" with a two-item menu. The dialog itself is native (backend). */
export function ImportMenu({ importing, onImport }: { importing: boolean; onImport: (s: ImportSource) => void }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const first = useRef<HTMLButtonElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    first.current?.focus();
    const away = (e: MouseEvent) => !root.current?.contains(e.target as Node) && setOpen(false);
    document.addEventListener("mousedown", away);
    return () => document.removeEventListener("mousedown", away);
  }, [open]);

  const pick = (s: ImportSource) => {
    setOpen(false);
    onImport(s);
  };

  return (
    <div className="menu-root" ref={root}>
      <button
        ref={trigger}
        type="button"
        className="button"
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={importing}
        onClick={() => setOpen((o) => !o)}
        data-testid="import-button"
      >
        {importing ? "Importing…" : "Import"}
      </button>
      {open && (
        <div
          className="menu"
          role="menu"
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              e.stopPropagation();
              setOpen(false);
              trigger.current?.focus();
            }
            if (e.key === "ArrowDown" || e.key === "ArrowUp") {
              e.preventDefault();
              const items = [...(root.current?.querySelectorAll<HTMLButtonElement>("[role=menuitem]") ?? [])];
              const i = items.indexOf(document.activeElement as HTMLButtonElement);
              items[(i + (e.key === "ArrowDown" ? 1 : items.length - 1)) % items.length]?.focus();
            }
          }}
        >
          <button ref={first} type="button" role="menuitem" onClick={() => pick("files")} data-testid="import-files">
            Files…
          </button>
          <button type="button" role="menuitem" onClick={() => pick("folder")} data-testid="import-folder">
            Folder…
          </button>
        </div>
      )}
    </div>
  );
}
