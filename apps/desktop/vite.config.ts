import { defineConfig, type Plugin } from "vitest/config";
import react from "@vitejs/plugin-react";
import { createReadStream, statSync } from "node:fs";
import type { IncomingMessage, ServerResponse } from "node:http";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const fixtures = fileURLToPath(new URL("../../tests/fixtures", import.meta.url));

/**
 * Fixture mode only: serves the playback matrix files (tests/fixtures/media)
 * under /fixture-media/ with byte ranges, so the player can be tested in a
 * browser (F2.4). Never part of a build: the real app uses the vvmedia
 * protocol.
 */
function fixtureMedia(): Plugin {
  const serve = (req: IncomingMessage, res: ServerResponse, next: () => void) => {
    const m = /^\/fixture-media\/([a-z0-9-]+\.(mp4|mkv|mov|webm))$/.exec(req.url?.split("?")[0] ?? "");
    if (!m) return next();
    const file = join(fixtures, "media", m[1]!);
    let size: number;
    try {
      size = statSync(file).size;
    } catch {
      res.statusCode = 404;
      return res.end();
    }
    const type = { mp4: "video/mp4", mkv: "video/x-matroska", mov: "video/quicktime", webm: "video/webm" }[m[2] as "mp4"];
    res.setHeader("Content-Type", type);
    res.setHeader("Accept-Ranges", "bytes");
    const range = /^bytes=(\d*)-(\d*)$/.exec(String(req.headers.range ?? ""));
    if (range) {
      const start = range[1] ? Number(range[1]) : Math.max(0, size - Number(range[2]));
      const end = range[1] && range[2] ? Math.min(Number(range[2]), size - 1) : size - 1;
      if (start >= size || start > end) {
        res.statusCode = 416;
        res.setHeader("Content-Range", `bytes */${size}`);
        return res.end();
      }
      res.statusCode = 206;
      res.setHeader("Content-Range", `bytes ${start}-${end}/${size}`);
      res.setHeader("Content-Length", String(end - start + 1));
      return void createReadStream(file, { start, end }).pipe(res);
    }
    res.setHeader("Content-Length", String(size));
    createReadStream(file).pipe(res);
  };
  return {
    name: "videovault-fixture-media",
    apply: (_config, env) => env.mode === "fixture",
    configureServer: (server) => void server.middlewares.use(serve),
    configurePreviewServer: (server) => void server.middlewares.use(serve),
  };
}

export default defineConfig({
  plugins: [react(), fixtureMedia()],
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
