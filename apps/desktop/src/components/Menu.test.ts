import { describe, expect, it } from "vitest";
import { clampNavWidth } from "../hooks/usePrefs";
import { placeMenu } from "./Menu";

const rect = (left: number, top: number, w = 26, h = 26) => ({ left, top, right: left + w, bottom: top + h, width: w, height: h, x: left, y: top, toJSON: () => null }) as DOMRect;
const view = { width: 1000, height: 700 };
const menu = { width: 180, height: 160 };

describe("placeMenu", () => {
  it("opens below the button, toward the content", () => {
    expect(placeMenu(rect(200, 100), menu, view)).toEqual({ top: 130, left: 200 });
  });
  it("aligns to the button's right edge near the right side of the window", () => {
    expect(placeMenu(rect(900, 100), menu, view).left).toBe(926 - 180);
  });
  it("opens above when there is no room below", () => {
    expect(placeMenu(rect(200, 600), menu, view).top).toBe(600 - 4 - 160);
  });
  it("stays inside a window too small for it", () => {
    const p = placeMenu(rect(5, 5), { width: 300, height: 900 }, { width: 250, height: 400 });
    expect(p.left).toBe(8);
    expect(p.top).toBe(35);
  });
});

describe("clampNavWidth", () => {
  it("keeps the navigation between 180 and 480 px", () => {
    expect(clampNavWidth(10)).toBe(180);
    expect(clampNavWidth(999)).toBe(480);
    expect(clampNavWidth(300.6)).toBe(301);
    expect(clampNavWidth(Number.NaN)).toBe(232);
  });
});
