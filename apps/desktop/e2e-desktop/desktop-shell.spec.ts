import { expect, test } from "@playwright/test";
import { launchDesktopApp, type DesktopApp } from "./app";

// STATE.md F0.4 / SPEC §25.1: end-to-end tests against the real Windows app.
// Unlike e2e/ (Vite + fixture client) these go through the real WebView2,
// the real Tauri IPC bridge, the real capability (ACL) file and the real CSP.

test.skip(process.platform !== "win32", "Real-window E2E runs on Windows only (WebView2 + CDP, ADR-0001)");
test.describe.configure({ mode: "serial" });

let app: DesktopApp;
test.beforeAll(async () => {
  app = await launchDesktopApp();
});
test.afterAll(async () => {
  await app?.close();
});

test("window shows real backend status, not fixture data", async () => {
  const { page } = app;
  await expect(page.getByRole("heading", { name: "Library" })).toBeVisible();
  // get_status answered over real IPC; fixture mode would say "Fixture data".
  await expect(page.getByTestId("backend-status")).toHaveText(/^Desktop v\d+\.\d+\.\d+$/);
  await expect(page.getByTestId("backend-status")).toHaveAttribute("data-backend", "desktop");
  await expect(page.getByTestId("library-empty")).toBeVisible();
  await page.screenshot({ path: "../../test-artifacts/screenshots/desktop-windows.png" });
});

test("ping round-trips through the real IPC bridge", async () => {
  const pong = await app.page.evaluate(() =>
    (window as unknown as { __TAURI_INTERNALS__: { invoke: (c: string, a?: unknown) => Promise<unknown> } }).__TAURI_INTERNALS__.invoke("ping", { nonce: 7 }),
  );
  expect(pong).toEqual({ nonce: 7, contractVersion: 1 });
});

test("commands outside the capability are refused (least privilege)", async () => {
  const results = await app.page.evaluate(async () => {
    const invoke = (window as unknown as { __TAURI_INTERNALS__: { invoke: (c: string, a?: unknown) => Promise<unknown> } }).__TAURI_INTERNALS__.invoke;
    const attempt = async (cmd: string, args?: unknown) => {
      try {
        await invoke(cmd, args);
        return `${cmd}: allowed`;
      } catch {
        return `${cmd}: refused`;
      }
    };
    return [await attempt("plugin:window|close", { label: "main" }), await attempt("plugin:app|version"), await attempt("read_file", { path: "C:/Windows/win.ini" })];
  });
  expect(results).toEqual(["plugin:window|close: refused", "plugin:app|version: refused", "read_file: refused"]);
});

test("CSP blocks remote images (SPEC §18.2)", async () => {
  const outcome = await app.page.evaluate(
    () =>
      new Promise<string>((done) => {
        const timer = setTimeout(() => done("timeout"), 5000);
        document.addEventListener("securitypolicyviolation", (e) => {
          if (e.blockedURI.startsWith("https://example.com")) {
            clearTimeout(timer);
            done(`blocked by ${e.effectiveDirective}`);
          }
        });
        const img = document.createElement("img");
        img.src = "https://example.com/vv-csp-probe.png";
        document.body.appendChild(img);
      }),
  );
  expect(outcome).toBe("blocked by img-src");
});
