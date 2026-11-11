#!/usr/bin/env node
// Runs FFmpeg/ffprobe with VideoVault's fixed flags (tests/fixtures/ffmpeg/
// args.json) and checks what they do (STATE.md F0.8c, ADR-0006, SPEC §14.1,
// AC-59). `scripts/fetch-tools.mjs` calls it with the pinned build; on its
// own it takes any build:
//
//   node scripts/ffmpeg-probe.mjs --ffmpeg <path> --ffprobe <path> [--update]
//
// Checks, in order; any failure exits non-zero:
//   1. every playback-matrix file probes to the committed JSON
//      (tests/fixtures/ffmpeg/probe/<id>.json);
//   2. HLS and concat inputs that point at a local HTTP server and at a file
//      outside the import folder are refused, and the server sees no request;
//      the protocol-only control shows why the format whitelist is needed;
//   3. MKV -> MP4 stream-copy remux produces H.264/AAC MP4, and how long a
//      20-minute file takes (written to test-artifacts/ffmpeg/remux.json).
// --update rewrites the fixtures from this build instead of comparing.

import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { isDeepStrictEqual } from "node:util";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const fx = join(root, "tests", "fixtures", "ffmpeg");
const args = JSON.parse(readFileSync(join(fx, "args.json"), "utf8"));
const media = JSON.parse(readFileSync(join(root, "tests", "fixtures", "media", "manifest.json"), "utf8"));

// Expected ffprobe names for the playback matrix's labels.
const FORMAT = { mp4: "mov,mp4,m4a,3gp,3g2,mj2", mov: "mov,mp4,m4a,3gp,3g2,mj2", mkv: "matroska,webm", webm: "matroska,webm" };

function run(exe, argv) {
  return new Promise((resolve) => {
    const p = spawn(exe, argv, { stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    p.stdout.on("data", (d) => (stdout += d));
    p.stderr.on("data", (d) => (stderr += d));
    const timer = setTimeout(() => p.kill(), 120_000);
    p.on("error", (e) => resolve({ code: -1, stdout, stderr: `${stderr}${e}` }));
    p.on("close", (code) => {
      clearTimeout(timer);
      resolve({ code, stdout, stderr });
    });
  });
}

const fileArg = (p) => `file:${p}`;
const probeArgv = (input, inputArgs = args.input) => [...args.common, ...inputArgs, ...args.probe, "-i", input];

/** `[hls @ 0x55…] Format not on whitelist '…'` -> `hls`; same parse as vv-ffmpeg. */
function rejectedDemuxer(stderr) {
  for (const l of stderr.split(/\r?\n/)) {
    const m = /^\[([^\]@]+?) @ [^\]]*\]\s*Format not on whitelist/.exec(l.trim());
    if (m) return m[1].trim();
  }
  return null;
}

const normalise = (stderr, work) =>
  stderr
    .split(/\r?\n/)
    .filter((l) => l.trim())
    .map((l) => l.split(work).join("<work>").replaceAll("\\", "/").replace(/ @ 0x[0-9a-f]+\]/g, " @ <addr>]").replace(/127\.0\.0\.1:\d+/g, "127.0.0.1:<port>"));

export async function checkFfmpeg({ ffmpeg, ffprobe, update = false, log = console.log }) {
  const failures = [];
  const ok = (m) => log(`✓ ${m}`);
  const bad = (m) => {
    failures.push(m);
    console.error(`✗ ${m}`);
  };
  const work = mkdtempSync(join(tmpdir(), "vv-ffmpeg-"));
  const server = createServer((req, res) => {
    server.requests.push(req.url);
    res.writeHead(404).end();
  });
  server.requests = [];
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  const port = server.address().port;
  try {
    // 1. Playback-matrix files.
    mkdirSync(join(fx, "probe"), { recursive: true });
    for (const f of media.files) {
      const r = await run(ffprobe, probeArgv(fileArg(join(root, "tests", "fixtures", "media", f.file))));
      if (r.code !== 0) {
        bad(`${f.id}: ffprobe exit ${r.code}: ${r.stderr.trim()}`);
        continue;
      }
      const got = JSON.parse(r.stdout);
      const v = got.streams.find((s) => s.codec_type === "video")?.codec_name;
      const a = got.streams.find((s) => s.codec_type === "audio")?.codec_name;
      if (got.format.format_name !== FORMAT[f.container] || v !== f.video || a !== f.audio) {
        bad(`${f.id}: probed ${got.format.format_name} ${v}/${a}, expected ${FORMAT[f.container]} ${f.video}/${f.audio}`);
      }
      const path = join(fx, "probe", `${f.id}.json`);
      if (update) writeFileSync(path, `${JSON.stringify(got, null, 2)}\n`);
      else if (!isDeepStrictEqual(got, JSON.parse(readFileSync(path, "utf8")))) bad(`${f.id}: probe differs from ${path}:\n${JSON.stringify(got)}`);
    }
    if (!failures.length) ok(`${media.files.length} playback-matrix files probe as committed`);

    // 2. Inputs that reference other files or URLs (AC-59).
    const importRoot = join(work, "import");
    const outside = join(work, "outside");
    mkdirSync(importRoot);
    mkdirSync(outside);
    const secret = join(outside, "secret.ts");
    const conv = await run(ffmpeg, [...args.common, "-i", fileArg(join(root, "tests", "fixtures", "media", "mkv-h264-aac.mkv")), "-c", "copy", "-f", "mpegts", fileArg(secret)]);
    if (conv.code !== 0) bad(`cannot write the outside segment: ${conv.stderr}`);
    const http = `http://127.0.0.1:${port}/seg.ts`;
    const hls = join(importRoot, "playlist.m3u8");
    writeFileSync(hls, `#EXTM3U\n#EXT-X-TARGETDURATION:3\n#EXTINF:3,\n../outside/secret.ts\n#EXTINF:3,\n${http}\n#EXT-X-ENDLIST\n`);
    const httpOnlyHls = join(importRoot, "remote.m3u8");
    writeFileSync(httpOnlyHls, `#EXTM3U\n#EXT-X-TARGETDURATION:3\n#EXTINF:3,\n${http}\n#EXT-X-ENDLIST\n`);
    // A concat list disguised as .mkv: detection is by content, not extension.
    const concat = join(importRoot, "movie.mkv");
    writeFileSync(concat, `ffconcat version 1.0\nfile '../outside/secret.ts'\nfile '${http}'\n`);
    const cases = [
      { case: "hls-restricted", input: fileArg(hls), restricted: true, expect: "rejected" },
      { case: "hls-http-only-restricted", input: fileArg(httpOnlyHls), restricted: true, expect: "rejected" },
      { case: "concat-as-mkv-restricted", input: fileArg(concat), restricted: true, expect: "rejected" },
      { case: "url-input-restricted", input: http, restricted: true, expect: "rejected" },
      // Controls: what the whitelists prevent.
      { case: "hls-protocol-whitelist-only", input: fileArg(hls), inputArgs: ["-protocol_whitelist", "file"], expect: "observed" },
      { case: "url-input-unrestricted", input: http, inputArgs: [], expect: "observed" },
    ];
    const results = [];
    for (const c of cases) {
      const before = server.requests.length;
      const r = await run(ffprobe, probeArgv(c.input, c.restricted ? args.input : c.inputArgs));
      const requests = server.requests.length - before;
      let formatName = null;
      try {
        formatName = JSON.parse(r.stdout).format?.format_name ?? null;
      } catch {}
      const res = {
        case: c.case,
        restricted: !!c.restricted,
        exitOk: r.code === 0,
        formatName,
        rejectedDemuxer: rejectedDemuxer(r.stderr),
        httpRequests: requests,
        stderr: normalise(r.stderr, work).slice(0, 4),
      };
      results.push(res);
      if (c.expect === "rejected" && (res.exitOk || requests !== 0)) bad(`${c.case}: not refused (exit ok ${res.exitOk}, ${requests} HTTP requests)`);
    }
    const unrestricted = results.find((r) => r.case === "url-input-unrestricted");
    if (unrestricted.httpRequests < 1) bad("the HTTP canary saw no request from the unrestricted control; the check proves nothing");
    const rejPath = join(fx, "rejections.json");
    const rejDoc = { _comment: "Written by scripts/ffmpeg-probe.mjs --update; compared on every run. AC-59 evidence.", cases: results };
    if (update) writeFileSync(rejPath, `${JSON.stringify(rejDoc, null, 2)}\n`);
    else {
      const want = JSON.parse(readFileSync(rejPath, "utf8")).cases;
      for (const r of results) {
        const w = want.find((x) => x.case === r.case);
        const pick = ({ exitOk, formatName, rejectedDemuxer, httpRequests }) => ({ exitOk, formatName, rejectedDemuxer, httpRequests });
        if (!w || !isDeepStrictEqual(pick(r), pick(w))) bad(`${r.case}: ${JSON.stringify(r)} differs from rejections.json`);
      }
    }
    for (const r of results) log(`  ${r.case}: exit ${r.exitOk ? "ok" : "error"}, demuxer refused ${r.rejectedDemuxer ?? "-"}, format ${r.formatName ?? "-"}, HTTP requests ${r.httpRequests}`);
    if (!failures.length) ok("referencing inputs are refused without any network request (AC-59)");

    // 3. Remux.
    const remux = (input, output) => [...args.common, "-nostdin", ...args.input, "-i", fileArg(input), ...args.remuxOutput, fileArg(output)];
    const out = join(work, "remuxed.mp4");
    const rm1 = await run(ffmpeg, remux(join(root, "tests", "fixtures", "media", "mkv-h264-aac.mkv"), out));
    const probedOut = await run(ffprobe, probeArgv(fileArg(out)));
    const po = rm1.code === 0 && probedOut.code === 0 ? JSON.parse(probedOut.stdout) : null;
    const codecs = po?.streams.map((s) => s.codec_name).join("/");
    if (po?.format.format_name !== FORMAT.mp4 || codecs !== "h264/aac") bad(`remux: exit ${rm1.code}, got ${po?.format.format_name} ${codecs}: ${rm1.stderr}`);
    else ok("MKV H.264/AAC -> MP4 stream copy");
    const sizeBefore = statSync(out).size;
    const again = await run(ffmpeg, remux(join(root, "tests", "fixtures", "media", "mkv-h264-aac.mkv"), out));
    log(`  existing output with -n: exit ${again.code}, file ${statSync(out).size === sizeBefore ? "unchanged" : "CHANGED"}`);
    if (statSync(out).size !== sizeBefore) bad("-n overwrote an existing output");

    // Transcription audio (F6.2, ADR-0028): first audio stream as 16 kHz mono PCM WAV.
    const wav = join(work, "audio.wav");
    const ex = await run(ffmpeg, [...args.common, "-nostdin", ...args.input, "-i", fileArg(join(root, "tests", "fixtures", "media", "mp4-h264-aac.mp4")), ...args.audioWavOutput, fileArg(wav)]);
    const pw = ex.code === 0 ? await run(ffprobe, probeArgv(fileArg(wav))) : null;
    const wj = pw?.code === 0 ? JSON.parse(pw.stdout) : null;
    const ws = wj?.streams ?? [];
    if (wj?.format.format_name !== "wav" || ws.length !== 1 || ws[0].codec_name !== "pcm_s16le" || ws[0].channels !== 1) bad(`audio WAV: exit ${ex.code}, got ${JSON.stringify(wj)}: ${ex.stderr}`);
    else ok("MP4 -> 16 kHz mono PCM WAV for transcription");

    // A 20-minute file: the 3 s fixture looped 400 times, stream copy.
    const long = join(work, "long.mkv");
    const mk = await run(ffmpeg, [...args.common, "-nostdin", "-stream_loop", "399", "-i", fileArg(join(root, "tests", "fixtures", "media", "mkv-h264-aac.mkv")), "-c", "copy", fileArg(long)]);
    if (mk.code !== 0) bad(`cannot build the long MKV: ${mk.stderr}`);
    else {
      const longOut = join(work, "long.mp4");
      const t0 = process.hrtime.bigint();
      const rm2 = await run(ffmpeg, remux(long, longOut));
      const ms = Number(process.hrtime.bigint() - t0) / 1e6;
      const lp = await run(ffprobe, probeArgv(fileArg(longOut)));
      const dur = lp.code === 0 ? Number(JSON.parse(lp.stdout).format.duration) : NaN;
      const bytes = statSync(long).size;
      const result = { platform: `${process.platform}-${process.arch}`, inputBytes: bytes, inputDurationS: Math.round(dur), remuxMs: Math.round(ms), mbPerS: +(bytes / 1e6 / (ms / 1000)).toFixed(1), realtimeFactor: Math.round(dur / (ms / 1000)) };
      if (rm2.code !== 0 || !(dur > 1150)) bad(`long remux failed: exit ${rm2.code}, duration ${dur}: ${rm2.stderr}`);
      else ok(`remux of ${result.inputDurationS} s (${(bytes / 1e6).toFixed(1)} MB) took ${result.remuxMs} ms: ${result.mbPerS} MB/s, ${result.realtimeFactor}x real time`);
      const artifacts = join(root, "test-artifacts", "ffmpeg");
      mkdirSync(artifacts, { recursive: true });
      writeFileSync(join(artifacts, "remux.json"), `${JSON.stringify(result, null, 2)}\n`);
    }
  } finally {
    server.close();
    rmSync(work, { recursive: true, force: true });
  }
  return failures;
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const opt = (name) => {
    const i = process.argv.indexOf(name);
    return i > 0 ? process.argv[i + 1] : undefined;
  };
  const ffmpeg = opt("--ffmpeg") ?? "ffmpeg";
  const ffprobe = opt("--ffprobe") ?? "ffprobe";
  if (!existsSync(ffmpeg) && ffmpeg !== "ffmpeg") throw new Error(`${ffmpeg} not found`);
  const failures = await checkFfmpeg({ ffmpeg, ffprobe, update: process.argv.includes("--update") });
  process.exit(failures.length ? 1 : 0);
}
