import { defineConfig, devices } from "@playwright/test";

const PORT = 5174;
// Optional: point at a preinstalled Chromium (e.g. sandboxed CI images).
const executablePath = process.env.PW_CHROMIUM_EXECUTABLE || undefined;

export default defineConfig({
  testDir: "./e2e",
  outputDir: "../../test-artifacts/e2e/results",
  reporter: [
    ["list"],
    ["junit", { outputFile: "../../test-artifacts/e2e/junit.xml" }],
    ["html", { outputFolder: "../../test-artifacts/e2e/report", open: "never" }],
  ],
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  use: {
    baseURL: `http://localhost:${PORT}`,
    screenshot: "only-on-failure",
    trace: "off",
  },
  // SPEC §25.1 visual sizes: 1366×768, 1920×1080 and a narrow window.
  projects: [
    { name: "desktop-1366", use: { ...devices["Desktop Chrome"], viewport: { width: 1366, height: 768 }, launchOptions: { executablePath } } },
    { name: "desktop-1920", use: { ...devices["Desktop Chrome"], viewport: { width: 1920, height: 1080 }, launchOptions: { executablePath } } },
    { name: "narrow-800", use: { ...devices["Desktop Chrome"], viewport: { width: 800, height: 700 }, launchOptions: { executablePath } } },
  ],
  webServer: {
    command: `npx vite --mode fixture --port ${PORT} --strictPort`,
    url: `http://localhost:${PORT}`,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
