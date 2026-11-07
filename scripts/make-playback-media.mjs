#!/usr/bin/env node
// Generates the playback-matrix media in tests/fixtures/media/ (STATE.md F0.8a,
// SPEC DEC-04). Each file is 3 s, 320x180, 24 fps, one keyframe per second, with
// a 440 Hz tone, so the matrix test can check decode, audio, duration and seek.
//
//   node scripts/make-playback-media.mjs
//
// Needs ffmpeg with libx264, libx265, libvpx, libaom (or libsvtav1) and libopus.
// The output is committed; `npm run verify` never runs ffmpeg. Re-run only when
// the matrix itself changes, and commit manifest.json with the media.

import { spawnSync } from "node:child_process";
import { mkdirSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = join(root, "tests", "fixtures", "media");

const VIDEO = {
  h264: ["-c:v", "libx264", "-profile:v", "high", "-pix_fmt", "yuv420p", "-preset", "veryfast", "-crf", "30"],
  // hvc1 tag: what Apple/Chromium expect in MP4/MOV; hev1 is not playable there.
  hevc: ["-c:v", "libx265", "-pix_fmt", "yuv420p", "-preset", "veryfast", "-crf", "32", "-x265-params", "log-level=error"],
  vp8: ["-c:v", "libvpx", "-b:v", "200k", "-deadline", "realtime"],
  vp9: ["-c:v", "libvpx-vp9", "-b:v", "0", "-crf", "45", "-deadline", "realtime", "-cpu-used", "8", "-row-mt", "1"],
  av1: ["-c:v", "libaom-av1", "-crf", "50", "-cpu-used", "8", "-row-mt", "1"],
};
const AUDIO = {
  aac: ["-c:a", "aac", "-b:a", "64k"],
  opus: ["-c:a", "libopus", "-b:a", "48k"],
  vorbis: ["-c:a", "libvorbis", "-q:a", "2"],
};

// id, container, video, audio, extra muxer args, MIME used for canPlayType.
// The ids are stable: the matrix test and ADR-0004 refer to them.
const MATRIX = [
  ["mp4-h264-aac", "mp4", "h264", "aac", ["-movflags", "+faststart"], 'video/mp4; codecs="avc1.640015, mp4a.40.2"'],
  ["mp4-h264-aac-moov-end", "mp4", "h264", "aac", [], 'video/mp4; codecs="avc1.640015, mp4a.40.2"'],
  ["mp4-hevc-aac", "mp4", "hevc", "aac", ["-movflags", "+faststart", "-tag:v", "hvc1"], 'video/mp4; codecs="hvc1.1.6.L60.90, mp4a.40.2"'],
  ["mp4-vp9-opus", "mp4", "vp9", "opus", ["-movflags", "+faststart"], 'video/mp4; codecs="vp09.00.10.08, opus"'],
  ["mp4-av1-opus", "mp4", "av1", "opus", ["-movflags", "+faststart"], 'video/mp4; codecs="av01.0.00M.08, opus"'],
  ["webm-vp9-opus", "webm", "vp9", "opus", [], 'video/webm; codecs="vp9, opus"'],
  ["webm-vp8-vorbis", "webm", "vp8", "vorbis", [], 'video/webm; codecs="vp8, vorbis"'],
  ["webm-av1-opus", "webm", "av1", "opus", [], 'video/webm; codecs="av01.0.00M.08, opus"'],
  ["mkv-h264-aac", "mkv", "h264", "aac", [], 'video/x-matroska; codecs="avc1.640015, mp4a.40.2"'],
  ["mkv-hevc-aac", "mkv", "hevc", "aac", [], 'video/x-matroska; codecs="hvc1.1.6.L60.90, mp4a.40.2"'],
  ["mkv-vp9-opus", "mkv", "vp9", "opus", [], 'video/x-matroska; codecs="vp9, opus"'],
  ["mkv-av1-opus", "mkv", "av1", "opus", [], 'video/x-matroska; codecs="av01.0.00M.08, opus"'],
  ["mov-h264-aac", "mov", "h264", "aac", ["-movflags", "+faststart"], 'video/quicktime; codecs="avc1.640015, mp4a.40.2"'],
  ["mov-hevc-aac", "mov", "hevc", "aac", ["-movflags", "+faststart", "-tag:v", "hvc1"], 'video/quicktime; codecs="hvc1.1.6.L60.90, mp4a.40.2"'],
];

const ffmpegVersion = spawnSync("ffmpeg", ["-version"], { encoding: "utf8" }).stdout?.split("\n")[0];
if (!ffmpegVersion) {
  console.error("ffmpeg not found on PATH");
  process.exit(1);
}

rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });

const entries = [];
for (const [id, container, video, audio, extra, mime] of MATRIX) {
  const file = `${id}.${container}`;
  const argv = [
    "-hide_banner", "-loglevel", "error", "-y",
    "-f", "lavfi", "-i", "testsrc2=size=320x180:rate=24:duration=3",
    "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=3",
    ...VIDEO[video], "-g", "24", "-keyint_min", "24",
    ...AUDIO[audio], "-ac", "2",
    "-map_metadata", "-1", "-fflags", "+bitexact", "-flags:v", "+bitexact", "-flags:a", "+bitexact",
    ...extra,
    join(outDir, file),
  ];
  const r = spawnSync("ffmpeg", argv, { stdio: "inherit" });
  if (r.status !== 0) {
    console.error(`ffmpeg failed for ${id}`);
    process.exit(1);
  }
  const probe = JSON.parse(
    spawnSync("ffprobe", ["-v", "error", "-show_entries", "stream=codec_type,codec_name,codec_tag_string:format=format_name,duration", "-of", "json", join(outDir, file)], { encoding: "utf8" }).stdout,
  );
  const bytes = readFileSync(join(outDir, file));
  entries.push({
    id,
    file,
    container,
    video,
    audio,
    mime,
    bytes: statSync(join(outDir, file)).size,
    sha256: createHash("sha256").update(bytes).digest("hex"),
    durationS: Number(Number(probe.format.duration).toFixed(3)),
    streams: probe.streams.map((s) => `${s.codec_type}:${s.codec_name}${s.codec_tag_string && !s.codec_tag_string.startsWith("[") ? `/${s.codec_tag_string}` : ""}`),
  });
  console.log(`${file}: ${entries.at(-1).bytes} bytes, ${entries.at(-1).streams.join(", ")}`);
}

writeFileSync(
  join(outDir, "manifest.json"),
  JSON.stringify({ generatedBy: "scripts/make-playback-media.mjs", ffmpeg: ffmpegVersion, files: entries }, null, 2) + "\n",
);
console.log(`\n${readdirSync(outDir).length - 1} files in ${outDir}`);
