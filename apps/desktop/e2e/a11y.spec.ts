import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

// STATE.md F2.9 (NFR-12, AC-27): an automatic axe scan of every main
// surface in both themes, keyboard reachability, and the 200% scale check.
// Axe finds only part of the problems; the manual checklist (MT list in
// HANDOFF.md) covers screen readers and Windows high contrast.

async function serious(page: Page, scope?: string) {
  const builder = new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"]);
  if (scope) builder.include(scope);
  const { violations } = await builder.analyze();
  return violations
    .filter((v) => v.impact === "critical" || v.impact === "serious")
    .map((v) => `${v.id} (${v.impact}): ${v.nodes.map((n) => n.target.join(" ")).slice(0, 3).join(", ")}`);
}

async function ready(page: Page, query = "") {
  await page.goto(`/${query}`);
  await expect(page.getByTestId("library-card").first()).toBeVisible();
}

async function setTheme(page: Page, theme: "dark" | "light") {
  await page.addInitScript((t) => localStorage.setItem("vv.theme", t), theme);
}

for (const theme of ["dark", "light"] as const) {
  test.describe(`axe, ${theme} theme (NFR-12)`, () => {
    test.beforeEach(async ({ page }, info) => {
      test.skip(info.project.name === "desktop-1920", "two sizes are enough for the scan");
      await setTheme(page, theme);
    });

    test("library, details and search", async ({ page }) => {
      await ready(page);
      expect(await serious(page)).toEqual([]);
      await page.locator('[data-testid="library-card"][data-id$="0005"]').dblclick();
      await expect(page.getByTestId("inspector")).toBeVisible();
      expect(await serious(page)).toEqual([]);
      await page.getByTestId("search-input").fill("zzzz");
      await expect(page.getByTestId("library-no-match")).toBeVisible();
      expect(await serious(page)).toEqual([]);
    });

    test("player, settings and Add URL", async ({ page }) => {
      await ready(page);
      await page.locator('[data-testid="library-card"][data-id$="0001"]').dblclick();
      await page.getByTestId("inspector-play").click();
      await expect(page.getByTestId("player")).toBeVisible();
      expect(await serious(page, '[data-testid="player"]')).toEqual([]);
      await page.keyboard.press("Escape");
      await page.getByTestId("settings-button").click();
      await expect(page.getByTestId("settings")).toBeVisible();
      expect(await serious(page, '[data-testid="settings"]')).toEqual([]);
      await page.keyboard.press("Escape");
      await page.getByTestId("add-url-button").click();
      await expect(page.getByTestId("add-url-dialog")).toBeVisible();
      expect(await serious(page, '[data-testid="add-url-dialog"]')).toEqual([]);
    });

    test("download dialog and the Downloads screen", async ({ page }, info) => {
      await ready(page, "?fixture=download-error");
      await page.locator('[data-testid="library-card"][data-id$="0002"]').dblclick();
      await page.getByTestId("inspector-download").click();
      await expect(page.getByTestId("download-dialog")).toBeVisible();
      expect(await serious(page, '[data-testid="download-dialog"]')).toEqual([]);
      await page.getByTestId("download-start").click();
      if (info.project.name === "narrow-800") {
        await page.keyboard.press("Escape");
        await page.getByTestId("menu-button").click();
      }
      await page.getByTestId("nav-downloads").click();
      await expect(page.getByTestId("downloads-failed")).toBeVisible();
      expect(await serious(page)).toEqual([]);
    });

    test("Sessions screen and its connect dialog (F5.5)", async ({ page }, info) => {
      await ready(page, "?fixture=login-required");
      if (info.project.name === "narrow-800") await page.getByTestId("menu-button").click();
      await page.getByTestId("nav-sessions").click();
      await expect(page.getByTestId("sessions")).toBeVisible();
      expect(await serious(page)).toEqual([]);
      await page.locator('[data-testid="session-site"][data-provider="reddit"]').getByTestId("session-site-connect").click();
      await expect(page.getByTestId("session-dialog")).toBeVisible();
      expect(await serious(page, '[data-testid="session-dialog"]')).toEqual([]);
    });

    test("AI Processing screen (F6.6)", async ({ page }, info) => {
      await ready(page, "?fixture=low-memory");
      if (info.project.name === "narrow-800") await page.getByTestId("menu-button").click();
      await page.getByTestId("nav-ai").click();
      await expect(page.getByTestId("ai-processing")).toBeVisible();
      await expect(page.getByTestId("model-row").first()).toBeVisible();
      expect(await serious(page)).toEqual([]);
    });

    test("first run, every step", async ({ page }) => {
      await page.goto("/?fixture=first-run");
      const ob = page.getByTestId("onboarding");
      for (const step of ["welcome", "legal", "storage", "updates", "theme"]) {
        await expect(ob).toHaveAttribute("data-step", step);
        expect(await serious(page), step).toEqual([]);
        await (step === "legal" ? ob.getByTestId("legal-ack") : ob.getByTestId("onboarding-next")).click();
      }
      await expect(ob).toHaveCount(0);
    });
  });
}

test.describe("200% scale (AC-27)", () => {
  // 1366×768 at 200% leaves a 683×384 CSS-pixel window.
  test.use({ viewport: { width: 683, height: 384 }, deviceScaleFactor: 2 });
  test.beforeEach(({}, info) => test.skip(info.project.name !== "desktop-1366", "one run is enough"));

  const noSideScroll = (page: Page) => page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth);

  test("save and search by keyboard; focus stays visible and nothing is cut off", async ({ page }) => {
    await ready(page);
    expect(await noSideScroll(page)).toBe(true);
    await expect(page.getByTestId("add-url-button")).toBeInViewport();

    await page.keyboard.press("Control+k");
    await expect(page.getByTestId("search-input")).toBeFocused();
    const outline = await page.getByTestId("search-input").evaluate((e) => getComputedStyle(e).outlineStyle);
    expect(outline).not.toBe("none");
    await page.keyboard.type("ekmek");
    await expect(page.getByTestId("library-card")).toHaveCount(1);
    await page.keyboard.press("Escape");

    await page.getByTestId("search-input").fill("");
    await page.getByTestId("add-url-button").focus();
    await page.keyboard.press("Enter");
    const dialog = page.getByTestId("add-url-dialog");
    await expect(dialog).toBeVisible();
    await page.keyboard.insertText("https://www.youtube.com/watch?v=bbbbbbbbbbb");
    await page.keyboard.press("Enter");
    await expect(dialog).toHaveCount(0);
    await expect(page.getByTestId("library-card").first()).toBeVisible();
    expect(await noSideScroll(page)).toBe(true);
  });

  test("dialogs fit and scroll inside the small window", async ({ page }) => {
    await ready(page);
    for (const open of [() => page.getByTestId("settings-button").click(), () => page.getByTestId("add-url-button").click()]) {
      await open();
      const box = await page.locator('[role="dialog"]').last().boundingBox();
      expect(box!.x).toBeGreaterThanOrEqual(0);
      expect(box!.x + box!.width).toBeLessThanOrEqual(683);
      expect(await noSideScroll(page)).toBe(true);
      await page.keyboard.press("Escape");
    }
    await page.goto("/?fixture=first-run");
    await expect(page.getByTestId("onboarding-next")).toBeVisible();
    await page.getByTestId("onboarding-next").scrollIntoViewIfNeeded();
    await expect(page.getByTestId("onboarding-next")).toBeInViewport();
    expect(await noSideScroll(page)).toBe(true);
  });
});
