import { expect, test } from "@playwright/test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { launchDesktopApp, repoRoot, type DesktopApp } from "./app";
import { BASELINE, probe, verdict, type MediaEntry, type Probe } from "./playback";

// STATE.md F0.8a / SPEC DEC-04: which container/codec pairs play in the real
// WebView2, loaded through the app's own media protocol (ADR-0004).
//
// Every file in tests/fixtures/media/manifest.json is measured and written to
// test-artifacts/playback-matrix/. Only the MVP baseline (MP4, H.264/AAC) is a
// hard requirement (SPEC §7.6 "Doğrulama kapısı"); the rest is a measurement,
// because it depends on the WebView2 runtime, Windows codec extensions and GPU.

test.skip(process.platform !== "win32", "Playback matrix runs in the real WebView2 on Windows only (ADR-0004)");
test.describe.configure({ mode: "serial" });

const mediaDir = join(repoRoot, "tests", "fixtures", "media");
const manifest = JSON.parse(readFileSync(join(mediaDir, "manifest.json"), "utf8")) as { files: MediaEntry[] };

let app: DesktopApp;
const rows: (MediaEntry & { probe: Probe; verdict: string })[] = [];

test.beforeAll(async () => {
  app = await launchDesktopApp({ env: { VV_E2E_MEDIA_DIR: mediaDir } });
});

test.afterAll(async () => {
  if (app && rows.length) {
    const runtime = await app.page.evaluate(() => navigator.userAgent);
    const webview2 = /Edg\/([\d.]+)/.exec(runtime)?.[1] ?? "unknown";
    const out = join(repoRoot, "test-artifacts", "playback-matrix");
    mkdirSync(out, { recursive: true });
    const report = { measuredAt: new Date().toISOString(), webview2, userAgent: runtime, ci: !!process.env.CI, rows };
    writeFileSync(join(out, "matrix.json"), JSON.stringify(report, null, 2) + "\n");
    const md = [
      `# Playback matrix — WebView2 ${webview2}`,
      "",
      `${report.measuredAt}${report.ci ? " (CI)" : ""}`,
      "",
      "| File | canPlayType | Verdict | Size | Frames | Audio bytes | Seek → | First frame | Error |",
      "|---|---|---|---|---|---|---|---|---|",
      ...rows.map(
        (r) =>
          `| ${r.id} | ${r.probe.canPlayType || "\"\""} | ${r.verdict} | ${r.probe.width}x${r.probe.height} | ${r.probe.framesDecoded} | ${r.probe.audioBytesDecoded} | ${r.probe.seekLandedS ?? "-"} | ${r.probe.firstFrameMs ?? "-"} ms | ${r.probe.error ?? ""} |`,
      ),
      "",
    ].join("\n");
    writeFileSync(join(out, "matrix.md"), md);
    console.log(`\n${md}`);
  }
  await app?.close();
});

test("manifest covers every container and codec named in DEC-04", () => {
  const containers = new Set(manifest.files.map((f) => f.container));
  const codecs = new Set(manifest.files.map((f) => f.video));
  for (const c of ["mp4", "webm", "mkv", "mov"]) expect(containers).toContain(c);
  for (const c of ["h264", "hevc", "vp9", "av1"]) expect(codecs).toContain(c);
});

for (const entry of manifest.files) {
  test(`measure ${entry.id}`, async () => {
    // On Windows a custom Tauri scheme is served as http://<scheme>.localhost/.
    const p = await probe(app.page, `http://vvmedia.localhost/${entry.file}`, entry.mime);
    const v = verdict(p);
    rows.push({ ...entry, probe: p, verdict: v });
    test.info().annotations.push({ type: "verdict", description: `${v} (canPlayType "${p.canPlayType}")` });
    if (entry.id === BASELINE) {
      // MVP exit condition: the base test format plays, decodes audio and seeks.
      expect(p, JSON.stringify(p)).toMatchObject({ loaded: true, played: true, seeked: true, playsAfterSeek: true, width: 320, height: 180 });
      expect(p.framesDecoded).toBeGreaterThan(0);
      expect(p.audioBytesDecoded).toBeGreaterThan(0);
      expect(Math.abs((p.durationS ?? 0) - entry.durationS)).toBeLessThan(0.2);
      expect(v).toBe("plays");
    }
  });
}
