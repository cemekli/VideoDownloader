import { expect, test, type Page } from "@playwright/test";
import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { desktopExe, launchDesktopApp, type DesktopApp } from "./app";

// STATE.md F0.10, AC-38, INV-13, ADR-0008: a second VideoVault process never
// opens the library. Layer 1 (single-instance plugin): the second launch hands
// over to the first and exits 0. Layer 2 (library lock): with the plugin left
// out, the second process finds the lock held and exits 3.

test.skip(process.platform !== "win32", "Real-window E2E runs on Windows only (WebView2 + CDP, ADR-0001)");
test.describe.configure({ mode: "serial" });

type Diagnostics = { libraryLock: boolean; secondLaunches: number };
const diagnostics = (page: Page) =>
  page.evaluate(() =>
    (window as unknown as { __TAURI_INTERNALS__: { invoke: (c: string) => Promise<unknown> } }).__TAURI_INTERNALS__.invoke("get_diagnostics"),
  ) as Promise<Diagnostics>;

/** Starts a second process on the same data directory; resolves with its exit code. */
function secondLaunch(dataDir: string, extraEnv: Record<string, string> = {}): Promise<{ code: number | null; ms: number }> {
  const t = Date.now();
  return new Promise((done, fail) => {
    const p = spawn(desktopExe, ["https://example.com/watch?v=vv-e2e"], {
      env: { ...process.env, ...extraEnv, VV_DATA_DIR: dataDir, VV_E2E_CDP_PORT: "" },
      stdio: "ignore",
    });
    const timer = setTimeout(() => {
      p.kill();
      fail(new Error("second instance still running after 30 s: it did not exit"));
    }, 30_000);
    p.once("exit", (code) => {
      clearTimeout(timer);
      done({ code, ms: Date.now() - t });
    });
  });
}

let dataDir: string;
let app: DesktopApp | undefined;
test.beforeEach(() => {
  dataDir = mkdtempSync(join(tmpdir(), "vv-e2e-si-"));
});
test.afterEach(async () => {
  await app?.close();
  app = undefined;
  rmSync(dataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
});

test("second launch hands over to the running window and exits (AC-38)", async () => {
  app = await launchDesktopApp({ env: { VV_DATA_DIR: dataDir } });
  expect(await diagnostics(app.page)).toMatchObject({ libraryLock: true, secondLaunches: 0 });
  // Windows refuses reads of the locked range, so only its presence is checked.
  expect(existsSync(join(dataDir, "videovault.lock"))).toBe(true);

  const second = await secondLaunch(dataDir);
  console.log(`second instance exited with ${second.code} after ${second.ms} ms`);
  expect(second.code).toBe(0);
  await expect.poll(async () => (await diagnostics(app!.page)).secondLaunches, { timeout: 10_000 }).toBe(1);
  expect(app.exitCode()).toBeNull();
  await expect(app.page.getByRole("heading", { name: "Library" })).toBeVisible();
});

test("without the plugin the library lock still keeps a second process out (INV-13)", async () => {
  const env = { VV_DATA_DIR: dataDir, VV_E2E_NO_SINGLE_INSTANCE: "1" };
  app = await launchDesktopApp({ env });
  expect(await diagnostics(app.page)).toMatchObject({ libraryLock: true, secondLaunches: 0 });

  const second = await secondLaunch(dataDir, { VV_E2E_NO_SINGLE_INSTANCE: "1" });
  console.log(`second process exited with ${second.code} after ${second.ms} ms`);
  expect(second.code).toBe(3); // instance::EXIT_LIBRARY_LOCKED
  expect(app.exitCode()).toBeNull();
  expect(await diagnostics(app.page)).toMatchObject({ libraryLock: true, secondLaunches: 0 });
});

test("the lock is free again once the first process is gone", async () => {
  const first = await launchDesktopApp({ env: { VV_DATA_DIR: dataDir } });
  await first.close();
  app = await launchDesktopApp({ env: { VV_DATA_DIR: dataDir } });
  expect(await diagnostics(app.page)).toMatchObject({ libraryLock: true, secondLaunches: 0 });
});
