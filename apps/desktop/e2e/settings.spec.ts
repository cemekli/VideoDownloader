import { expect, test, type Page, type TestInfo } from "@playwright/test";

// STATE.md F2.5–F2.8 against `vite --mode fixture`: theme, Offline Mode,
// export/import, backups and the first run. File dialogs, the database and
// the network guard are tested in Rust (tests/portable.rs,
// tests/playback_settings.rs, vv-netguard) and in e2e-desktop.

const shot = (page: Page, name: string, info: TestInfo) => page.screenshot({ path: `../../test-artifacts/screenshots/settings-${name}-${info.project.name}.png` });
const lastToast = (page: Page) => page.getByTestId("toast").last();

async function ready(page: Page, query = "") {
  await page.goto(`/${query}`);
  await expect(page.getByTestId("library-card").first()).toBeVisible();
}

/** Presses Tab until the element with this test id has focus. */
async function tabTo(page: Page, testId: string) {
  for (let i = 0; i < 25; i++) {
    if (await page.evaluate((id) => document.activeElement?.getAttribute("data-testid") === id, testId)) return;
    await page.keyboard.press("Tab");
  }
  throw new Error(`Tab never reached ${testId}`);
}

test.describe("settings", () => {
  test("light theme applies at once and survives a reload (F2.8)", async ({ page }, info) => {
    await ready(page);
    await page.keyboard.press("Control+,");
    const settings = page.getByTestId("settings");
    await expect(settings).toBeVisible();
    await settings.getByTestId("theme-light").check();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
    const bg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    expect(bg).toBe("rgb(246, 247, 249)");
    await shot(page, "light-dialog", info);
    await page.keyboard.press("Escape");
    await page.reload();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
    await expect(page.getByTestId("library-card").first()).toBeVisible();
    await shot(page, "light-library", info);
  });

  test("Offline Mode: banner, no source page, search keeps working, turn off from the banner (AC-17, DATA-04)", async ({ page }, info) => {
    await ready(page);
    await page.getByTestId("settings-button").click();
    await page.getByTestId("settings-offline").check();
    await expect(page.getByTestId("offline-mode-banner")).toContainText("Offline Mode is on");
    await page.getByTestId("settings-close").click();
    await shot(page, "offline-mode", info);

    await page.locator('[data-testid="library-card"][data-id$="0002"]').dblclick();
    await page.getByTestId("inspector").getByTestId("inspector-open-source").click();
    await expect(lastToast(page)).toContainText("Offline Mode is on");

    // Search and playback keep working.
    await page.keyboard.press("Escape");
    await page.keyboard.press("Control+k");
    await page.keyboard.type("holiday");
    await expect(page.getByTestId("library-card")).toHaveCount(1);

    await page.getByTestId("offline-mode-off").click();
    await expect(page.getByTestId("offline-mode-banner")).toHaveCount(0);
  });

  test("export, import with a preview, back up and restore (DATA-01, DATA-06)", async ({ page }) => {
    await ready(page);
    await page.getByTestId("settings-button").click();
    const settings = page.getByTestId("settings");
    await settings.getByTestId("export-library").click();
    await expect(lastToast(page)).toContainText("Exported 12 items to videovault-library-2026-11-08.json.");

    await settings.getByTestId("import-library").click();
    const dialog = page.getByTestId("import-library-dialog");
    await expect(dialog).toBeVisible();
    await expect(dialog.getByTestId("import-root")).not.toHaveCount(0);
    await dialog.getByTestId("import-policy-keep_existing").check();
    await dialog.getByTestId("import-apply").click();
    await expect(lastToast(page)).toContainText("Imported 3 items");

    await settings.getByTestId("backup-now").click();
    await expect(lastToast(page)).toContainText("Backup saved");
    await expect(settings.getByTestId("backup-row")).toHaveCount(1);
    await settings.getByTestId("backup-restore").click();
    const restore = page.getByTestId("restore-dialog");
    await expect(restore.getByTestId("restore-items")).toBeVisible();
    await restore.getByTestId("restore-confirm").click();
    await expect(lastToast(page)).toContainText("Library restored");
  });
});

test.describe("first run", () => {
  test("five steps by keyboard only, then the library (F2.8)", async ({ page }, info) => {
    await page.goto("/?fixture=first-run");
    const ob = page.getByTestId("onboarding");
    await expect(ob).toHaveAttribute("data-step", "welcome");
    await expect(ob.getByTestId("onboarding-title")).toBeFocused();
    await shot(page, "onboarding-welcome", info);
    await tabTo(page, "onboarding-next");
    await page.keyboard.press("Enter");
    await expect(ob).toHaveAttribute("data-step", "legal");
    await tabTo(page, "legal-ack");
    await page.keyboard.press("Enter");
    await expect(ob).toHaveAttribute("data-step", "storage");
    await shot(page, "onboarding-storage", info);
    await tabTo(page, "onboarding-next");
    await page.keyboard.press("Enter");
    await expect(ob).toHaveAttribute("data-step", "updates");
    await tabTo(page, "onboarding-next");
    await page.keyboard.press("Enter");
    await expect(ob).toHaveAttribute("data-step", "theme");
    await tabTo(page, "theme-dark");
    await page.keyboard.press("ArrowDown");
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
    await tabTo(page, "onboarding-next");
    await expect(page.getByTestId("onboarding-next")).toHaveText("Start using VideoVault");
    await page.keyboard.press("Enter");
    await expect(ob).toHaveCount(0);
    await expect(page.getByTestId("library-card").first()).toBeVisible();
  });

  test("Skip goes straight to the library with the defaults", async ({ page }) => {
    await page.goto("/?fixture=first-run");
    await page.getByTestId("onboarding-skip").click();
    await expect(page.getByTestId("onboarding")).toHaveCount(0);
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
    await expect(page.getByTestId("library-card").first()).toBeVisible();
  });
});
