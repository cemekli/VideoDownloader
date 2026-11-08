import { expect, test, type Page, type TestInfo } from "@playwright/test";

// STATE.md F2.4 against `vite --mode fixture`: the player plays the real
// playback-matrix files (tests/fixtures/media, served under /fixture-media/).
// The vvmedia protocol and WebView2 itself are tested in the real app
// (e2e-desktop/playback-matrix.spec.ts, tests/media_protocol.rs).

const shot = (page: Page, name: string, info: TestInfo) => page.screenshot({ path: `../../test-artifacts/screenshots/player-${name}-${info.project.name}.png` });

async function openPlayer(page: Page, idEnd: string) {
  await page.goto("/");
  await page.locator(`[data-testid="library-card"][data-id$="${idEnd}"]`).dblclick();
  await page.getByTestId("inspector").getByTestId("inspector-play").click();
  const player = page.getByTestId("player");
  await expect(player).toBeVisible();
  return player;
}

const time = (page: Page) => page.getByTestId("player-video").evaluate((v: HTMLVideoElement) => v.currentTime);

test.describe("player", () => {
  test("resumes at the saved position and seeks within 500 ms (PLAY-01, PLAY-02, NFR-10)", async ({ page }, info) => {
    const player = await openPlayer(page, "0005");
    await expect(player.getByTestId("player-resume-button")).toHaveText("Resume from 0:02");
    await expect(player.getByTestId("player-resume-button")).toBeFocused();
    await shot(page, "resume", info);
    await page.keyboard.press("Enter");
    const video = player.getByTestId("player-video");
    await expect(video).toBeFocused();
    // Pause right away so the position under test does not run on.
    await page.keyboard.press("k");
    await expect.poll(() => video.evaluate((v: HTMLVideoElement) => v.paused)).toBe(true);
    expect(Math.abs((await time(page)) - 2)).toBeLessThanOrEqual(0.5);
    // A picture, not only sound (ADR-0004: H.264 plays).
    expect(await video.evaluate((v: HTMLVideoElement) => v.videoWidth)).toBeGreaterThan(0);
    await expect(player.getByTestId("player-problem")).toHaveCount(0);

    await page.keyboard.press("Home");
    await expect.poll(() => time(page)).toBeLessThanOrEqual(0.5);
    await page.keyboard.press("ArrowRight");
    await expect.poll(async () => Math.abs((await time(page)) - 3)).toBeLessThanOrEqual(0.5);
    await page.keyboard.press("ArrowLeft");
    await expect.poll(() => time(page)).toBeLessThanOrEqual(0.5);

    await page.keyboard.press("m");
    expect(await video.evaluate((v: HTMLVideoElement) => v.muted)).toBe(true);
    await page.keyboard.press("Escape");
    await expect(player).toHaveCount(0);
  });

  test("start over plays from the beginning and closing keeps the library usable", async ({ page }) => {
    const player = await openPlayer(page, "0005");
    await player.getByTestId("player-start-over").click();
    const video = player.getByTestId("player-video");
    await expect.poll(() => video.evaluate((v: HTMLVideoElement) => !v.paused && v.currentTime > 0)).toBe(true);
    await expect(player.getByTestId("player-resume")).toHaveCount(0);
    await player.getByTestId("player-close").click();
    await expect(player).toHaveCount(0);
    await expect(page.getByTestId("inspector-play")).toBeFocused();

    // Under 5 s watched there is nothing to resume: Play starts at once.
    await page.getByTestId("inspector-play").click();
    await expect(player.getByTestId("player-resume")).toHaveCount(0);
    await expect.poll(() => player.getByTestId("player-video").evaluate((v: HTMLVideoElement) => !v.paused && v.currentTime > 0)).toBe(true);
  });

  test("HEVC: a warning first, then the outcome is named and another player offered (AC-32, PLAY-04)", async ({ page }, info) => {
    const player = await openPlayer(page, "0001");
    // The warning shows until the element reports what it did.
    await expect(player.getByTestId("player-codec-warning").or(player.getByTestId("player-problem"))).toBeVisible();
    await expect(player.getByTestId("player-open-external")).toBeVisible();
    await player.getByTestId("player-video").evaluate((v: HTMLVideoElement) => {
      v.muted = true;
      void v.play().catch(() => {});
    });
    // Chromium shows HEVC as sound only ("partial") or not at all ("failed"),
    // depending on the build; it must never look like it simply worked.
    const outcome = await player.evaluate(
      (p) =>
        new Promise<string>((done) => {
          const end = Date.now() + 8_000;
          const tick = () => {
            const problem = p.querySelector<HTMLElement>("[data-testid=player-problem]")?.dataset.problem;
            const v = p.querySelector("video")!;
            if (problem) return done(problem);
            if (v.videoWidth > 0 && v.currentTime > 0.3) return done("plays");
            if (Date.now() > end) return done("timeout");
            setTimeout(tick, 100);
          };
          tick();
        }),
    );
    if (outcome === "plays") test.info().annotations.push({ type: "codec", description: "this Chromium decodes HEVC; the warning stays" });
    else {
      expect(["partial", "failed"]).toContain(outcome);
      await expect(player.getByTestId("player-problem")).toContainText("CODEC_UNSUPPORTED");
    }
    await shot(page, "hevc", info);
    await player.getByTestId("player-open-external").click();
    await expect(page.getByTestId("toast").last()).toContainText("Opened in your default player.");
  });

  test("a file on a drive that is away explains itself and can be located (DATA-03)", async ({ page }) => {
    const player = await openPlayer(page, "0007");
    await expect(player.getByTestId("player-unavailable")).toBeVisible();
    await expect(player.getByTestId("player-relink")).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("toast").last()).toContainText("File found again.");
  });
});
