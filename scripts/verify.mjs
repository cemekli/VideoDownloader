#!/usr/bin/env node
// One-command quality gate for VideoVault (STATE.md "Doğrulama").
//
//   node scripts/verify.mjs            # all gates
//   node scripts/verify.mjs --skip e2e # skip one or more groups (comma separated)
//   node scripts/verify.mjs --only rust
//
// Writes test-artifacts/verify/report.json and report.md. Each step is reported
// as "automated pass" or "automated fail" (SPEC §25.4); skipped steps are "not run".
// Exit code is non-zero if any executed step fails.

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join, dirname, relative } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const web = join(root, "apps", "desktop");
const isWin = process.platform === "win32";

const args = process.argv.slice(2);
const pick = (flag) => {
  const i = args.indexOf(flag);
  return i >= 0 && args[i + 1] ? new Set(args[i + 1].split(",")) : null;
};
const only = pick("--only");
const skip = pick("--skip") ?? new Set();

/** @type {{id:string, group:string, title:string, run:() => {ok:boolean, detail?:string}}[]} */
const steps = [
  { id: "fixtures-no-secrets", group: "repo", title: "Fixtures contain no real secrets (SPEC §25.2)", run: checkFixtures },
  { id: "rust-fmt", group: "rust", title: "cargo fmt --check", run: () => sh("cargo", ["fmt", "--all", "--check"], root) },
  { id: "rust-clippy", group: "rust", title: "cargo clippy -D warnings", run: () => sh("cargo", ["clippy", "--workspace", "--all-targets", "--quiet", "--", "-D", "warnings"], root) },
  { id: "rust-test", group: "rust", title: "cargo test (unit + fixture)", run: () => sh("cargo", ["test", "--workspace", "--quiet"], root) },
  { id: "web-typecheck", group: "web", title: "TypeScript typecheck", run: () => sh("npx", ["tsc", "--noEmit"], web) },
  { id: "web-unit", group: "web", title: "Vitest unit/component tests", run: () => sh("npx", ["vitest", "run"], web) },
  { id: "web-build", group: "web", title: "Vite production build", run: () => sh("npx", ["vite", "build", "--outDir", join(root, "test-artifacts", "web-build")], web) },
  { id: "e2e", group: "e2e", title: "Playwright E2E (fixture IPC, 3 viewports)", run: () => sh("npx", ["playwright", "test", "--trace", "off"], web) },
  { id: "e2e-desktop", group: "desktop", title: "Real-app E2E on Windows (WebView2 + real IPC, ADR-0001)", run: desktopE2e },
];

// Needs Windows and the E2E test build (`npm run desktop:e2e-exe`). Elsewhere it is
// reported as "not run", never as a pass. CI sets VV_REQUIRE_DESKTOP_E2E=1 so a
// missing build fails the gate instead of being skipped.
function desktopE2e() {
  const required = process.env.VV_REQUIRE_DESKTOP_E2E === "1";
  const exe = process.env.VV_DESKTOP_E2E_EXE ?? join(root, "target", "release", "videovault-e2e.exe");
  if (!isWin) return required ? { ok: false, detail: "required but this is not Windows" } : { skipped: true, detail: "Windows only" };
  if (!existsSync(exe)) return required ? { ok: false, detail: `missing ${relative(root, exe)}` } : { skipped: true, detail: "no E2E build; run npm run desktop:e2e-exe" };
  return sh("npx", ["playwright", "test", "-c", "playwright.desktop.config.ts", "--trace", "off"], web);
}

function sh(cmd, argv, cwd) {
  const r = spawnSync(cmd, argv, { cwd, stdio: "inherit", shell: isWin, env: process.env });
  if (r.error) return { ok: false, detail: String(r.error.message) };
  return { ok: r.status === 0, detail: `exit ${r.status}` };
}

function walk(dir) {
  return readdirSync(dir).flatMap((n) => {
    const p = join(dir, n);
    return statSync(p).isDirectory() ? walk(p) : [p];
  });
}

// Real cookies/tokens must never be committed as fixtures. Canary values used by
// redaction tests must contain the marker "CANARY" so they are recognisable.
function checkFixtures() {
  const patterns = [
    /set-cookie\s*:/i,
    /\bauthorization\s*:\s*(?!(bearer\s+)?CANARY)\S+/i,
    /\bbearer\s+(?!CANARY)[a-z0-9._-]{16,}/i,
    /\b(remixsid|sessionid|sid|__secure-[a-z0-9-]+)\s*=\s*(?!CANARY)[a-z0-9]{12,}/i,
    /^#\s*netscape http cookie file/im,
  ];
  const hits = [];
  for (const f of walk(join(root, "tests", "fixtures"))) {
    const text = readFileSync(f, "utf8");
    for (const p of patterns) if (p.test(text)) hits.push(`${relative(root, f)} matches ${p}`);
  }
  return hits.length ? { ok: false, detail: hits.join("; ") } : { ok: true, detail: "no secret-like content" };
}

function version(cmd, argv) {
  const r = spawnSync(cmd, argv, { encoding: "utf8", shell: isWin });
  return r.status === 0 ? r.stdout.trim().split("\n")[0] : "unavailable";
}

const results = [];
for (const s of steps) {
  const selected = (!only || only.has(s.group) || only.has(s.id)) && !skip.has(s.group) && !skip.has(s.id);
  if (!selected) {
    results.push({ ...s, status: "not run", ms: 0 });
    continue;
  }
  console.log(`\n=== ${s.id}: ${s.title} ===`);
  const t = Date.now();
  const r = s.run();
  const status = r.skipped ? "not run" : r.ok ? "automated pass" : "automated fail";
  results.push({ ...s, status, detail: r.detail, ms: Date.now() - t });
}

const report = {
  generated_at: new Date().toISOString(),
  commit: version("git", ["-C", root, "rev-parse", "--short", "HEAD"]),
  environment: {
    os: `${os.type()} ${os.release()} ${os.arch()}`,
    node: process.version,
    rustc: version("rustc", ["--version"]),
    cargo: version("cargo", ["--version"]),
  },
  steps: results.map(({ id, group, title, status, detail, ms }) => ({ id, group, title, status, detail, ms })),
};
const outDir = join(root, "test-artifacts", "verify");
mkdirSync(outDir, { recursive: true });
writeFileSync(join(outDir, "report.json"), JSON.stringify(report, null, 2) + "\n");
const md = [
  `# Verify report`,
  ``,
  `- Generated: ${report.generated_at}`,
  `- Commit: ${report.commit}`,
  `- Environment: ${report.environment.os}; node ${report.environment.node}; ${report.environment.rustc}`,
  ``,
  `| Step | Status | Time | Detail |`,
  `|---|---|---|---|`,
  ...report.steps.map((s) => `| ${s.id} | ${s.status} | ${(s.ms / 1000).toFixed(1)} s | ${s.detail ?? ""} |`),
  ``,
].join("\n");
writeFileSync(join(outDir, "report.md"), md);

console.log("\n" + md);
const failed = results.filter((r) => r.status === "automated fail");
process.exit(failed.length ? 1 : 0);
