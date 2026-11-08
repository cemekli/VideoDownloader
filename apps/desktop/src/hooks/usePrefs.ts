import { useCallback, useState } from "react";

// Panel and view preferences are kept locally (SPEC §9.2). Storage can be
// missing or throw (private mode, blocked site data); the defaults apply then.

export interface Prefs {
  layout: "grid" | "list";
  inspectorOpen: boolean;
  sidebarCollapsed: boolean;
  /** Width of the left navigation in CSS pixels; the user drags its edge. */
  navWidth: number;
}

export const NAV_WIDTH = { min: 180, max: 480, initial: 232 } as const;

export function clampNavWidth(w: number): number {
  if (!Number.isFinite(w)) return NAV_WIDTH.initial;
  return Math.round(Math.min(NAV_WIDTH.max, Math.max(NAV_WIDTH.min, w)));
}

export const DEFAULT_PREFS: Prefs = { layout: "grid", inspectorOpen: false, sidebarCollapsed: false, navWidth: NAV_WIDTH.initial };
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
      navWidth: typeof p.navWidth === "number" ? clampNavWidth(p.navWidth) : NAV_WIDTH.initial,
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
