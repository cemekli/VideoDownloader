import { expect, test, type Page, type TestInfo } from "@playwright/test";

// STATE.md F5.5 (SPEC §9.3) against `vite --mode fixture` with the
// `login-required` scenario: a download that needs a login waits, the user
// connects the site on the Sessions screen with explicit consent, the
// download finishes, and Check now proves the session (§12.1). Cookie
// reading, the working copy and the evidence rule are tested in Rust
// (crates/vv-engine/tests/engine.rs `f5_*`).

const shot = (page: Page, name: string, info: TestInfo) => page.screenshot({ path: `../../test-artifacts/screenshots/sessions-${name}-${info.project.name}.png` });

async function nav(page: Page, info: TestInfo, testId: string) {
  if (info.project.name === "narrow-800" && !(await page.getByTestId(testId).isVisible())) await page.getByTestId("menu-button").click();
  await page.getByTestId(testId).click();
}

test("a download that needs a login: connect on the Sessions screen, it finishes, Check now proves it", async ({ page }, info) => {
  await page.goto("/?fixture=login-required");
  await expect(page.getByTestId("library-card").first()).toBeVisible();
  // Instagram video (fixture item 0002) waits for a login.
  await page.locator('[data-testid="library-card"][data-id$="0002"]').dblclick();
  await page.getByTestId("inspector-download").click();
  await page.getByTestId("download-dialog").getByTestId("download-start").click();
  await expect(page.getByTestId("download-dialog")).toBeHidden();
  if (info.project.name === "narrow-800") await page.keyboard.press("Escape");
  await nav(page, info, "nav-downloads");
  await expect(page.getByTestId("job")).toHaveAttribute("data-status", "blocked", { timeout: 10_000 });

  await nav(page, info, "nav-sessions");
  const sessions = page.getByTestId("sessions");
  await expect(sessions.getByTestId("sessions-empty")).toBeVisible();
  await sessions.locator('[data-testid="session-site"][data-provider="instagram"]').getByTestId("session-site-connect").click();
  const dialog = page.getByTestId("session-dialog");
  await expect(dialog.getByTestId("session-provider")).toHaveValue("instagram");
  await expect(dialog.getByTestId("session-connect-submit")).toBeDisabled();
  await dialog.getByTestId("session-consent").check();
  await shot(page, "connect", info);
  await dialog.getByTestId("session-connect-submit").click();
  await expect(dialog).toBeHidden();

  const row = sessions.getByTestId("session");
  await expect(row).toHaveAttribute("data-provider", "instagram");
  await nav(page, info, "nav-downloads");
  await expect(page.getByTestId("downloads-done")).toBeVisible({ timeout: 10_000 });

  await nav(page, info, "nav-sessions");
  await expect(row).toHaveAttribute("data-state", "session_available");
  await row.getByTestId("session-verify").click();
  await expect(page.getByTestId("toast").last()).toContainText("the check passed");
  await expect(row.getByTestId("session-verified")).toContainText("checked");
  await shot(page, "screen", info);

  // Clear: the session stays listed but is not used; Reconnect brings it back.
  await row.getByTestId("session-clear").click();
  await expect(row).toHaveAttribute("data-state", "not_connected");
  await row.getByTestId("session-reconnect").click();
  await expect(row).toHaveAttribute("data-state", "needs_verification");
});

test("the Sessions screen scrolls when its sites do not fit the window", async ({ page }, info) => {
  await page.setViewportSize({ width: info.project.name === "narrow-800" ? 800 : 1366, height: 480 });
  await page.goto("/?fixture=login-required");
  await expect(page.getByTestId("library-card").first()).toBeVisible();
  await nav(page, info, "nav-sessions");
  const sessions = page.getByTestId("sessions");
  const last = sessions.getByTestId("session-site").last();
  await expect(last).toBeVisible();
  const fits = await sessions.evaluate((el) => el.scrollHeight <= el.clientHeight);
  expect(fits).toBe(false);
  await last.scrollIntoViewIfNeeded();
  const box = await last.boundingBox();
  expect(box && box.y + box.height).toBeLessThanOrEqual(480);
  expect(await sessions.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
});
