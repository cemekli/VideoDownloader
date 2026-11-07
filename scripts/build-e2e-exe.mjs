#!/usr/bin/env node
// Builds the E2E test executable (ADR-0001): release build with the cargo
// feature `e2e-cdp`, copied to target/release/videovault-e2e(.exe).
// The shipped executable (`npm run desktop:exe`) is built without the feature.
// src-tauri/tauri.e2e.conf.json is merged over the config: it widens media-src
// to the vvmedia protocol used by the playback matrix (ADR-0004).

import { spawnSync } from "node:child_process";
import { copyFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const ext = process.platform === "win32" ? ".exe" : "";
const r = spawnSync("npx", ["tauri", "build", "--no-bundle", "--features", "e2e-cdp", "--config", join("src-tauri", "tauri.e2e.conf.json")], {
  cwd: join(root, "apps", "desktop"),
  stdio: "inherit",
  shell: process.platform === "win32",
});
if (r.status !== 0) process.exit(r.status ?? 1);
const from = join(root, "target", "release", `videovault${ext}`);
const to = join(root, "target", "release", `videovault-e2e${ext}`);
copyFileSync(from, to);
console.log(`E2E executable: ${to}`);
