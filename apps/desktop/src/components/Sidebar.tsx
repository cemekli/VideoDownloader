import { useState } from "react";
import type { LibraryView, Platform } from "../ipc/types";
import { PLATFORMS } from "../ipc/types";
import { PLATFORM_LABEL, VIEW_LABEL } from "../labels";

const VIEWS: LibraryView[] = ["all", "favorites", "recent"];

/**
 * Left navigation (SPEC §9.2). Entries for later phases (Inbox, Downloads,
 * Collections, Sessions, AI, Settings) are hidden until they work (§9.3).
 */
export function Sidebar({
  view,
  platform,
  onView,
  onPlatform,
}: {
  view: LibraryView;
  platform: Platform | null;
  onView: (v: LibraryView) => void;
  onPlatform: (p: Platform | null) => void;
}) {
  const [platformsOpen, setPlatformsOpen] = useState(true);
  const navButton = (label: string, current: boolean, onClick: () => void, testId: string) => (
    <li key={testId}>
      <button type="button" className="nav-item" aria-current={current ? "page" : undefined} onClick={onClick} data-testid={testId}>
        {label}
      </button>
    </li>
  );
  return (
    <nav className="sidebar" aria-label="Library navigation" data-testid="sidebar">
      <ul className="nav-list">{VIEWS.map((v) => navButton(VIEW_LABEL[v], view === v && platform === null, () => onView(v), `nav-${v}`))}</ul>
      <div className="nav-group">
        <button type="button" className="nav-group-toggle" aria-expanded={platformsOpen} onClick={() => setPlatformsOpen((o) => !o)}>
          Platforms
        </button>
        {platformsOpen && (
          <ul className="nav-list">
            {PLATFORMS.map((p) => navButton(PLATFORM_LABEL[p], view === "all" && platform === p, () => onPlatform(p), `nav-platform-${p}`))}
          </ul>
        )}
      </div>
      <ul className="nav-list nav-bottom">{navButton(VIEW_LABEL.archive, view === "archive" && platform === null, () => onView("archive"), "nav-archive")}</ul>
    </nav>
  );
}
