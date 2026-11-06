import { expect, test } from "@playwright/test";

// Runs against `vite --mode fixture` (deterministic fixture IPC, SPEC §25.1).
// A pass here does NOT prove real Tauri IPC works; that is covered in F0.3+.

test.describe("library shell", () => {
  test("lists fixture items and separates bookmark-only from downloaded", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("heading", { name: "Library" })).toBeVisible();

    const cards = page.getByTestId("library-card");
    await expect(cards).toHaveCount(6);
    await expect(page.locator('[data-local-state="bookmark_only"]').first().getByTestId("local-state")).toHaveText("Bookmark only");
    await expect(page.locator('[data-local-state="local_ready"]').first().getByTestId("local-state")).toHaveText("Downloaded");
  });

  test("unknown duration is shown as a dash", async ({ page }) => {
    await page.goto("/");
    const card = page.getByTestId("library-card").filter({ hasText: "Thread with embedded video" });
    await expect(card.getByTestId("duration")).toHaveText("—");
  });

  test("long titles do not cause horizontal page scroll", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByTestId("library-card")).toHaveCount(6);
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    expect(overflow).toBeLessThanOrEqual(0);
  });

  test("cards are reachable with the keyboard and show visible focus", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByTestId("library-card")).toHaveCount(6);
    await page.keyboard.press("Tab");
    const focused = page.locator(":focus");
    await expect(focused).toHaveAttribute("data-testid", "library-card");
    const outline = await focused.evaluate((el) => getComputedStyle(el).outlineStyle);
    expect(outline).not.toBe("none");
  });

  test("screenshot for visual review", async ({ page }, info) => {
    await page.goto("/");
    await expect(page.getByTestId("library-card")).toHaveCount(6);
    await page.screenshot({ path: `../../test-artifacts/screenshots/library-${info.project.name}.png`, fullPage: true });
  });
});
