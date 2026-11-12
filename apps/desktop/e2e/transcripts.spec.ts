import { expect, test, type Page, type TestInfo } from "@playwright/test";

// STATE.md F6.2 against `vite --mode fixture`: Details → Transcript. The
// 11 Nov 2026 bug left the panel on "Getting…" for minutes after the job's
// thread panicked. Here a failed job must end with a message and a way to
// try again, and the Turkish lines of the self-authored YouTube-like file
// (tests/fixtures/captions/youtube-tr-manual.vtt: `>>` speaker marks,
// entities, emoji) must show exactly, with timecodes. The job itself, with
// the same file through the fake and the real yt-dlp, is tested in Rust
// (crates/vv-engine/tests/captions.rs, real_ytdlp.rs).

const shot = (page: Page, name: string, info: TestInfo) => page.screenshot({ path: `../../test-artifacts/screenshots/transcripts-${name}-${info.project.name}.png` });

async function openDetails(page: Page, fixture: string, idEnd: string) {
  await page.goto(`/?fixture=${fixture}`);
  await page.locator(`[data-testid="library-card"][data-id$="${idEnd}"]`).dblclick();
  const section = page.getByTestId("inspector").getByTestId("inspector-transcripts");
  await expect(section).toBeVisible();
  return section;
}

test.describe("Details → Transcript", () => {
  test("a failed captions job ends with a reason; trying again brings the Turkish lines intact", async ({ page }, info) => {
    const section = await openDetails(page, "captions-error", "0001");
    await expect(section).toContainText("No transcript yet");
    await section.getByTestId("captions-fetch").click();
    await expect(section.getByTestId("captions-running")).toBeVisible();

    // The job ends: no endless "Getting…", a reason, and the button is back.
    await expect(section.getByTestId("captions-running")).toHaveCount(0, { timeout: 5_000 });
    await expect(section.getByTestId("captions-failed")).toContainText("Could not get the captions");
    await expect(section.getByTestId("captions-fetch")).toHaveText("Check site captions again");
    await shot(page, "failed", info);

    await section.getByTestId("captions-fetch").click();
    const row = section.getByTestId("transcript-row");
    await expect(row).toContainText("Turkish", { timeout: 5_000 });
    await expect(row).toContainText("Site captions (uploader)");
    await expect(row).toContainText("8 lines");
    await expect(section.getByTestId("captions-failed")).toHaveCount(0);

    await row.getByTestId("transcript-show").click();
    const lines = section.getByTestId("transcript-lines").getByRole("listitem");
    await expect(lines).toHaveCount(8);
    await expect(lines.nth(0)).toContainText("0:00");
    await expect(lines.nth(0)).toContainText(">> Merhaba arkadaşlar, bugün İstanbul'dayız.");
    await expect(lines.nth(3)).toContainText(">> Ayşe: Çığlık atmayın, kuşlar ürküyor! >> Güldüğümü görüyorsun değil mi?");
    await expect(lines.nth(4)).toContainText("Iğdır, Iğdır… İzmir, ılık, IŞIK, ışık.");
    await expect(lines.nth(5)).toContainText("Fiyatı 25 ₺ — “çok ucuz” dedi & gitti.");
    // `&lt;yarın&gt;` is text, not a tag.
    await expect(lines.nth(6)).toContainText("Öğretmen: Ödevleri <yarın> getirin.");
    await expect(lines.nth(7)).toContainText("0:21");
    await expect(lines.nth(7)).toContainText("😀 Teşekkürler! 👍🏽 Görüşmek üzere 🇹🇷");
    await shot(page, "lines", info);

    await row.getByTestId("transcript-show").click();
    await expect(section.getByTestId("transcript-lines")).toHaveCount(0);
    await row.getByTestId("transcript-delete").click();
    await expect(section.getByTestId("transcript-row")).toHaveCount(0);
  });

  test("a site without captions says so and the job still ends", async ({ page }) => {
    const section = await openDetails(page, "default", "0002");
    await section.getByTestId("captions-fetch").click();
    await expect(section.getByTestId("captions-none")).toBeVisible({ timeout: 5_000 });
    await expect(section.getByTestId("captions-running")).toHaveCount(0);
    await expect(section.getByTestId("transcript-row")).toHaveCount(0);
  });

  test("a local file has no site captions to get", async ({ page }) => {
    const section = await openDetails(page, "default", "0005");
    await expect(section.getByTestId("captions-fetch")).toHaveCount(0);
  });
});

// STATE.md F6.4 (SEA-06, PLAY-06, AC-22): words said in the video find the
// item; the moment plays from its segment's start.
test.describe("Search in transcripts", () => {
  test("a spoken word finds the item and plays from that moment", async ({ page }, info) => {
    await page.goto("/?fixture=transcripts");
    await page.getByTestId("search-input").fill("Kadıköy");
    const card = page.getByTestId("library-card");
    await expect(card).toHaveCount(1);
    await expect(card.getByTestId("snippet")).toHaveAttribute("data-field", "transcript");
    await expect(card.getByTestId("snippet")).toContainText("0:03");
    await shot(page, "search-card", info);
    await card.dblclick();
    const moments = page.getByTestId("inspector").getByTestId("inspector-moments");
    await expect(moments).toContainText("Said in the video");
    await shot(page, "search-moments", info);
    await moments.getByRole("button", { name: /Play from 0:03/ }).click();
    await expect(page.getByTestId("player-video")).toHaveAttribute("data-start-ms", "3040");
  });
});
