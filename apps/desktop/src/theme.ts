import type { Theme } from "./ipc/types";

// Theme switching (SPEC §9.6, §9.7 step 5). `public/theme-init.js` applies
// the last theme before React starts; this keeps it in step afterwards.

const KEY = "vv.theme";

export function resolveTheme(theme: Theme): "dark" | "light" {
  if (theme !== "system") return theme;
  return typeof window !== "undefined" && typeof window.matchMedia === "function" && window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = resolveTheme(theme);
  try {
    localStorage.setItem(KEY, theme);
  } catch {
    // Storage may be blocked; the theme still applies for this run.
  }
}
