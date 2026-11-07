#!/usr/bin/env node
// Runs the pinned yt-dlp through the network guard (crates/vv-netguard,
// STATE.md F0.9, ADR-0007, SPEC §20.2, AC-29) and checks that a forbidden
// target is never contacted, whatever the URL, redirect or manifest says.
//
//   node scripts/fetch-tools.mjs        # once, for the pinned yt-dlp
//   node scripts/netguard-probe.mjs
//
// Two local servers: "media" is registered with the guard as a test target
// (it stands for a public site); "canary" is not, and must see zero
// connections in every guarded run. A control run without the guard proves
// the canary would notice. Nothing leaves the machine except the DNS lookup
// of localtest.me (a public name that resolves to 127.0.0.1).
// Writes test-artifacts/netguard/report.json.

import { spawn, spawnSync } from "node:child_process";
import { createServer as createHttp } from "node:http";
import { createServer as createTcp } from "node:net";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createInterface } from "node:readline";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const isWin = process.platform === "win32";
const platform = `${isWin ? "windows" : process.platform}-${process.arch === "x64" ? "x86_64" : process.arch}`;
const ytdlp = join(root, "tools", "bin", platform, "yt-dlp", isWin ? "yt-dlp.exe" : "yt-dlp");
const ytArgs = JSON.parse(readFileSync(join(root, "tests", "fixtures", "ytdlp", "args.json"), "utf8"));
const clip = readFileSync(join(root, "tests", "fixtures", "media", "mp4-h264-aac.mp4"));
if (!existsSync(ytdlp)) {
  console.error(`✗ ${ytdlp} missing: run node scripts/fetch-tools.mjs first`);
  process.exit(1);
}

const listen = (srv) => new Promise((r) => srv.listen(0, "127.0.0.1", () => r(srv.address().port)));

// Canary: counts raw TCP connections, so even a TLS ClientHello counts.
let canaryHits = 0;
const canary = createTcp((s) => {
  canaryHits++;
  s.destroy();
});
const C = await listen(canary);

let mediaHits = 0;
const media = createHttp((req, res) => {
  mediaHits++;
  const path = new URL(req.url, "http://x").pathname;
  const redirect = (to) => res.writeHead(302, { Location: to }).end();
  switch (path) {
    case "/clip.mp4":
      return res.writeHead(200, { "Content-Type": "video/mp4", "Content-Length": clip.length }).end(clip);
    case "/to-canary":
      return redirect(`http://127.0.0.1:${C}/clip.mp4`);
    case "/to-loopback":
      return redirect("http://127.0.0.1/clip.mp4");
    case "/to-mapped":
      return redirect("http://[::ffff:127.0.0.1]/clip.mp4");
    case "/to-metadata":
      return redirect("http://169.254.169.254/latest/meta-data/");
    case "/to-private":
      return redirect("http://10.0.0.1/clip.mp4");
    case "/to-dns":
      return redirect("http://localtest.me/clip.mp4");
    case "/hls.m3u8":
      res.writeHead(200, { "Content-Type": "application/vnd.apple.mpegurl" });
      return res.end(`#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:3\n#EXTINF:3,\nhttp://127.0.0.1:${C}/seg0.ts\n#EXT-X-ENDLIST\n`);
    case "/seg-ok.ts":
      return res.writeHead(200, { "Content-Type": "video/mp2t", "Content-Length": clip.length }).end(clip);
    case "/hls-mixed.m3u8":
      // First segment allowed, second forbidden: without
      // --abort-on-unavailable-fragments yt-dlp would finish with a gap.
      res.writeHead(200, { "Content-Type": "application/vnd.apple.mpegurl" });
      return res.end(`#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:3\n#EXTINF:3,\n/seg-ok.ts\n#EXTINF:3,\nhttp://127.0.0.1:${C}/seg1.ts\n#EXT-X-ENDLIST\n`);
    case "/page.html":
      res.writeHead(200, { "Content-Type": "text/html" });
      return res.end(`<!doctype html><title>t</title><video src="http://127.0.0.1:${C}/v.mp4"></video>`);
    default:
      return res.writeHead(404).end();
  }
});
const M = await listen(media);
const origin = `http://127.0.0.1:${M}`;

// The guard, built from the workspace.
const guard = spawn("cargo", ["run", "-q", "-p", "vv-netguard", "--bin", "vv-netguard-probe", "--", "--test-target", `127.0.0.1:${M}`], {
  cwd: root,
  stdio: ["pipe", "pipe", "inherit"],
});
const lines = createInterface({ input: guard.stdout });
const nextLine = () => new Promise((r) => lines.once("line", r));
const { proxy } = JSON.parse(await nextLine());

const work = mkdtempSync(join(tmpdir(), "vv-netguard-"));
// Hostile environment: proxy variables pointing at the canary and NO_PROXY
// for everything. --proxy must win over all of them.
const env = { ...process.env, HTTP_PROXY: `http://127.0.0.1:${C}`, HTTPS_PROXY: `http://127.0.0.1:${C}`, ALL_PROXY: `http://127.0.0.1:${C}`, http_proxy: `http://127.0.0.1:${C}`, https_proxy: `http://127.0.0.1:${C}`, NO_PROXY: "*", no_proxy: "*" };

function ytRun(url, { guarded = true, out }) {
  const argv = [
    ...ytArgs.common,
    // The same flags the app uses (vv-ytdlp args::NETWORK_GUARD).
    ...(guarded ? ytArgs.networkGuard.map((a) => (a === "{proxy}" ? proxy : a)) : ["--downloader", "native"]),
    "--no-js-runtimes",
    "--no-remote-components",
    "--no-playlist",
    "--socket-timeout",
    "5",
    "--retries",
    "0",
    "--fragment-retries",
    "0",
    "-o",
    join(out, "%(id)s.%(ext)s"),
    url,
  ];
  return new Promise((resolve) => {
    const p = spawn(ytdlp, argv, { env: guarded ? env : { ...process.env, HTTP_PROXY: "", HTTPS_PROXY: "", NO_PROXY: "*" } });
    let stderr = "";
    p.stderr.on("data", (d) => (stderr += d));
    p.stdout.on("data", () => {});
    const t = setTimeout(() => p.kill(), 90_000);
    p.on("close", (code) => {
      clearTimeout(t);
      resolve({ code, stderr });
    });
  });
}

const cases = [
  { case: "allowed-direct-file", url: `${origin}/clip.mp4`, expect: "ok" },
  { case: "redirect-to-canary", url: `${origin}/to-canary`, expect: "blocked" },
  { case: "redirect-to-loopback", url: `${origin}/to-loopback`, expect: "blocked" },
  { case: "redirect-to-mapped-ipv6", url: `${origin}/to-mapped`, expect: "blocked" },
  { case: "redirect-to-metadata", url: `${origin}/to-metadata`, expect: "blocked" },
  { case: "redirect-to-private", url: `${origin}/to-private`, expect: "blocked" },
  { case: "redirect-to-name-resolving-to-loopback", url: `${origin}/to-dns`, expect: "blocked" },
  { case: "hls-segments-on-canary", url: `${origin}/hls.m3u8`, expect: "blocked" },
  { case: "hls-mixed-segments", url: `${origin}/hls-mixed.m3u8`, expect: "blocked" },
  { case: "page-embedding-canary", url: `${origin}/page.html`, expect: "blocked" },
  { case: "https-to-canary", url: `https://127.0.0.1:${C}/clip.mp4`, expect: "blocked" },
  { case: "localhost-name", url: `http://localhost:${C}/clip.mp4`, expect: "blocked" },
];

const failures = [];
const results = [];
let eventsBefore = 0;
for (const c of cases) {
  const out = join(work, c.case);
  mkdirSync(out);
  const before = canaryHits;
  const r = await ytRun(c.url, { out });
  const files = readdirSync(out).filter((f) => !/\.(part|ytdl)$/.test(f));
  const res = {
    case: c.case,
    exit: r.code,
    downloaded: files.length,
    canaryConnections: canaryHits - before,
    error: r.stderr.split(/\r?\n/).find((l) => l.startsWith("ERROR:"))?.replace(proxy, "<proxy>").replace(/127\.0\.0\.1:\d+/g, "127.0.0.1:<port>") ?? null,
  };
  results.push(res);
  if (res.canaryConnections !== 0) failures.push(`${c.case}: the canary saw ${res.canaryConnections} connection(s)`);
  if (c.expect === "ok" && (r.code !== 0 || files.length !== 1)) failures.push(`${c.case}: download failed: ${r.stderr}`);
  if (c.expect === "blocked" && (r.code === 0 || files.length)) failures.push(`${c.case}: yt-dlp succeeded`);
  console.log(`${res.canaryConnections === 0 ? "✓" : "✗"} ${c.case}: exit ${r.code}, files ${files.length}, canary ${res.canaryConnections}${res.error ? `, ${res.error}` : ""}`);
}

// Control: the same redirect without the guard does reach the canary.
const ctlOut = join(work, "control");
mkdirSync(ctlOut);
const before = canaryHits;
await ytRun(`${origin}/to-canary`, { guarded: false, out: ctlOut });
const control = canaryHits - before;
if (control < 1) failures.push("control run without the guard did not reach the canary: the probe proves nothing");
console.log(`${control >= 1 ? "✓" : "✗"} control without guard: canary saw ${control} connection(s)`);

guard.stdin.end();
const { events } = JSON.parse(await nextLine());
const blockedEvents = events.filter((e) => e.decision === "blocked");
const reasons = [...new Set(blockedEvents.map((e) => e.block.reason))].sort();
const unauthorized = events.filter((e) => e.decision === "unauthorized").length;
if (unauthorized) failures.push(`${unauthorized} unauthorized proxy use(s)`);
const allowedOther = events.filter((e) => e.decision === "allowed" && e.port !== M);
if (allowedOther.length) failures.push(`guard allowed other targets: ${JSON.stringify(allowedOther)}`);
console.log(`  guard: ${events.length} connection(s), ${blockedEvents.length} blocked (${reasons.join(", ")})`);

const report = {
  ytDlp: spawnSync(ytdlp, ["--version"], { encoding: "utf8" }).stdout.trim(),
  platform,
  results,
  control: { canaryConnections: control },
  guard: { connections: events.length, blocked: blockedEvents.length, reasons, blockedTargets: blockedEvents.map((e) => `${e.method} ${e.host}:${e.port === C ? "<canary>" : e.port} ${e.block.reason}`) },
};
mkdirSync(join(root, "test-artifacts", "netguard"), { recursive: true });
writeFileSync(join(root, "test-artifacts", "netguard", "report.json"), `${JSON.stringify(report, null, 2)}\n`);
canary.close();
media.close();
rmSync(work, { recursive: true, force: true });
if (failures.length) {
  for (const f of failures) console.error(`✗ ${f}`);
  process.exit(1);
}
console.log("\n✓ no guarded run reached a forbidden target (AC-29)");
