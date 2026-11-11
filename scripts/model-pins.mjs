#!/usr/bin/env node
// Pins the transcription models in tools/models.json (STATE.md F6.1,
// ADR-0026) from the Hugging Face file listing of the whisper.cpp model
// repository.
//
//   node scripts/model-pins.mjs           # compare the pins with the listing
//   node scripts/model-pins.mjs --write   # pin revision, sha256 and size
//   node scripts/model-pins.mjs --download whisper-tiny
//                                         # also download one model and check
//                                         # its SHA-1 (published by whisper.cpp)
//                                         # and SHA-256 (pinned) end to end
//   node scripts/model-pins.mjs --download whisper-tiny --save test-models
//                                         # and keep it there (the real
//                                         # whisper-cli test in CI, ADR-0028)
//
// The listing gives each LFS file's SHA-256 and size at one commit. The app
// downloads from that commit only, so a later push to the repository cannot
// change what a pinned model is. The SHA-1 in the JSON comes from a second
// source (whisper.cpp models/README.md); --download checks both agree.

import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const file = join(root, "tools", "models.json");
const catalog = JSON.parse(readFileSync(file, "utf8"));
const repo = catalog.repository.replace("https://huggingface.co/", "");
const write = process.argv.includes("--write");
const dlIdx = process.argv.indexOf("--download");
const downloadId = dlIdx > 0 ? process.argv[dlIdx + 1] : null;
const saveIdx = process.argv.indexOf("--save");
const saveDir = saveIdx > 0 ? process.argv[saveIdx + 1] : null;

const fail = (m) => {
  console.error(`✗ ${m}`);
  process.exit(1);
};

async function json(url) {
  const res = await fetch(url);
  if (!res.ok) fail(`${url}: HTTP ${res.status}`);
  return res.json();
}

const revision = catalog.revision ?? (await json(`https://huggingface.co/api/models/${repo}`)).sha;
if (!/^[0-9a-f]{40}$/.test(revision ?? "")) fail(`no commit id for ${repo}`);
const tree = await json(`https://huggingface.co/api/models/${repo}/tree/${revision}`);
let problems = 0;
for (const m of catalog.models) {
  const entry = tree.find((e) => e.path === m.file);
  if (!entry?.lfs) fail(`${m.file} is not an LFS file at ${revision}`);
  const sha256 = entry.lfs.oid ?? entry.lfs.sha256;
  const size = entry.lfs.size ?? entry.size;
  console.log(`${m.id}: ${m.file} size=${size} sha256=${sha256}`);
  if (write) {
    m.sha256 = sha256;
    m.size = size;
  } else if (m.sha256 !== sha256 || m.size !== size) {
    console.error(`✗ ${m.id}: pinned ${m.sha256}/${m.size}, listing says ${sha256}/${size}`);
    problems++;
  }
}
if (write) {
  catalog.revision = revision;
  writeFileSync(file, JSON.stringify(catalog, null, 2) + "\n");
  console.log(`✓ pinned ${catalog.models.length} models at ${revision}`);
} else if (problems) {
  process.exit(1);
} else {
  console.log(`✓ pins match ${repo}@${revision}`);
}

if (downloadId) {
  const m = catalog.models.find((x) => x.id === downloadId) ?? fail(`unknown model ${downloadId}`);
  const url = `${catalog.repository}/resolve/${revision}/${m.file}`;
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) fail(`${url}: HTTP ${res.status}`);
  const buf = Buffer.from(await res.arrayBuffer());
  const sha1 = createHash("sha1").update(buf).digest("hex");
  const sha256 = createHash("sha256").update(buf).digest("hex");
  console.log(`${m.id}: downloaded ${buf.length} bytes sha1=${sha1} sha256=${sha256}`);
  if (sha1 !== m.sha1) fail(`${m.id}: SHA-1 ${sha1} is not the published ${m.sha1}`);
  if (sha256 !== m.sha256 || buf.length !== m.size) fail(`${m.id}: download does not match the pin`);
  console.log(`✓ ${m.id} matches both the published SHA-1 and the pinned SHA-256`);
  if (saveDir) {
    mkdirSync(saveDir, { recursive: true });
    writeFileSync(join(saveDir, m.file), buf);
    console.log(`✓ saved as ${join(saveDir, m.file)}`);
  }
}
