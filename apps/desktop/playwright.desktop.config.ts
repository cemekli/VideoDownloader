import { defineConfig } from "@playwright/test";

// Real-app E2E (STATE.md F0.4, ADR-0001). No web server and no browser
// download: the tests start target/release/videovault.exe themselves.
export default defineConfig({
  testDir: "./e2e-desktop",
  outputDir: "../../test-artifacts/e2e-desktop/results",
  reporter: [["list"], ["junit", { outputFile: "../../test-artifacts/e2e-desktop/junit.xml" }]],
  workers: 1,
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: 0,
  timeout: 60_000,
  use: { trace: "off" },
});
