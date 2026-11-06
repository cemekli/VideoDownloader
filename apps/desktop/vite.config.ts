import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

const fixtures = fileURLToPath(new URL("../../tests/fixtures", import.meta.url));

export default defineConfig({
  plugins: [react()],
  resolve: { alias: { "@fixtures": fixtures } },
  server: { fs: { allow: [".", fixtures] } },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    reporters: ["default", "junit"],
    outputFile: { junit: "../../test-artifacts/vitest/junit.xml" },
  },
});
