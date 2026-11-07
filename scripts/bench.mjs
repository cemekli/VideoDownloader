#!/usr/bin/env node
// Benchmark runner (STATE.md F0.12, ADR-0009, SPEC §22, DEC-17).
//
//   npm run bench                    # default seed, 10.000 items, 100.000 segments
//   npm run bench -- --seed 42       # any vv-bench flag is passed through
//
// Builds vv-bench in release, runs BM-01…03 on generated data and writes
// test-artifacts/bench/report.json and report.md with the machine
// description SPEC §22 asks for (CPU, OS build, RAM, power mode, versions).
// Budgets are compared, never enforced here: only the reference machine's
// numbers count (docs/testing.md), and F4.5 runs them on the real schema.

import { spawnSync } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const isWin = process.platform === "win32";
const out = join(root, "test-artifacts", "bench");
mkdirSync(out, { recursive: true });

const run = (cmd, args, opts = {}) => spawnSync(cmd, args, { encoding: "utf8", shell: false, ...opts });
const first = (cmd, args) => {
  const r = run(cmd, args);
  return r.status === 0 ? r.stdout.trim() : null;
};

const bench = run("cargo", ["run", "-q", "--release", "-p", "vv-benchdata", "--bin", "vv-bench", "--", "run", "--out", join(out, "db"), ...process.argv.slice(2)], {
  cwd: root,
  stdio: ["ignore", "pipe", "inherit"],
  maxBuffer: 64 * 1024 * 1024,
});
if (bench.status !== 0) {
  console.error("✗ vv-bench failed");
  process.exit(1);
}
const { manifest, report } = JSON.parse(bench.stdout);
// The 26 MB benchmark database is not an artifact worth keeping.
rmSync(join(out, "db"), { recursive: true, force: true });

function powerMode() {
  if (isWin) return (first("powercfg", ["/getactivescheme"])?.match(/\(([^)]+)\)\s*$/)?.[1] ?? "unknown");
  return first("cat", ["/sys/firmware/acpi/platform_profile"]) ?? "unknown";
}
function storage() {
  if (isWin) {
    return first("powershell", ["-NoProfile", "-Command", "(Get-PhysicalDisk | Select-Object -First 1 | ForEach-Object { \"$($_.MediaType) $($_.BusType)\" })"]) ?? "unknown";
  }
  const rot = first("sh", ["-c", "cat /sys/block/$(lsblk -no pkname $(df --output=source . | tail -1) 2>/dev/null | head -1)/queue/rotational 2>/dev/null"]);
  return rot === "0" ? "SSD (non-rotational)" : rot === "1" ? "HDD" : "unknown";
}

const env = {
  date: new Date().toISOString(),
  commit: first("git", ["rev-parse", "--short", "HEAD"]),
  os: `${os.type()} ${os.release()}`,
  osVersion: os.version(),
  arch: os.arch(),
  cpu: os.cpus()[0]?.model.trim() ?? "unknown",
  logicalCpus: os.availableParallelism(),
  ramGiB: Math.round((os.totalmem() / 2 ** 30) * 10) / 10,
  powerMode: powerMode(),
  storage: storage(),
  ci: process.env.CI === "true",
  rustc: first("rustc", ["--version"]),
  node: process.version,
};

// SPEC §22 reference: Windows 11 x64, SSD, ≥ 8 GB RAM, ≥ 4 logical CPUs.
const reference =
  isWin && os.arch() === "x64" && env.logicalCpus >= 4 && env.ramGiB >= 7.5 && !env.ci && /SSD/i.test(env.storage) ? "reference-class" : "not reference";

const full = { env, reference, manifest, report };
writeFileSync(join(out, "report.json"), `${JSON.stringify(full, null, 2)}\n`);

const verdict = (s) => (s.p95Ms <= s.budgetMs ? "bütçe içinde" : "**bütçe aşıldı**");
const md = [
  "# Benchmark raporu",
  "",
  `- Tarih: ${env.date}, commit ${env.commit}`,
  `- Makine: ${env.cpu}, ${env.logicalCpus} mantıksal CPU, ${env.ramGiB} GiB RAM, ${env.storage}, güç modu: ${env.powerMode}`,
  `- İşletim sistemi: ${env.os} (${env.osVersion}), ${env.arch}${env.ci ? ", CI" : ""}`,
  `- Sınıf: **${reference}** (SPEC §22 referansı: Windows 11 x64, SSD, ≥ 8 GB RAM, ≥ 4 mantıksal CPU, CI dışı)`,
  `- Araçlar: ${env.rustc}, Node ${env.node}, SQLite ${report.sqliteVersion}`,
  `- Veri: vv-benchdata v${manifest.generatorVersion}, seed ${manifest.seed}, ${manifest.items} öğe, ${manifest.segments} segment (items ${manifest.itemsSha256.slice(0, 12)}…, segments ${manifest.segmentsSha256.slice(0, 12)}…)`,
  `- Yükleme: öğeler ${report.loadItemsMs} ms, segmentler ${report.loadSegmentsMs} ms; veritabanı ${(report.dbBytes / 2 ** 20).toFixed(1)} MiB`,
  "",
  "Prototip tablolar (`bench_*`); uygulama şeması F1.1'de gelince F4.5'te tekrarlanır. Bütçe karşılaştırması yalnızca referans sınıfı bir makinede anlamlıdır.",
  "",
  "| Kimlik | Ölçüm | NFR | Örnek | p50 ms | p95 ms | max ms | Bütçe (p95) | Sonuç |",
  "|---|---|---|---|---|---|---|---|---|",
  ...report.stats.map((s) => `| ${s.id} | ${s.what} | ${s.nfr} | ${s.samples} | ${s.p50Ms} | ${s.p95Ms} | ${s.maxMs} | ≤ ${s.budgetMs} | ${verdict(s)} |`),
  "| BM-04 | başlangıç, 10.000 öğe | NFR-06 | — | — | — | — | ≤ 3000 | not run: uygulama kütüphaneyi henüz açmıyor (F1) |",
  "| BM-05 | idle/scroll bellek | NFR-07 | — | — | — | — | ≤ 500 MB | not run: liste ekranı F1.7'de |",
  "",
].join("\n");
writeFileSync(join(out, "report.md"), md);
console.log(md);
