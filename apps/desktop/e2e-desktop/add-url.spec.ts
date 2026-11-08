import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { launchDesktopApp } from "./app";

// STATE.md F1.8, Journey 1 (SPEC §8.1) and AC-01 in the real Windows app:
// save a link through the real IPC, kill the app, start it again on the same
// data directory and find the bookmark with search. Saving never touches the
// network (no metadata fetch exists in M1), so this is the offline path.

test.skip(process.platform !== "win32", "Real-window E2E runs on Windows only (WebView2 + CDP, ADR-0001)");

test("save a link, restart, find it (Journey 1, AC-01)", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "vv-e2e-addurl-"));
  try {
    let app = await launchDesktopApp({ env: { VV_DATA_DIR: dataDir } });
    try {
      const { page } = app;
      await expect(page.getByTestId("library-empty")).toBeVisible();
      await page.getByTestId("add-url-button").click();
      await expect(page.getByTestId("add-url-input")).toBeFocused();
      const t0 = Date.now();
      await page.keyboard.insertText("https://www.youtube.com/watch?v=vvE2eVid001&si=share");
      await page.keyboard.press("Enter");
      const card = page.getByTestId("library-card").filter({ hasText: "YouTube · vvE2eVid001" });
      await expect(card).toBeVisible();
      const ms = Date.now() - t0;
      test.info().annotations.push({ type: "NFR-02", description: `paste → visible card: ${ms} ms (target ≈ 5 s, 2 primary actions)` });
      expect(ms).toBeLessThan(5_000);
      await expect(card.getByTestId("local-state")).toHaveText(/Bookmark only/);
      await page.screenshot({ path: "../../test-artifacts/screenshots/desktop-windows-add-url.png" });

      // The same link again goes to the existing item.
      await page.getByTestId("add-url-button").click();
      await page.keyboard.insertText("https://youtu.be/vvE2eVid001");
      await expect(page.getByTestId("add-url-existing")).toBeVisible();
      await page.keyboard.press("Escape");
    } finally {
      await app.close();
    }

    app = await launchDesktopApp({ env: { VV_DATA_DIR: dataDir } });
    try {
      const { page } = app;
      await expect(page.getByTestId("result-count")).toHaveText("1 item");
      await page.getByTestId("search-input").fill("vvE2eVid001");
      await expect(page.getByTestId("library-card")).toHaveCount(1);
      await expect(page.getByTestId("library-card").first()).toContainText("YouTube · vvE2eVid001");
    } finally {
      await app.close();
    }
  } finally {
    rmSync(dataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  }
});
