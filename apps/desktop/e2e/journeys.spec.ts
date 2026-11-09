import { expect, test, type Page, type TestInfo } from "@playwright/test";

// SPEC §25 critical journeys against `vite --mode fixture`. Journey 1 (save
// in five seconds) is in add-url.spec.ts. Here: Journey 2, find an old
// bookmark (F1 exit gate), Journey 5, organize a large Inbox (F2 exit
// gate), and Journey 6, recover after an error (F3.8). Restart recovery of
// Journey 6 (leases, checkpoints, a move killed at every step) needs the
// real engine and database and is tested in Rust (crates/vv-engine/tests). The same steps against the real app and database are in
// e2e-desktop (add-url, organize, offline-flow).

const shot = (page: Page, name: string, info: TestInfo) => page.screenshot({ path: `../../test-artifacts/screenshots/journey-${name}-${info.project.name}.png` });
const cards = (page: Page) => page.getByTestId("library-card");
const lastToast = (page: Page) => page.getByTestId("toast").last();

test.describe("Journey 2: find an old bookmark", () => {
  test("a remembered word, a filter, the details, then the source page", async ({ page }, info) => {
    await page.goto("/");
    await expect(cards(page).first()).toBeVisible();
    // Typed as remembered: upper case, Turkish letters folded (SEA-01).
    await page.keyboard.press("Control+k");
    await page.keyboard.type("EKMEK");
    await expect(cards(page)).toHaveCount(1);
    await expect(cards(page).first().getByRole("heading")).toHaveText("Kısa tarif: ev yapımı ekmek");
    await page.getByTestId("chip-kind-bookmark").click();
    await expect(cards(page)).toHaveCount(1);
    await shot(page, "2-found", info);

    // Keyboard from the search box into the result and its details.
    await cards(page).first().focus();
    await page.keyboard.press("Enter");
    const inspector = page.getByTestId("inspector");
    await expect(inspector.getByTestId("inspector-title")).toHaveText("Kısa tarif: ev yapımı ekmek");
    await expect(inspector.getByTestId("inspector-play")).toHaveCount(0);
    await inspector.getByTestId("inspector-open-source").click();
    await expect(lastToast(page)).toContainText("Opened the source page in your browser.");
  });

  test("words from the notes find it too", async ({ page }) => {
    await page.goto("/");
    await page.getByTestId("search-input").fill("first day");
    await expect(cards(page)).toHaveCount(1);
    await expect(cards(page).first().getByRole("heading")).toHaveText("Holiday clip 🎉");
    await expect(page.getByTestId("snippet")).toHaveAttribute("data-field", "notes");
  });

  test("a wrong word says so and one step brings everything back", async ({ page }, info) => {
    await page.goto("/");
    await page.getByTestId("search-input").fill("ekmke");
    const none = page.getByTestId("library-no-match");
    await expect(none).toContainText("No results for “ekmke”");
    await shot(page, "2-no-result", info);
    await none.getByRole("button", { name: "Clear search" }).click();
    await expect(cards(page)).toHaveCount(7);
    await expect(page.getByTestId("search-input")).toHaveValue("");
  });
});

test.describe("Journey 5: organize a large Inbox", () => {
  test("select 20 by keyboard, tag, undo, archive, undo; the selection stays (AC-07)", async ({ page }, info) => {
    await page.goto("/?fixture=many");
    await expect(cards(page).first()).toBeVisible();
    const count = page.getByTestId("result-count");
    const before = (await count.textContent())!;

    await cards(page).first().click();
    for (let i = 0; i < 19; i++) await page.keyboard.press("Shift+ArrowRight");
    await expect(count).toContainText("20 selected");
    await expect(page.getByTestId("bulk-bar")).toBeVisible();
    await shot(page, "5-selected", info);

    await page.getByTestId("bulk-tag").click();
    await page.keyboard.insertText("izlenecek");
    await page.keyboard.press("Enter");
    await expect(lastToast(page)).toContainText("Tagged 20 items with #izlenecek.");
    await expect(count).toContainText("20 selected");
    await lastToast(page).getByTestId("toast-undo").click();
    await expect(lastToast(page)).toContainText("Tagging undone.");
    await expect(count).toContainText("20 selected");

    await page.getByTestId("bulk-archive").click();
    await expect(lastToast(page)).toContainText("Archived 20 items.");
    await expect(count).not.toHaveText(before);
    await shot(page, "5-archived", info);
    await page.keyboard.press("Control+z");
    await expect(lastToast(page)).toContainText("Archive change undone.");
    await expect(count).toHaveText(before);
  });
});

test.describe("Journey 6: recover after an error", () => {
  test("a download fails on the network, keeps its partial file, and Try again finishes it; the file plays", async ({ page }, info) => {
    const narrow = info.project.name === "narrow-800";
    const nav = async (id: string) => {
      if (narrow) await page.getByTestId("menu-button").click();
      await page.getByTestId(id).click();
    };
    await page.goto("/?fixture=download-error");
    await expect(cards(page).first()).toBeVisible();
    await page.locator('[data-testid="library-card"][data-id$="0002"]').dblclick();
    const inspector = page.getByTestId("inspector");
    await inspector.getByTestId("inspector-download").click();
    const dialog = page.getByTestId("download-dialog");
    await dialog.getByTestId("preset-720").check();
    await dialog.getByTestId("download-start").click();
    await expect(lastToast(page)).toContainText("Download started.");
    if (narrow) await page.keyboard.press("Escape");
    await nav("nav-downloads");

    // The error is a state the user can act on, with the partial file kept.
    const failed = page.getByTestId("downloads-failed");
    await expect(failed.getByTestId("job-error")).toContainText("Can't reach the internet");
    await expect(failed.getByTestId("job-partial")).toContainText("50 MB");
    await expect(failed.getByTestId("job-status")).toHaveText("Failed");
    await shot(page, "6-failed", info);

    await failed.getByTestId("job-retry").click();
    const active = page.getByTestId("downloads-active");
    await expect(active.getByTestId("job-progress")).toHaveAttribute("aria-valuenow", "50");
    await shot(page, "6-progress", info);
    const done = page.getByTestId("downloads-done");
    await expect(done.getByTestId("job-status")).toHaveText("Done");
    await expect(page.getByTestId("downloads-failed")).toHaveCount(0);

    // The item is Downloaded and its file opens in the player.
    await nav("nav-all");
    const card = page.locator('[data-testid="library-card"][data-id$="0002"]');
    await expect(card).toContainText("Downloaded");
    await card.dblclick();
    await page.getByTestId("inspector").getByTestId("inspector-play").click();
    await expect(page.getByTestId("player")).toBeVisible();
    await shot(page, "6-playing", info);
  });
});
