// Launches the real VideoVault executable and attaches Playwright to its
// WebView2 over the Chrome DevTools Protocol (docs/decisions.md ADR-0001).
//
// Windows only: WebView2 is Chromium-based and honours
// WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS. The debugging port is opened only for
// this test process, on 127.0.0.1, and the app gets a throwaway WebView2 profile.

import { chromium, type Browser, type Page } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const repoRoot = resolve(import.meta.dirname, "../../..");

/** Release build from `npm run desktop:exe`; override with VV_DESKTOP_EXE. */
export const desktopExe = process.env.VV_DESKTOP_EXE ?? join(repoRoot, "target", "release", "videovault.exe");

export interface DesktopApp {
  page: Page;
  browser: Browser;
  proc: ChildProcess;
  close(): Promise<void>;
}

function freePort(): Promise<number> {
  return new Promise((ok, fail) => {
    const s = createServer();
    s.once("error", fail);
    s.listen(0, "127.0.0.1", () => {
      const { port } = s.address() as { port: number };
      s.close(() => ok(port));
    });
  });
}

async function waitFor<T>(what: string, timeoutMs: number, probe: () => Promise<T | undefined>): Promise<T> {
  const until = Date.now() + timeoutMs;
  let last: unknown;
  while (Date.now() < until) {
    try {
      const v = await probe();
      if (v !== undefined) return v;
    } catch (e) {
      last = e;
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`Timed out after ${timeoutMs} ms waiting for ${what}${last ? `: ${String(last)}` : ""}`);
}

export async function launchDesktopApp(): Promise<DesktopApp> {
  if (!existsSync(desktopExe)) throw new Error(`Desktop executable not found: ${desktopExe}. Run \`npm run desktop:exe\` first.`);

  const port = await freePort();
  const profile = mkdtempSync(join(tmpdir(), "vv-e2e-webview2-"));
  const proc = spawn(desktopExe, [], {
    env: {
      ...process.env,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
      WEBVIEW2_USER_DATA_FOLDER: profile,
    },
    stdio: "ignore",
  });
  let exited: number | null = null;
  proc.once("exit", (code) => (exited = code ?? -1));

  const close = async () => {
    if (exited === null) proc.kill();
    await new Promise((r) => setTimeout(r, 500));
    rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  };

  try {
    await waitFor("the WebView2 debugging endpoint", 30_000, async () => {
      if (exited !== null) throw new Error(`videovault.exe exited with code ${exited}`);
      const r = await fetch(`http://127.0.0.1:${port}/json/version`);
      return r.ok ? true : undefined;
    });
    const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
    const page = await waitFor("the main window page", 15_000, async () =>
      browser
        .contexts()
        .flatMap((c) => c.pages())
        .find((p) => /^(https?:\/\/tauri\.localhost|tauri:\/\/localhost)/.test(p.url())),
    );
    await page.waitForLoadState("domcontentloaded");
    return {
      page,
      browser,
      proc,
      close: async () => {
        await browser.close().catch(() => {});
        await close();
      },
    };
  } catch (e) {
    const diag = diagnostics(profile, exited);
    await close();
    throw new Error(`${(e as Error).message}\n--- diagnostics ---\n${diag}`);
  }
}

/** What WebView2 actually did, so a CI failure says why the port never opened. */
function diagnostics(profile: string, exited: number | null): string {
  const lines = [`app exited: ${exited === null ? "no (still running)" : `yes, code ${exited}`}`];
  const walk = (dir: string, depth: number): string[] =>
    depth > 3 ? [] : readdirSync(dir).flatMap((n) => {
      const p = join(dir, n);
      return statSync(p).isDirectory() ? walk(p, depth + 1) : [p];
    });
  try {
    const files = walk(profile, 0);
    lines.push(`profile files: ${files.length}`);
    const port = files.find((f) => f.endsWith("DevToolsActivePort"));
    lines.push(port ? `DevToolsActivePort: ${readFileSync(port, "utf8").replace(/\s+/g, " ")}` : "DevToolsActivePort: not found");
    lines.push(...files.slice(0, 15).map((f) => `  ${f}`));
  } catch (e) {
    lines.push(`profile unreadable: ${String(e)}`);
  }
  if (process.platform === "win32") {
    const ps = spawnSync(
      "powershell",
      ["-NoProfile", "-Command", "Get-CimInstance Win32_Process -Filter \"Name='msedgewebview2.exe' or Name='videovault.exe'\" | Select-Object -First 4 ProcessId,Name,CommandLine | Format-List | Out-String -Width 400"],
      { encoding: "utf8" },
    );
    lines.push(ps.stdout || ps.stderr || "no process info");
  }
  return lines.join("\n");
}
