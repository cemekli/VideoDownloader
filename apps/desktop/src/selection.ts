// Multi-selection over the loaded results (SPEC §9.3 "toplu seçim"). Pure
// functions so the rules are unit-tested apart from the grid.

export interface Selection {
  ids: ReadonlySet<string>;
  /** Index where a Shift range starts. */
  anchor: number | null;
  /** Index with keyboard focus (roving tabindex). */
  focus: number | null;
}

export const EMPTY_SELECTION: Selection = { ids: new Set(), anchor: null, focus: null };

function range(ids: readonly string[], a: number, b: number): Set<string> {
  const [lo, hi] = a <= b ? [a, b] : [b, a];
  return new Set(ids.slice(lo, hi + 1));
}

/** Click: plain selects one, Ctrl/Cmd toggles one, Shift selects from the anchor. */
export function clickSelect(sel: Selection, ids: readonly string[], index: number, mods: { shift: boolean; toggle: boolean }): Selection {
  const id = ids[index];
  if (id === undefined) return sel;
  if (mods.shift && sel.anchor !== null) return { ids: range(ids, sel.anchor, index), anchor: sel.anchor, focus: index };
  if (mods.toggle) {
    const next = new Set(sel.ids);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    return { ids: next, anchor: index, focus: index };
  }
  return { ids: new Set([id]), anchor: index, focus: index };
}

/**
 * Arrow keys: focus moves and selects that item; with Shift the range from
 * the anchor is selected; with Ctrl only focus moves (ARIA listbox pattern).
 */
export function moveFocus(sel: Selection, ids: readonly string[], to: number, mods: { shift: boolean; ctrl: boolean }): Selection {
  if (ids.length === 0) return sel;
  const index = Math.max(0, Math.min(ids.length - 1, to));
  if (mods.ctrl) return { ...sel, focus: index };
  if (mods.shift) {
    const anchor = sel.anchor ?? sel.focus ?? index;
    return { ids: range(ids, anchor, index), anchor, focus: index };
  }
  return { ids: new Set([ids[index]!]), anchor: index, focus: index };
}

export function selectAll(ids: readonly string[], sel: Selection): Selection {
  return { ids: new Set(ids), anchor: sel.anchor ?? 0, focus: sel.focus ?? 0 };
}

/** Space with Ctrl: toggle the focused item without moving. */
export function toggleFocused(sel: Selection, ids: readonly string[]): Selection {
  if (sel.focus === null) return sel;
  return clickSelect(sel, ids, sel.focus, { shift: false, toggle: true });
}
