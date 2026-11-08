import { expect, test, type Page } from "@playwright/test";

// STATE.md F1.8: Journey 1 "save in five seconds" (SPEC §8.1, NFR-02) and
// the Add URL states against `vite --mode fixture` at 1366×768, 1920×1080
// and 800 px. The real save path is covered by Rust tests and e2e-desktop.

const shot = (page: Page, name: string, project: string) =>
  page.screenshot({ path: `../../test-artifacts/screenshots/add-url-${name}-${project}.png` });

async function ready(page: Page) {
  await page.goto("/");
  await expect(page.getByTestId("library-card").first()).toBeVisible();
}

test.describe("add url", () => {
  test("Journey 1: two primary actions from a ready link to a visible bookmark", async ({ page }, info) => {
    await ready(page);
    // Action 1: Add URL. The field has focus, so the link is pasted straight in.
    await page.getByTestId("add-url-button").click();
    await expect(page.getByTestId("add-url-input")).toBeFocused();
    const t0 = Date.now();
    await page.keyboard.insertText("https://youtu.be/journeyVid1");
    await expect(page.getByTestId("add-url-title")).toHaveText("YouTube · journeyVid1");
    await shot(page, "preview", info.project.name);
    // Action 2: Enter saves.
    await page.keyboard.press("Enter");
    const card = page.getByTestId("library-card").first();
    await expect(card).toContainText("YouTube · journeyVid1");
    const ms = Date.now() - t0;
    info.annotations.push({ type: "NFR-02", description: `paste → visible card: ${ms} ms, 2 primary actions` });
    expect(ms).toBeLessThan(5_000);
    await expect(card).toHaveAttribute("aria-selected", "true");
    await expect(page.getByTestId("toast")).toContainText("Saved");
    await expect(page.getByTestId("result-count")).toHaveText("8 items");
    await shot(page, "saved", info.project.name);
  });

  test("Ctrl+K, paste, Enter, Enter", async ({ page }) => {
    await ready(page);
    await page.keyboard.press("Control+k");
    await page.keyboard.insertText("https://vk.com/video-7_8");
    await expect(page.getByTestId("url-suggestion")).toBeVisible();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("add-url-input")).toHaveValue("https://vk.com/video-7_8");
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("library-card").first()).toContainText("VK · video-7_8");
  });

  test("error, duplicate and removed states", async ({ page }, info) => {
    await ready(page);
    await page.getByTestId("add-url-button").click();
    await page.keyboard.insertText("http://127.0.0.1/admin");
    await expect(page.getByTestId("add-url-error")).toContainText("local network");
    await shot(page, "error", info.project.name);

    await page.getByTestId("add-url-input").fill("https://www.youtube.com/watch?v=aaaaaaaaaaa");
    await expect(page.getByTestId("add-url-existing")).toBeVisible();
    await shot(page, "duplicate", info.project.name);

    await page.getByTestId("add-url-input").fill("https://youtu.be/removedVid1");
    await expect(page.getByTestId("add-url-removed")).toBeVisible();
    await shot(page, "removed", info.project.name);
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("toast")).toContainText("Restored");
    await expect(page.getByTestId("result-count")).toHaveText("8 items");
  });

  test("Esc closes and returns focus; the dialog fits the window", async ({ page }) => {
    await ready(page);
    await page.getByTestId("add-url-button").click();
    const box = await page.getByTestId("add-url-dialog").boundingBox();
    const vw = page.viewportSize()!.width;
    expect(box!.x).toBeGreaterThanOrEqual(0);
    expect(box!.x + box!.width).toBeLessThanOrEqual(vw);
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("add-url-dialog")).toHaveCount(0);
    await expect(page.getByTestId("add-url-button")).toBeFocused();
  });
});
