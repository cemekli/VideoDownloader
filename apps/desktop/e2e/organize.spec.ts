import { expect, test, type Page, type TestInfo } from "@playwright/test";

// STATE.md F2.1–F2.3 against `vite --mode fixture` at 1366×768, 1920×1080
// and 800 px: bulk tagging with Undo, collections, Recently removed and a
// previewed duplicate merge. The database side is tested in Rust.

const shot = (page: Page, name: string, info: TestInfo) =>
  page.screenshot({
    path: `../../test-artifacts/screenshots/organize-${name}-${info.project.name}.png`,
  });

async function ready(page: Page) {
  await page.goto("/");
  await expect(page.getByTestId("library-card").first()).toBeVisible();
}

/** Opens the navigation drawer in the narrow layout. */
async function nav(page: Page, info: TestInfo) {
  if (info.project.name === "narrow-800") await page.getByTestId("menu-button").click();
}

const lastToast = (page: Page) => page.getByTestId("toast").last();

test.describe("organize", () => {
  test("bulk tag 7 items, Undo from the toast, then Ctrl+Z after archiving (AC-07)", async ({ page }, info) => {
    await ready(page);
    await page.getByTestId("library-card").first().click();
    await page.keyboard.press("Control+a");
    await expect(page.getByTestId("result-count")).toContainText("7 selected");
    await expect(page.getByTestId("bulk-bar")).toBeVisible();
    await shot(page, "bulk-bar", info);
    await page.getByTestId("bulk-tag").click();
    await page.keyboard.insertText("deniz");
    await page.keyboard.press("Enter");
    await expect(lastToast(page)).toContainText("Tagged 7 items with #deniz.");
    await shot(page, "undo-toast", info);
    await lastToast(page).getByTestId("toast-undo").click();
    await expect(lastToast(page)).toContainText("Tagging undone.");

    await page.getByTestId("bulk-archive").click();
    await expect(page.getByTestId("result-count")).toHaveText("0 items");
    await page.keyboard.press("Control+z");
    await expect(page.getByTestId("result-count")).toHaveText("7 items");
  });

  test("collections tree with a keyboard menu (ORG-08)", async ({ page }, info) => {
    await ready(page);
    await nav(page, info);
    await page.getByTestId("new-collection").click();
    await page.keyboard.insertText("Gezi");
    await page.keyboard.press("Enter");
    await nav(page, info);
    const row = page.getByTestId("collection-row").filter({ hasText: "Gezi" });
    await expect(row).toBeVisible();
    // Keyboard: open the row menu, arrow to "New sub-collection…".
    await row.getByTestId("collection-menu").focus();
    await page.keyboard.press("Enter");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
    await page.keyboard.insertText("Deniz");
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("collection-row")).toHaveCount(2);
    if (info.project.name === "narrow-800") await page.keyboard.press("Escape");

    await page.getByTestId("library-card").first().click();
    await page.getByTestId("bulk-collect").click();
    await page.getByTestId("pick-submit").click();
    await expect(lastToast(page)).toContainText("Added 1 item to Gezi.");
    await nav(page, info);
    await shot(page, "collections", info);
    await page.getByTestId("nav-collection").first().click();
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Gezi");
    await expect(page.getByTestId("result-count")).toHaveText("1 item");
  });

  test("Recently removed and a previewed duplicate merge (ORG-11)", async ({ page }, info) => {
    await ready(page);
    await page.getByTestId("library-card").nth(1).click();
    await page.keyboard.press("Delete");
    await expect(lastToast(page)).toContainText("Moved 1 item to Recently removed.");
    await nav(page, info);
    await page.getByTestId("nav-removed").click();
    await expect(page.getByTestId("result-count")).toHaveText("2 items");
    await page.getByTestId("library-card").first().click();
    await expect(page.getByTestId("bulk-delete")).toBeVisible();
    await shot(page, "removed", info);

    await nav(page, info);
    await page.getByTestId("nav-duplicates").click();
    await page.getByTestId("dup-keep").first().click();
    await expect(page.getByTestId("merge-preview")).toContainText("is kept.");
    await shot(page, "merge-preview", info);
    await page.getByTestId("merge-confirm").click();
    await expect(page.getByTestId("duplicates-empty")).toBeVisible();
  });
});
