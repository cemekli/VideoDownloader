import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, test, type Page } from "@playwright/test";
import { launchDesktopApp } from "./app";

// STATE.md F2.1–F2.2 in the real Windows app through the real IPC: bulk tag
// and Undo, a collection, Recently removed, then a restart. After the
// restart the removal is still there and Undo has nothing left to revert
// (Undo lasts until the app restarts, §7.3).

test.skip(process.platform !== "win32", "Real-window E2E runs on Windows only (WebView2 + CDP, ADR-0001)");

async function save(page: Page, url: string) {
  await page.getByTestId("add-url-button").click();
  await page.keyboard.insertText(url);
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("add-url-dialog")).toBeHidden();
}

const lastToast = (page: Page) => page.getByTestId("toast").last();

test("tag, collect, remove and Undo through the real backend; Undo ends with the app run", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "vv-e2e-organize-"));
  try {
    let app = await launchDesktopApp({ env: { VV_DATA_DIR: dataDir } });
    try {
      const { page } = app;
      await expect(page.getByTestId("library-empty")).toBeVisible();
      await save(page, "https://youtu.be/vvOrgVid001");
      await save(page, "https://youtu.be/vvOrgVid002");
      await page.getByTestId("nav-all").click();
      await expect(page.getByTestId("result-count")).toHaveText("2 items");

      await page.getByTestId("library-card").first().click();
      await page.keyboard.press("Control+a");
      await page.getByTestId("bulk-tag").click();
      await page.keyboard.insertText("deniz");
      await page.keyboard.press("Enter");
      await expect(lastToast(page)).toContainText("Tagged 2 items with #deniz.");
      // Tags are searchable right after the change (SEA-05).
      await page.getByTestId("search-input").fill("deniz");
      await expect(page.getByTestId("library-card")).toHaveCount(2);
      await page.getByTestId("search-input").fill("");
      await page.keyboard.press("Escape");
      await page.getByTestId("library-card").first().click();
      await page.keyboard.press("Control+z");
      await expect(lastToast(page)).toContainText("Tagging undone.");
      await expect(page.getByTestId("tag-row")).toHaveCount(0);

      await page.getByTestId("new-collection").click();
      await page.keyboard.insertText("Gezi");
      await page.keyboard.press("Enter");
      await page.getByTestId("library-card").first().click();
      await page.getByTestId("bulk-collect").click();
      await page.getByTestId("pick-submit").click();
      await expect(lastToast(page)).toContainText("Added 1 item to Gezi.");
      await expect(page.getByTestId("nav-inbox")).toContainText("1");

      await page.getByTestId("library-card").first().click();
      await page.keyboard.press("Delete");
      await expect(lastToast(page)).toContainText("Moved 1 item to Recently removed.");
      await expect(page.getByTestId("result-count")).toHaveText("1 item");
      await page.screenshot({ path: "../../test-artifacts/screenshots/desktop-windows-organize.png" });
    } finally {
      await app.close();
    }

    app = await launchDesktopApp({ env: { VV_DATA_DIR: dataDir } });
    try {
      const { page } = app;
      await expect(page.getByTestId("result-count")).toHaveText("1 item");
      await page.getByTestId("library-card").first().click();
      await page.keyboard.press("Control+z");
      await expect(lastToast(page)).toContainText("Nothing to undo.");
      await page.getByTestId("nav-removed").click();
      await expect(page.getByTestId("result-count")).toHaveText("1 item");
      await page.getByTestId("library-card").first().click();
      await page.getByTestId("bulk-restore").click();
      await expect(lastToast(page)).toContainText("Restored 1 item.");
    } finally {
      await app.close();
    }
  } finally {
    rmSync(dataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  }
});
