import { copyFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, test, type Page } from "@playwright/test";
import { launchDesktopApp, repoRoot } from "./app";

// F1 and F2 exit gates (STATE.md; SPEC §3 M1, DATA-04) in the real Windows
// app: with Offline Mode on, save a link, import a file, tag, search and
// play it, then restart and find everything again. Offline Mode stops the
// app's own network use (ADR-0007 netguard refuses before DNS); M1 has no
// network feature, so this is the same path as a machine with no network.
// The manual check with the network unplugged is listed in HANDOFF.md.

test.skip(process.platform !== "win32", "Real-window E2E runs on Windows only (WebView2 + CDP, ADR-0001)");

const lastToast = (page: Page) => page.getByTestId("toast").last();
const cards = (page: Page) => page.getByTestId("library-card");

test("offline: save, import, tag, search, play, restart (F1/F2 exit gates, DATA-04)", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "vv-e2e-offline-"));
  const mediaDir = mkdtempSync(join(tmpdir(), "vv-e2e-offline-media-"));
  const clip = join(mediaDir, "deniz-tatili.mp4");
  copyFileSync(join(repoRoot, "tests", "fixtures", "media", "mp4-h264-aac.mp4"), clip);
  const env = { VV_DATA_DIR: dataDir, VV_TOOLS_DIR: join(repoRoot, "tools", "bin"), VV_E2E_PICK: clip };
  try {
    let app = await launchDesktopApp({ env });
    try {
      const { page } = app;
      await expect(page.getByTestId("library-empty")).toBeVisible();
      await page.keyboard.press("Control+,");
      await page.getByTestId("settings-offline").check();
      await page.getByTestId("settings-close").click();
      await expect(page.getByTestId("offline-mode-banner")).toBeVisible();

      await page.getByTestId("add-url-button").click();
      await page.keyboard.insertText("https://youtu.be/vvOffVid001");
      await page.keyboard.press("Enter");
      await expect(cards(page).filter({ hasText: "vvOffVid001" })).toBeVisible();

      await page.getByTestId("import-button").click();
      await page.getByTestId("import-files").click();
      await expect(lastToast(page)).toContainText("Imported 1 file.", { timeout: 30_000 });
      const clipCard = cards(page).filter({ hasText: "deniz-tatili" });
      await expect(clipCard).toBeVisible();

      await clipCard.click();
      await page.getByTestId("bulk-tag").click();
      await page.keyboard.insertText("tatil");
      await page.keyboard.press("Enter");
      await expect(lastToast(page)).toContainText("Tagged 1 item with #tatil.");

      await page.getByTestId("search-input").fill("tatil");
      await expect(cards(page)).toHaveCount(1);
      await cards(page).first().dblclick();
      await page.getByTestId("inspector-play").click();
      const video = page.getByTestId("player-video");
      await video.evaluate((v: HTMLVideoElement) => {
        v.muted = true;
        void v.play();
      });
      await expect.poll(() => video.evaluate((v: HTMLVideoElement) => v.currentTime), { timeout: 10_000 }).toBeGreaterThan(0.5);
      expect(await video.evaluate((v: HTMLVideoElement) => v.videoWidth)).toBeGreaterThan(0);
      await page.screenshot({ path: "../../test-artifacts/screenshots/desktop-windows-offline-play.png" });
      await page.keyboard.press("Escape");
      await expect(page.getByTestId("player")).toHaveCount(0);
    } finally {
      await app.close();
    }

    app = await launchDesktopApp({ env });
    try {
      const { page } = app;
      // Offline Mode is a saved setting: still on after the restart.
      await expect(page.getByTestId("offline-mode-banner")).toBeVisible();
      await page.getByTestId("nav-all").click();
      await expect(page.getByTestId("result-count")).toHaveText("2 items");
      await page.getByTestId("search-input").fill("vvOffVid001");
      await expect(cards(page)).toHaveCount(1);
      await page.getByTestId("search-input").fill("tatil");
      await expect(cards(page)).toHaveCount(1);
      await expect(cards(page).first()).toContainText("deniz-tatili");
    } finally {
      await app.close();
    }
  } finally {
    rmSync(dataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
    rmSync(mediaDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  }
});
