import { describe, expect, it } from "vitest";
import { clickSelect, EMPTY_SELECTION, moveFocus, selectAll, toggleFocused } from "./selection";
import { geometry, GAP, MIN_CARD } from "./components/Results";

const ids = ["a", "b", "c", "d", "e"];
const set = (s: { ids: ReadonlySet<string> }) => [...s.ids].sort();

describe("selection", () => {
  it("click selects one, Ctrl toggles, Shift selects a range from the anchor", () => {
    let s = clickSelect(EMPTY_SELECTION, ids, 1, { shift: false, toggle: false });
    expect(set(s)).toEqual(["b"]);
    s = clickSelect(s, ids, 3, { shift: true, toggle: false });
    expect(set(s)).toEqual(["b", "c", "d"]);
    s = clickSelect(s, ids, 2, { shift: false, toggle: true });
    expect(set(s)).toEqual(["b", "d"]);
    s = clickSelect(s, ids, 0, { shift: true, toggle: false });
    expect(set(s)).toEqual(["a", "b", "c"]);
  });

  it("arrows select, Shift extends, Ctrl only moves focus; ends are clamped", () => {
    let s = moveFocus(EMPTY_SELECTION, ids, 2, { shift: false, ctrl: false });
    expect(set(s)).toEqual(["c"]);
    s = moveFocus(s, ids, 9, { shift: true, ctrl: false });
    expect(set(s)).toEqual(["c", "d", "e"]);
    expect(s.focus).toBe(4);
    s = moveFocus(s, ids, 0, { shift: false, ctrl: true });
    expect(set(s)).toEqual(["c", "d", "e"]);
    expect(s.focus).toBe(0);
    s = toggleFocused(s, ids);
    expect(set(s)).toEqual(["a", "c", "d", "e"]);
    expect(set(selectAll(ids, s))).toEqual(ids);
  });
});

describe("grid geometry (SPEC §9.2)", () => {
  it("keeps cards at least the minimum width", () => {
    for (const w of [500, 1000, 1366 - 232 - 32, 1920 - 232 - 32 - 340, 3000]) {
      const g = geometry(w, "grid", false);
      expect(g.cardWidth).toBeGreaterThanOrEqual(MIN_CARD);
      expect(g.cols * g.cardWidth + (g.cols - 1) * GAP).toBeCloseTo(w);
    }
  });

  it("1366×768 with the inspector closed has at least two columns; 1920 has four or more", () => {
    expect(geometry(1366 - 232 - 32, "grid", false).cols).toBeGreaterThanOrEqual(2);
    expect(geometry(1920 - 232 - 32, "grid", false).cols).toBeGreaterThanOrEqual(4);
  });

  it("list rows are one column", () => {
    expect(geometry(900, "list", false).cols).toBe(1);
  });
});
