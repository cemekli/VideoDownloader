import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

const fixtures = fileURLToPath(new URL("../../tests/fixtures", import.meta.url));

export default defineConfig({
  plugins: [react()],
  resolve: { alias: { "@fixtures": fixtures } },
  // Tauri (src-tauri/tauri.conf.json devUrl) expects exactly this port.
  server: { port: 5173, strictPort: true, fs: { allow: [".", fixtures] } },
  clearScreen: false,
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    reporters: ["default", "junit"],
    outputFile: { junit: "../../test-artifacts/vitest/junit.xml" },
  },
});
