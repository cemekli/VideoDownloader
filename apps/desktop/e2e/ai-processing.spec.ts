import { expect, test, type Page, type TestInfo } from "@playwright/test";

// STATE.md F6.6 against `vite --mode fixture`, SPEC AI-A05 and AI-A06: the
// low-memory scenario (`?fixture=low-memory`, a 4 GB PC with Base and
// Medium installed). Medium is not picked by default and does not start
// without "Use Medium anyway"; AI Processing shows the PC, marks Medium,
// lists the running transcription and cancels it. The memory rule and the
// job's refusal are tested in Rust (crates/vv-engine/tests/low_memory.rs).

const shot = (page: Page, name: string, info: TestInfo) => page.screenshot({ path: `../../test-artifacts/screenshots/ai-processing-${name}-${info.project.name}.png` });

async function openNav(page: Page, info: TestInfo, id: string) {
  if (info.project.name === "narrow-800") await page.getByTestId("menu-button").click();
  await page.getByTestId(id).click();
}

test.describe("Low memory and AI Processing (F6.6)", () => {
  test("the big model needs 'use it anyway', and the queue shows and cancels it", async ({ page }, info) => {
    await page.goto("/?fixture=low-memory");
    await page.locator('[data-testid="library-card"][data-id$="0005"]').dblclick();
    const section = page.getByTestId("inspector").getByTestId("inspector-transcripts");
    const select = section.getByTestId("transcribe-model");
    await expect(select).toHaveValue("whisper-base");
    await select.selectOption("whisper-medium");
    const warn = section.getByTestId("transcribe-memory");
    await expect(warn).toContainText("this PC has");
    await expect(section.getByTestId("transcribe-start")).toBeDisabled();
    await shot(page, "warning", info);
    await section.getByTestId("transcribe-anyway").check();
    await section.getByTestId("transcribe-start").click();
    await expect(section.getByTestId("transcribe-running")).toContainText("Medium");

    if (info.project.name === "narrow-800") await page.keyboard.press("Escape");
    await openNav(page, info, "nav-ai");
    const view = page.getByTestId("ai-processing");
    await expect(view.getByTestId("ai-memory")).toContainText("free of 4");
    const medium = view.locator('[data-testid="model-row"][data-model="whisper-medium"]');
    await expect(medium.getByTestId("model-fit")).toContainText("more than this PC has");
    const job = view.getByTestId("ai-job");
    await expect(job).toHaveAttribute("data-status", "running");
    await shot(page, "queue", info);
    await job.getByTestId("ai-job-cancel").click();
    await expect(job).toHaveAttribute("data-status", "canceled");

    await view.getByTestId("ai-cpu-light").check();
    await expect(view.getByTestId("ai-threads")).toContainText("Uses 2 of 8 threads");
  });
});
