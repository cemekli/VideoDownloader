import { expect, test, type Page } from "@playwright/test";

// STATE.md F1.7 Library screen against `vite --mode fixture` (deterministic
// fixture IPC, SPEC §25.1), at 1366×768, 1920×1080 and a narrow 800 px
// window. A pass here does NOT prove real Tauri IPC; e2e-desktop covers that.

const shot = (page: Page, name: string, project: string) =>
  page.screenshot({ path: `../../test-artifacts/screenshots/library-${name}-${project}.png` });

async function ready(page: Page, url = "/") {
  await page.goto(url);
  await expect(page.getByTestId("library-card").first()).toBeVisible();
}

test.describe("library screen", () => {
  test("the page runs with Object.prototype frozen (prototype-pollution guard, ADR-0025)", async ({ page }) => {
    await ready(page);
    expect(await page.evaluate(() => Object.isFrozen(Object.prototype))).toBe(true);
  });

  test("lists fixture items with their local state", async ({ page }) => {
    await ready(page);
    await expect(page.getByRole("heading", { level: 1, name: "All Items" })).toBeVisible();
    // Fixture data must be labelled as such (F0.3 status badge).
    await expect(page.getByTestId("backend-status")).toHaveText("Fixture data");
    await expect(page.getByTestId("library-card")).toHaveCount(7);
    await expect(page.getByTestId("result-count")).toHaveText("7 items");
    for (const [state, label] of [
      ["bookmark_only", "Bookmark only"],
      ["local_ready", "Downloaded"],
      ["missing", "File missing"],
      ["disconnected", "Drive disconnected"],
    ] as const) {
      await expect(page.locator(`[data-local-state="${state}"]`).first().getByTestId("local-state")).toHaveText(new RegExp(label));
    }
  });

  test("unknown duration is a dash; a missing thumbnail has a placeholder", async ({ page }) => {
    await ready(page);
    const card = page.getByTestId("library-card").filter({ hasText: "Thread with embedded video" });
    await expect(card.getByTestId("duration")).toHaveText("—");
    await expect(card.locator(".thumb")).toHaveAttribute("data-thumb", "missing");
  });

  test("no horizontal page scroll, also with long titles", async ({ page }) => {
    await ready(page);
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    expect(overflow).toBeLessThanOrEqual(0);
  });

  test("grid columns follow the window (SPEC §9.2)", async ({ page }, info) => {
    await ready(page);
    const lefts = await page.getByTestId("library-card").evaluateAll((els) => new Set(els.map((e) => Math.round(e.getBoundingClientRect().left))).size);
    if (info.project.name === "desktop-1366") expect(lefts).toBeGreaterThanOrEqual(2);
    if (info.project.name === "desktop-1920") expect(lefts).toBeGreaterThanOrEqual(4);
    const width = await page.getByTestId("library-card").first().evaluate((e) => e.getBoundingClientRect().width);
    expect(width).toBeGreaterThanOrEqual(220);
  });

  test("search with Ctrl+K, highlight and no-result state", async ({ page }) => {
    await ready(page);
    await page.keyboard.press("Control+k");
    await expect(page.getByTestId("search-input")).toBeFocused();
    await page.keyboard.type("ekmek");
    await expect(page.getByTestId("library-card")).toHaveCount(1);
    await expect(page.getByTestId("library-card").locator("mark")).toHaveText("ekmek");
    await page.getByTestId("search-input").fill("zzzz");
    await expect(page.getByTestId("library-no-match")).toContainText("No results for “zzzz”");
    await page.getByRole("button", { name: "Clear search" }).click();
    await expect(page.getByTestId("library-card")).toHaveCount(7);
  });

  test("filter chips and an empty filter", async ({ page }, info) => {
    await ready(page);
    await page.getByTestId("chip-kind-local_video").click();
    await expect(page.getByTestId("library-card")).toHaveCount(2);
    await page.getByTestId("chip-kind-bookmark").click();
    await expect(page.getByTestId("library-card")).toHaveCount(5);
    await page.getByTestId("clear-filters").click();
    await expect(page.getByTestId("library-card")).toHaveCount(7);
    if (info.project.name === "narrow-800") await page.getByTestId("menu-button").click();
    await page.getByTestId("nav-platform-facebook").click();
    await expect(page.getByTestId("library-no-match")).toBeVisible();
    await shot(page, "no-match", info.project.name);
  });

  test("keyboard: Tab into the grid, arrows, Enter opens details, Esc closes (§9.4)", async ({ page }, info) => {
    await ready(page);
    const first = page.getByTestId("library-card").first();
    await first.focus();
    await expect(first).toBeFocused();
    const outline = await first.evaluate((el) => getComputedStyle(el).outlineStyle);
    expect(outline).not.toBe("none");
    await page.keyboard.press("ArrowDown");
    const focused = page.locator('[data-testid="library-card"]:focus');
    await expect(focused).toHaveAttribute("aria-selected", "true");
    const title = await focused.getByRole("heading").textContent();
    await page.keyboard.press("Enter");
    const inspector = page.getByTestId("inspector");
    await expect(inspector.getByTestId("inspector-title")).toHaveText(title!);
    if (info.project.name === "narrow-800") await expect(inspector).toHaveAttribute("role", "dialog");
    await shot(page, "inspector", info.project.name);
    await inspector.getByTestId("inspector-close").focus();
    await page.keyboard.press("Escape");
    await expect(inspector).toHaveCount(0);
    await expect(focused).toBeFocused();
  });

  test("list layout is remembered across reloads", async ({ page }, info) => {
    await ready(page);
    await page.getByTestId("layout-list").click();
    await expect(page.getByTestId("library-results")).toHaveAttribute("data-layout", "list");
    await shot(page, "list", info.project.name);
    await page.reload();
    await expect(page.getByTestId("library-results")).toHaveAttribute("data-layout", "list");
  });

  test("import shows the result", async ({ page }) => {
    await ready(page);
    await page.getByTestId("import-button").click();
    await page.getByTestId("import-files").click();
    await expect(page.getByTestId("toast")).toContainText("Imported 2 files.");
  });

  test("first use: empty library", async ({ page }, info) => {
    await page.goto("/?fixture=empty");
    await expect(page.getByTestId("library-empty")).toBeVisible();
    await expect(page.getByTestId("empty-import-files")).toBeEnabled();
    await shot(page, "empty", info.project.name);
  });

  test("loading state", async ({ page }, info) => {
    await page.goto("/?fixture=slow");
    await expect(page.getByTestId("library-loading")).toBeVisible();
    await shot(page, "loading", info.project.name);
  });

  test("error state", async ({ page }, info) => {
    await page.goto("/?fixture=error");
    await expect(page.getByTestId("library-error")).toContainText("INTERNAL_ERROR");
    await shot(page, "error", info.project.name);
  });

  test("large library is virtualized and keeps paging", async ({ page }, info) => {
    await ready(page, "/?fixture=many");
    await expect(page.getByTestId("result-count")).toHaveText("2000 items");
    const rendered = await page.getByTestId("library-card").count();
    expect(rendered).toBeLessThan(200);
    await shot(page, "many", info.project.name);
    // Scrolling to the end of the loaded page brings in the next pages.
    const results = page.getByTestId("library-results");
    const maxShown = () => page.getByTestId("library-card").evaluateAll((els) => Math.max(...els.map((e) => Number(e.getAttribute("aria-posinset")))));
    await expect
      .poll(async () => {
        await results.evaluate((el) => (el.scrollTop = el.scrollHeight));
        return maxShown();
      })
      .toBeGreaterThan(250);
    expect(await page.getByTestId("library-card").count()).toBeLessThan(200);
  });

  test("offline banner", async ({ page, context }, info) => {
    await ready(page);
    await context.setOffline(true);
    await expect(page.getByTestId("offline-banner")).toBeVisible();
    await expect(page.getByTestId("library-card")).toHaveCount(7);
    await shot(page, "offline", info.project.name);
    await context.setOffline(false);
    await expect(page.getByTestId("offline-banner")).toHaveCount(0);
  });

  test("screenshot for visual review", async ({ page }, info) => {
    await ready(page);
    await page.screenshot({ path: `../../test-artifacts/screenshots/library-${info.project.name}.png` });
  });
});
