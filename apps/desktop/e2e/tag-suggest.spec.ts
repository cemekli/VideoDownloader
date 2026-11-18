import { expect, test, type Page, type TestInfo } from "@playwright/test";

// STATE.md F7.3, SPEC AI-B01/B02 against `vite --mode fixture`: the chat
// API is set in AI Processing; an item's details then show suggested tags
// that change nothing until one is accepted (it becomes a tag) or rejected
// (it is gone). The rules themselves are tested in Rust
// (crates/vv-engine/tests/suggest.rs, src-tauri/tests/ipc.rs).

const shot = (page: Page, name: string, info: TestInfo) => page.screenshot({ path: `../../test-artifacts/screenshots/tag-suggest-${name}-${info.project.name}.png` });

async function openNav(page: Page, info: TestInfo, id: string) {
  if (info.project.name === "narrow-800") await page.getByTestId("menu-button").click();
  await page.getByTestId(id).click();
}

test("suggested tags wait in the details until accepted or rejected (F7.3)", async ({ page }, info) => {
  await page.goto("/");
  await openNav(page, info, "nav-ai");
  const block = page.getByTestId("tag-suggest");
  await expect(block.getByTestId("suggest-api-model")).toHaveValue("qwen3.5:4b");
  await block.getByTestId("suggest-api-save").click();
  await expect(block.getByTestId("suggest-api-summary")).toContainText("qwen3.5:4b at http://localhost:11434/v1");
  await shot(page, "settings", info);

  await openNav(page, info, "nav-all");
  await page.locator('[data-testid="library-card"][data-id$="0002"]').dblclick();
  const box = page.getByTestId("inspector").getByTestId("inspector-suggestions");
  await box.getByTestId("suggestion-ask").click();
  const chips = box.getByTestId("suggestion");
  await expect(chips).toHaveCount(3);
  const tags = page.getByTestId("inspector").getByTestId("inspector-tags");
  await expect(tags).not.toContainText("#tarif");
  await shot(page, "item", info);

  await box.getByRole("button", { name: "Add tag tarif" }).click();
  await expect(tags).toContainText("#tarif");
  await box.getByRole("button", { name: "Reject suggestion kısa" }).click();
  await expect(chips).toHaveCount(1);
  await expect(tags).not.toContainText("#kısa");
});
