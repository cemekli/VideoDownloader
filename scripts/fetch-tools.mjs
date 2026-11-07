#!/usr/bin/env node
// Downloads the managed tools pinned in tools/manifest.json for this platform
// into tools/bin/, verifies them and runs the health check (STATE.md F0.8b,
// ADR-0005).
//
//   node scripts/fetch-tools.mjs            # download (if needed), verify, check
//   node scripts/fetch-tools.mjs --offline  # verify what is already there
//
// Verification, in order; any failure stops with a non-zero exit:
//   1. size and SHA-256 of each download (manifest);
//   2. yt-dlp only: SHA2-256SUMS is signed by the pinned key and lists the
//      same hash (needs gpgv; Git for Windows ships one);
//   3. SHA-256 of the executable after unpacking;
//   4. `--version` of each tool equals the pinned version;
//   5. yt-dlp with our JS runtime flags reports the managed Deno as supported,
//      and ignores a Deno on PATH.
//
// This is a developer and CI tool. The app never downloads executables
// (SPEC §23.1); in a release the installer carries these exact files.

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const manifest = JSON.parse(readFileSync(join(root, "tools", "manifest.json"), "utf8"));
const offline = process.argv.includes("--offline");
const isWin = process.platform === "win32";
const platform = `${isWin ? "windows" : process.platform}-${process.arch === "x64" ? "x86_64" : process.arch}`;
const binDir = join(root, "tools", "bin", platform);
// Same constant as crates/vv-tools/src/manifest.rs YTDLP_SIGNING_KEY.
const YTDLP_KEY = "AC0CBBE6848D6A873464AF4E57CF65933B5A7581";

const fail = (msg) => {
  console.error(`✗ ${msg}`);
  process.exit(1);
};
const ok = (msg) => console.log(`✓ ${msg}`);
const sha256 = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
const tool = (id) => manifest.tools.find((t) => t.id === id) ?? fail(`manifest has no ${id}`);
const artifact = (t) => t.artifacts.find((a) => a.platform === platform) ?? fail(`${t.id} has no artifact for ${platform}`);

async function download(url, to) {
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) fail(`${url}: HTTP ${res.status}`);
  writeFileSync(to, Buffer.from(await res.arrayBuffer()));
}

// gpgv only checks a signature against a keyring file: no agent, no home
// directory, no trust database. That keeps it working with the MSYS gpg that
// Git for Windows ships, where a full `gpg --import` can fail.
function gpgvBinary() {
  for (const c of ["gpgv", "C:\\Program Files\\Git\\usr\\bin\\gpgv.exe"]) {
    if (spawnSync(c, ["--version"], { encoding: "utf8" }).status === 0) return c;
  }
  return null;
}

/** ASCII-armoured key -> binary keyring (what `gpg --dearmor` does). */
function dearmor(armored) {
  const lines = armored.replace(/\r/g, "").split("\n");
  const begin = lines.findIndex((l) => l.startsWith("-----BEGIN PGP PUBLIC KEY BLOCK-----"));
  if (begin < 0) fail("tools/yt-dlp-public.key is not an armoured public key");
  let i = begin + 1;
  while (i < lines.length && lines[i].trim() !== "") i++; // armour headers
  const body = [];
  for (i++; i < lines.length; i++) {
    const l = lines[i].trim();
    if (l.startsWith("=") || l.startsWith("-----END")) break; // CRC24 line or end
    body.push(l);
  }
  return Buffer.from(body.join(""), "base64");
}

async function verifyYtDlpSignature(t, a) {
  const p = t.provenance;
  const work = mkdtempSync(join(tmpdir(), "vv-gpg-"));
  try {
    const gpgv = gpgvBinary() ?? fail("gpgv not found; it is needed to check yt-dlp's signed checksums (Git for Windows ships one)");
    const sums = join(work, "SHA2-256SUMS");
    const sig = join(work, "SHA2-256SUMS.sig");
    const keyring = join(work, "yt-dlp.gpg");
    await download(p.sumsUrl, sums);
    await download(p.signatureUrl, sig);
    // The key is committed (tools/yt-dlp-public.key): downloading it from the
    // same place as the binary would make the check circular.
    writeFileSync(keyring, dearmor(readFileSync(join(root, "tools", "yt-dlp-public.key"), "utf8")));
    // Relative paths from inside the work directory: gpg reads "C:/..." as a
    // URL scheme ("invalid key resource URL"); "./x" is a plain file.
    const v = spawnSync(gpgv, ["--status-fd", "1", "--keyring", "./yt-dlp.gpg", "SHA2-256SUMS.sig", "SHA2-256SUMS"], { cwd: work, encoding: "utf8" });
    const valid = /\[GNUPG:\] VALIDSIG ([0-9A-F]{40})/.exec(v.stdout ?? "");
    if (v.status !== 0 || !valid || valid[1] !== YTDLP_KEY) {
      fail(`SHA2-256SUMS signature is not valid for key ${YTDLP_KEY} (gpgv exit ${v.status})\n${v.stderr ?? ""}${v.error ?? ""}`);
    }
    ok(`yt-dlp SHA2-256SUMS signed by ${YTDLP_KEY}`);
    const name = a.url.split("/").pop();
    const line = readFileSync(sums, "utf8").split("\n").find((l) => l.trim().endsWith(`  ${name}`));
    if (!line || line.split(/\s+/)[0] !== a.sha256) fail(`signed SHA2-256SUMS does not list ${a.sha256} for ${name}`);
    ok(`signed checksum matches the manifest for ${name}`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

async function fetchTool(id) {
  const t = tool(id);
  const a = artifact(t);
  const dir = join(binDir, id);
  const exe = join(dir, a.executable);
  if (existsSync(exe) && sha256(exe) === a.executableSha256) {
    ok(`${id} ${t.version} already present and verified`);
    return exe;
  }
  if (offline) fail(`${id}: ${exe} missing or does not match the manifest`);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  // A bare executable is saved under its final name; an archive next to it.
  const file = a.archive ? join(dir, a.url.split("/").pop()) : exe;
  console.log(`… downloading ${a.url}`);
  await download(a.url, file);
  const size = statSync(file).size;
  if (size !== a.size) fail(`${id}: size ${size}, manifest says ${a.size}`);
  const got = sha256(file);
  if (got !== a.sha256) fail(`${id}: SHA-256 ${got}, manifest says ${a.sha256}`);
  ok(`${id} download matches the manifest (${size} bytes)`);
  if (t.provenance.kind === "gpg-signed-sums") await verifyYtDlpSignature(t, a);
  if (a.archive === "zip") {
    // Windows 10+ ships bsdtar, which reads zip. Call it by full path: Git's GNU
    // tar may come first on PATH and cannot read zip. unzip elsewhere.
    const winTar = join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe");
    const r = isWin ? spawnSync(winTar, ["-xf", file, "-C", dir, a.executable]) : spawnSync("unzip", ["-oq", file, a.executable, "-d", dir]);
    if (r.status !== 0) fail(`${id}: cannot unpack ${file}`);
    rmSync(file);
  }
  if (!existsSync(exe)) fail(`${id}: ${a.executable} not found after download`);
  if (!isWin) chmodSync(exe, 0o755);
  if (sha256(exe) !== a.executableSha256) fail(`${id}: executable SHA-256 does not match the manifest`);
  ok(`${id} executable verified`);
  return exe;
}

function run(exe, args, env = process.env) {
  const r = spawnSync(exe, args, { encoding: "utf8", env, timeout: 60_000 });
  return `${r.stdout ?? ""}${r.stderr ?? ""}`;
}

const ytdlp = await fetchTool("yt-dlp");
const deno = await fetchTool("deno");

const denoV = run(deno, ["--version"]).split("\n")[0].trim();
if (!denoV.startsWith(`deno ${tool("deno").version} `)) fail(`deno --version printed ${JSON.stringify(denoV)}`);
ok(denoV);
const ytV = run(ytdlp, ["--version"]).trim();
if (ytV !== tool("yt-dlp").version) fail(`yt-dlp --version printed ${JSON.stringify(ytV)}`);
ok(`yt-dlp ${ytV}`);

// Same flags as crates/vv-ytdlp args::COMMON + JS_RUNTIME. --simulate against
// a closed local port: nothing leaves the machine, only the -v header matters.
const probe = (runtimeArgs, env) =>
  (run(ytdlp, ["--ignore-config", "--no-plugin-dirs", "--no-cache-dir", ...runtimeArgs, "-v", "--simulate", "http://127.0.0.1:1/vv-probe"], env)
    .split("\n")
    .find((l) => l.startsWith("[debug] JS runtimes:")) ?? "").trim();
const managed = probe(["--no-js-runtimes", "--js-runtimes", `deno:${deno}`, "--no-remote-components"]);
if (managed !== `[debug] JS runtimes: deno-${tool("deno").version}`) fail(`managed Deno not picked up: ${JSON.stringify(managed)}`);
ok(`yt-dlp uses the managed Deno: ${managed}`);
const sep = isWin ? ";" : ":";
const withPath = { ...process.env, PATH: `${dirname(deno)}${sep}${process.env.PATH}` };
const ignored = probe(["--no-js-runtimes", "--js-runtimes", `deno:${join(binDir, "missing", "deno")}`, "--no-remote-components"], withPath);
if (ignored !== "[debug] JS runtimes: none") fail(`a Deno on PATH was used: ${JSON.stringify(ignored)}`);
ok("a Deno on PATH is ignored");
console.log(`\nManaged tools ready in ${binDir}`);
