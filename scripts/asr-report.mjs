#!/usr/bin/env node
// Transcription quality and speed report (STATE.md F6.7, SPEC AI-A07, AC-21,
// ADR-0034).
//
//   npm run asr:report                              # items with local transcripts
//   npm run asr:report -- --item <id|part of title or address> [--item …]
//          [--terms "Kadıköy,PostgreSQL"]
//          [--reference metin.txt --reference-language tr]
//          [--db path\to\library.sqlite]
//
// Reads the app's library read-only (the app may stay open): for each item,
// every local transcript is scored against the uploader's captions (or the
// site's automatic captions, a checked text file, or the largest model) and
// timed by its transcription job. Writes test-artifacts/asr/report.md with
// the machine description (as `npm run bench` does) and prints it.

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import os from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const isWin = process.platform === "win32";
const args = process.argv.slice(2);

const values = (flag) => args.flatMap((a, i) => (a === flag && i + 1 < args.length ? [args[i + 1]] : []));
const value = (flag) => values(flag)[0];

const defaultDb = isWin
  ? join(process.env.APPDATA ?? join(os.homedir(), "AppData", "Roaming"), "dev.videovault.desktop", "db", "library.sqlite")
  : process.platform === "darwin"
    ? join(os.homedir(), "Library", "Application Support", "dev.videovault.desktop", "db", "library.sqlite")
    : join(process.env.XDG_DATA_HOME ?? join(os.homedir(), ".local", "share"), "dev.videovault.desktop", "db", "library.sqlite");
const db = value("--db") ?? defaultDb;
if (!existsSync(db)) {
  console.error(`✗ No library at ${db} (open the app once, or pass --db).`);
  process.exit(1);
}

const build = spawnSync("cargo", ["build", "-q", "--release", "-p", "vv-asr-quality", "--bin", "vv-asr-report"], { cwd: root, stdio: "inherit" });
if (build.status !== 0) {
  console.error("✗ cargo build failed");
  process.exit(1);
}
const bin = join(root, "target", "release", `vv-asr-report${isWin ? ".exe" : ""}`);
const report = (extra) => spawnSync(bin, ["--db", db, ...extra], { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });

const items = values("--item");
if (items.length === 0) {
  const r = report(["list"]);
  process.stdout.write(r.stdout);
  if (r.status !== 0) {
    process.stderr.write(r.stderr);
    process.exit(1);
  }
  console.log("\nRun again with --item <id or part of the title> (one --item per video, e.g. a Turkish and an English one).");
  process.exit(0);
}
if (items.length > 1 && value("--reference")) {
  console.error("✗ --reference applies to one --item only.");
  process.exit(1);
}

const pass = [];
for (const f of ["--terms", "--reference", "--reference-language"]) {
  const v = value(f);
  if (v !== undefined) pass.push(f, v);
}
const sections = [];
for (const item of items) {
  const r = report(["--item", item, ...pass]);
  if (r.status !== 0) {
    process.stderr.write(r.stderr);
    process.exit(1);
  }
  sections.push(r.stdout);
}

const first = (cmd, a) => {
  const r = spawnSync(cmd, a, { encoding: "utf8" });
  return r.status === 0 ? r.stdout.trim() : null;
};
const powerMode = () =>
  isWin ? (first("powercfg", ["/getactivescheme"])?.match(/\(([^)]+)\)\s*$/)?.[1] ?? "unknown") : (first("cat", ["/sys/firmware/acpi/platform_profile"]) ?? "unknown");
const env = {
  date: new Date().toISOString(),
  commit: first("git", ["-C", root, "rev-parse", "--short", "HEAD"]),
  cpu: os.cpus()[0]?.model.trim() ?? "unknown",
  logicalCpus: os.availableParallelism(),
  ramGiB: Math.round((os.totalmem() / 2 ** 30) * 10) / 10,
  os: `${os.type()} ${os.release()} (${os.version()}), ${os.arch()}`,
  powerMode: powerMode(),
};

const md = [
  "# Transkripsiyon kalite ve hız raporu",
  "",
  `- Tarih: ${env.date}, commit ${env.commit}`,
  `- Makine: ${env.cpu}, ${env.logicalCpus} mantıksal CPU, ${env.ramGiB} GiB RAM, güç modu: ${env.powerMode}`,
  `- İşletim sistemi: ${env.os}`,
  "- Ölçüm: `npm run asr:report` (ADR-0034). Sayılar bu videolar ve bu makine içindir; uygulama genelinde doğruluk ya da hız sözü değildir (SPEC AI-A07).",
  "",
  ...sections,
].join("\n");

const out = join(root, "test-artifacts", "asr");
mkdirSync(out, { recursive: true });
writeFileSync(join(out, "report.md"), md);
console.log(md);
console.error(`\n✓ Saved to ${join(out, "report.md")}`);
