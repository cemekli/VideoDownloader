import { useCallback, useState } from "react";

// Panel and view preferences are kept locally (SPEC §9.2). Storage can be
// missing or throw (private mode, blocked site data); the defaults apply then.

export interface Prefs {
  layout: "grid" | "list";
  inspectorOpen: boolean;
  sidebarCollapsed: boolean;
}

export const DEFAULT_PREFS: Prefs = { layout: "grid", inspectorOpen: false, sidebarCollapsed: false };
const KEY = "vv.prefs.v1";

export function readPrefs(): Prefs {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return DEFAULT_PREFS;
    const p = JSON.parse(raw) as Partial<Prefs>;
    return {
      layout: p.layout === "list" ? "list" : "grid",
      inspectorOpen: p.inspectorOpen === true,
      sidebarCollapsed: p.sidebarCollapsed === true,
    };
  } catch {
    return DEFAULT_PREFS;
  }
}

export function usePrefs(): [Prefs, (patch: Partial<Prefs>) => void] {
  const [prefs, setPrefs] = useState<Prefs>(readPrefs);
  const update = useCallback((patch: Partial<Prefs>) => {
    setPrefs((p) => {
      const next = { ...p, ...patch };
      try {
        localStorage.setItem(KEY, JSON.stringify(next));
      } catch {
        // Not persisted; the choice still applies for this session.
      }
      return next;
    });
  }, []);
  return [prefs, update];
}
