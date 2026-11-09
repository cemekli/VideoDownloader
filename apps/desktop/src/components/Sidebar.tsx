import { Fragment, useState } from "react";
import type { CollectionDto, LibraryView, Platform, Sidebar as SidebarData, SmartCollectionDto, TagDto } from "../ipc/types";
import { PLATFORMS } from "../ipc/types";
import { PLATFORM_LABEL, VIEW_LABEL } from "../labels";
import { MenuButton } from "./Menu";
import { t } from "../i18n";

/** What the main area shows besides the plain views (STATE.md F2.1). */
export type Scope = { kind: "none" } | { kind: "collection"; id: string } | { kind: "smart"; id: string } | { kind: "tag"; id: string } | { kind: "duplicates" } | { kind: "downloads" };

export type SidebarAction =
  | { kind: "new-collection"; parentId: string | null }
  | { kind: "rename-collection"; collection: CollectionDto }
  | { kind: "move-collection"; collection: CollectionDto }
  | { kind: "delete-collection"; collection: CollectionDto }
  | { kind: "new-smart" }
  | { kind: "edit-smart"; smart: SmartCollectionDto }
  | { kind: "delete-smart"; smart: SmartCollectionDto }
  | { kind: "rename-tag"; tag: TagDto }
  | { kind: "merge-tag"; tag: TagDto }
  | { kind: "delete-tag"; tag: TagDto };

const TOP_VIEWS: LibraryView[] = ["inbox", "all", "favorites", "recent"];

/**
 * Left navigation (SPEC §9.2). Entries for later phases (Sessions, AI)
 * stay hidden until they work (§9.3).
 */
export function Sidebar({
  view,
  platform,
  scope,
  data,
  onView,
  onPlatform,
  onScope,
  onAction,
}: {
  view: LibraryView;
  platform: Platform | null;
  scope: Scope;
  data: SidebarData | null;
  onView: (v: LibraryView) => void;
  onPlatform: (p: Platform | null) => void;
  onScope: (s: Scope) => void;
  onAction: (a: SidebarAction) => void;
}) {
  const [open, setOpen] = useState({
    collections: true,
    smart: true,
    tags: true,
    platforms: true,
  });
  const plain = scope.kind === "none" && platform === null;
  const count = (n: number | undefined) =>
    n ? (
      <span className="nav-count" aria-label={`${n} items`}>
        {n}
      </span>
    ) : null;
  const navButton = (label: string, current: boolean, onClick: () => void, testId: string, extra?: React.ReactNode, depth = 0) => (
    <button
      type="button"
      className="nav-item"
      aria-current={current ? "page" : undefined}
      onClick={onClick}
      data-testid={testId}
      style={depth ? { paddingLeft: `calc(var(--s3) + ${depth * 14}px)` } : undefined}
    >
      <span className="nav-label">{label}</span>
      {extra}
    </button>
  );
  const group = (key: keyof typeof open, title: string, add: React.ReactNode, body: React.ReactNode) => (
    <div className="nav-group">
      <div className="nav-group-head">
        <button type="button" className="nav-group-toggle" aria-expanded={open[key]} onClick={() => setOpen((o) => ({ ...o, [key]: !o[key] }))}>
          {title}
        </button>
        {add}
      </div>
      {open[key] && body}
    </div>
  );
  const is = (kind: Scope["kind"], id?: string) => scope.kind === kind && (id === undefined || ("id" in scope && scope.id === id));

  return (
    <nav className="sidebar" aria-label="Library navigation" data-testid="sidebar">
      <ul className="nav-list">
        {TOP_VIEWS.map((v) => (
          <Fragment key={v}>
            <li>{navButton(VIEW_LABEL[v], plain && view === v, () => onView(v), `nav-${v}`, v === "inbox" ? count(data?.counts.inbox) : null)}</li>
            {/* §9.2 order: Inbox, All Items, Favorites, Downloads, Recently Added. */}
            {v === "favorites" && <li>{navButton(t("downloads.title"), is("downloads"), () => onScope({ kind: "downloads" }), "nav-downloads")}</li>}
          </Fragment>
        ))}
      </ul>

      {group(
        "collections",
        "Collections",
        <button type="button" className="icon-button nav-add" aria-label="New collection" onClick={() => onAction({ kind: "new-collection", parentId: null })} data-testid="new-collection">
          +
        </button>,
        <ul className="nav-list" data-testid="collection-tree">
          {data?.collections.length === 0 && <li className="muted small nav-empty">No collections yet.</li>}
          {data?.collections.map((c) => (
            <li key={c.id} className="nav-row" data-testid="collection-row" data-id={c.id}>
              {navButton(c.name, is("collection", c.id), () => onScope({ kind: "collection", id: c.id }), "nav-collection", count(c.items), c.depth)}
              <MenuButton
                label={`Actions for collection ${c.name}`}
                testId="collection-menu"
                items={[
                  {
                    label: "Rename…",
                    onSelect: () => onAction({ kind: "rename-collection", collection: c }),
                    testId: "menu-rename",
                  },
                  {
                    label: "New sub-collection…",
                    onSelect: () => onAction({ kind: "new-collection", parentId: c.id }),
                    testId: "menu-new-sub",
                  },
                  {
                    label: "Move to…",
                    onSelect: () => onAction({ kind: "move-collection", collection: c }),
                    testId: "menu-move",
                  },
                  {
                    label: "Delete…",
                    onSelect: () => onAction({ kind: "delete-collection", collection: c }),
                    danger: true,
                    testId: "menu-delete",
                  },
                ]}
              />
            </li>
          ))}
        </ul>,
      )}

      {group(
        "smart",
        "Smart collections",
        <button type="button" className="icon-button nav-add" aria-label="New smart collection" onClick={() => onAction({ kind: "new-smart" })} data-testid="new-smart">
          +
        </button>,
        <ul className="nav-list">
          {data?.smartCollections.length === 0 && <li className="muted small nav-empty">Saved searches appear here.</li>}
          {data?.smartCollections.map((s) => (
            <li key={s.id} className="nav-row" data-testid="smart-row">
              {navButton(
                s.name,
                is("smart", s.id),
                () => onScope({ kind: "smart", id: s.id }),
                "nav-smart",
                s.invalid ? (
                  <span className="nav-warn" title="A tag or collection in its rules was deleted" aria-label="Rules need fixing">
                    !
                  </span>
                ) : null,
              )}
              <MenuButton
                label={`Actions for smart collection ${s.name}`}
                testId="smart-menu"
                items={[
                  {
                    label: "Edit rules…",
                    onSelect: () => onAction({ kind: "edit-smart", smart: s }),
                    testId: "menu-edit",
                  },
                  {
                    label: "Delete…",
                    onSelect: () => onAction({ kind: "delete-smart", smart: s }),
                    danger: true,
                    testId: "menu-delete",
                  },
                ]}
              />
            </li>
          ))}
        </ul>,
      )}

      {group(
        "tags",
        "Tags",
        null,
        <ul className="nav-list" data-testid="tag-list">
          {data?.tags.length === 0 && <li className="muted small nav-empty">Tag items from the selection bar or the details panel.</li>}
          {data?.tags.map((t) => (
            <li key={t.id} className="nav-row" data-testid="tag-row">
              {navButton(`#${t.name}`, is("tag", t.id), () => onScope({ kind: "tag", id: t.id }), "nav-tag", count(t.items))}
              <MenuButton
                label={`Actions for tag ${t.name}`}
                testId="tag-menu"
                items={[
                  {
                    label: "Rename…",
                    onSelect: () => onAction({ kind: "rename-tag", tag: t }),
                    testId: "menu-rename",
                  },
                  {
                    label: "Merge into…",
                    onSelect: () => onAction({ kind: "merge-tag", tag: t }),
                    testId: "menu-merge",
                  },
                  {
                    label: "Delete…",
                    onSelect: () => onAction({ kind: "delete-tag", tag: t }),
                    danger: true,
                    testId: "menu-delete",
                  },
                ]}
              />
            </li>
          ))}
        </ul>,
      )}

      {group(
        "platforms",
        "Platforms",
        null,
        <ul className="nav-list">
          {PLATFORMS.map((p) => (
            <li key={p}>{navButton(PLATFORM_LABEL[p], scope.kind === "none" && view === "all" && platform === p, () => onPlatform(p), `nav-platform-${p}`)}</li>
          ))}
        </ul>,
      )}

      <ul className="nav-list nav-bottom">
        <li>{navButton("Duplicates", is("duplicates"), () => onScope({ kind: "duplicates" }), "nav-duplicates", count(data?.counts.duplicates))}</li>
        <li>{navButton(VIEW_LABEL.archive, plain && view === "archive", () => onView("archive"), "nav-archive")}</li>
        <li>{navButton(VIEW_LABEL.removed, plain && view === "removed", () => onView("removed"), "nav-removed", count(data?.counts.removed))}</li>
      </ul>
    </nav>
  );
}
