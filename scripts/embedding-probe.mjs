#!/usr/bin/env node
// One-off probe for F7.1 (DEC-12), run in CI because the development
// container cannot reach GitHub release assets or Hugging Face:
//
// 1. the latest llama.cpp release: tag, assets and SHA-256 of the CPU builds
//    (to pin `llama-embedding` in tools/manifest.json like whisper-cli);
// 2. candidate multilingual embedding models in GGUF: commit, files, sizes,
//    SHA-256 (to pin them like tools/models.json);
// 3. on Linux, the real `llama-embedding` with each candidate over the TR/EN
//    fixture tests/fixtures/semantic-tr-en.json: hit@1, recall@k, MRR, time.
//
//   node scripts/embedding-probe.mjs [--bench]

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { createWriteStream, existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const work = join(root, "test-artifacts", "embedding-probe");
mkdirSync(work, { recursive: true });
const bench = process.argv.includes("--bench");
const isWin = process.platform === "win32";
const out = [];
const log = (s = "") => {
  console.log(s);
  out.push(s);
};

async function getJson(url, headers = {}) {
  const res = await fetch(url, { headers });
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  return res.json();
}

async function download(url, dest) {
  if (existsSync(dest)) return dest;
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  await pipeline(Readable.fromWeb(res.body), createWriteStream(dest + ".part"));
  execFileSync(isWin ? "cmd" : "mv", isWin ? ["/c", "move", "/y", dest + ".part", dest] : [dest + ".part", dest]);
  return dest;
}

function sha256(path) {
  const h = createHash("sha256");
  h.update(readFileSync(path));
  return h.digest("hex");
}

// ---- 1. llama.cpp release -------------------------------------------------
const gh = process.env.GITHUB_TOKEN ? { Authorization: `Bearer ${process.env.GITHUB_TOKEN}` } : {};
let llamaBin = null;
try {
  const rel = await getJson("https://api.github.com/repos/ggml-org/llama.cpp/releases/latest", gh);
  log(`## llama.cpp ${rel.tag_name} (${rel.published_at})`);
  log(rel.html_url);
  for (const a of rel.assets) {
    if (/bin-(win-cpu-x64|ubuntu-x64)\.(zip|tar\.gz)$/.test(a.name)) {
      log(`- ${a.name} size=${a.size} digest=${a.digest ?? "?"} url=${a.browser_download_url}`);
    }
  }
  const want = rel.assets.find((a) => (isWin ? /bin-win-cpu-x64\.zip$/ : /bin-ubuntu-x64\.(zip|tar\.gz)$/).test(a.name));
  if (want) {
    const dest = await download(want.browser_download_url, join(work, want.name));
    log(`  downloaded ${want.name}: sha256=${sha256(dest)} size=${statSync(dest).size}`);
    const dir = join(work, "llama");
    mkdirSync(dir, { recursive: true });
    if (want.name.endsWith(".zip")) {
      if (isWin) execFileSync("tar", ["-xf", dest, "-C", dir]);
      else execFileSync("unzip", ["-o", "-q", dest, "-d", dir]);
    } else execFileSync("tar", ["-xzf", dest, "-C", dir]);
    const files = [];
    const walk = (d) => {
      for (const n of readdirSync(d)) {
        const p = join(d, n);
        if (statSync(p).isDirectory()) walk(p);
        else files.push(p);
      }
    };
    walk(dir);
    log(`  entries (${files.length}):`);
    for (const f of files) {
      const rel2 = f.slice(dir.length + 1).replaceAll("\\", "/");
      if (/llama-embedding|\.(dll|so[.0-9]*)$|LICENSE/i.test(rel2)) log(`    ${rel2} size=${statSync(f).size} sha256=${sha256(f)}`);
    }
    llamaBin = files.find((f) => /llama-embedding(\.exe)?$/.test(f)) ?? null;
    if (llamaBin) {
      if (!isWin) execFileSync("chmod", ["+x", llamaBin]);
      try {
        log("  " + execFileSync(llamaBin, ["--version"], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim());
      } catch (e) {
        log(`  --version: ${(e.stderr ?? e.message).toString().trim().split("\n").slice(-3).join(" | ")}`);
      }
      if (!isWin) {
        try {
          log("  ldd:\n" + execFileSync("ldd", [llamaBin], { encoding: "utf8" }));
        } catch {}
      }
    }
  }
} catch (e) {
  log(`llama.cpp: ${e.message}`);
}

// ---- 2. candidate models ------------------------------------------------------
const candidates = [
  { repo: "Qwen/Qwen3-Embedding-0.6B-GGUF", file: /Q8_0\.gguf$/, kind: "qwen3" },
  { repo: "ggml-org/Qwen3-Embedding-0.6B-GGUF", file: /Q8_0\.gguf$/, kind: "qwen3" },
  { repo: "ggml-org/embeddinggemma-300M-GGUF", file: /Q8_0\.gguf$/, kind: "gemma" },
  { repo: "ggml-org/bge-m3-Q8_0-GGUF", file: /\.gguf$/, kind: "plain" },
  { repo: "ggml-org/multilingual-e5-small-Q8_0-GGUF", file: /\.gguf$/, kind: "e5" },
  { repo: "ggml-org/multilingual-e5-base-Q8_0-GGUF", file: /\.gguf$/, kind: "e5" },
];
for (const q of ["multilingual-e5-small gguf", "multilingual-e5-base gguf", "bge-m3 gguf", "Qwen3-Embedding-0.6B gguf", "embeddinggemma gguf"]) {
  try {
    const list = await getJson(`https://huggingface.co/api/models?search=${encodeURIComponent(q.replace(" gguf", ""))}&filter=gguf&sort=downloads&direction=-1&limit=6`);
    log(`## search '${q}': ${list.map((m) => `${m.id} (${m.downloads ?? "?"})`).join(", ")}`);
  } catch (e) {
    log(`## search '${q}': ${e.message}`);
  }
}
const ready = [];
for (const c of candidates) {
  try {
    const info = await getJson(`https://huggingface.co/api/models/${c.repo}`);
    const tree = await getJson(`https://huggingface.co/api/models/${c.repo}/tree/${info.sha}`);
    const license = info.cardData?.license ?? info.tags?.find((t) => t.startsWith("license:")) ?? "?";
    log(`## ${c.repo}@${info.sha} license=${license} base=${info.cardData?.base_model ?? "?"}`);
    for (const e of tree) if (e.path.endsWith(".gguf")) log(`- ${e.path} size=${e.lfs?.size ?? e.size} sha256=${e.lfs?.oid ?? "?"}`);
    const pick = tree.find((e) => c.file.test(e.path));
    if (pick) ready.push({ ...c, sha: info.sha, path: pick.path, sha256: pick.lfs?.oid, size: pick.lfs?.size });
  } catch (e) {
    log(`## ${c.repo}: ${e.message}`);
  }
}

// ---- 3. retrieval benchmark ----------------------------------------------------
const prefixes = {
  qwen3: {
    q: (t) => `Instruct: Given a search query, retrieve relevant passages from video transcripts that answer the query\nQuery:${t}`,
    p: (t) => t,
  },
  gemma: { q: (t) => `task: search result | query: ${t}`, p: (t) => `title: none | text: ${t}` },
  e5: { q: (t) => `query: ${t}`, p: (t) => `passage: ${t}` },
  plain: { q: (t) => t, p: (t) => t },
};
const SEP = "<#vv-sep#>";
function embed(model, texts) {
  const file = join(work, "prompts.txt");
  writeFileSync(file, texts.join(SEP));
  const t0 = Date.now();
  const stdout = execFileSync(
    llamaBin,
    ["-m", model, "-f", file, "--embd-separator", SEP, "--embd-output-format", "json", "--embd-normalize", "2", "-c", "2048", "-b", "2048", "-ub", "2048", "-t", "4"],
    { encoding: "utf8", maxBuffer: 1 << 28, stdio: ["ignore", "pipe", "pipe"] },
  );
  const ms = Date.now() - t0;
  const json = JSON.parse(stdout.slice(stdout.indexOf("{")));
  return { vecs: json.data.sort((a, b) => a.index - b.index).map((d) => d.embedding), ms };
}
const dot = (a, b) => a.reduce((s, x, i) => s + x * b[i], 0);
function norm(v, dims) {
  const w = dims ? v.slice(0, dims) : v;
  const n = Math.sqrt(dot(w, w)) || 1;
  return w.map((x) => x / n);
}

if (bench && llamaBin && !isWin) {
  const fx = JSON.parse(readFileSync(join(root, "tests", "fixtures", "semantic-tr-en.json"), "utf8"));
  for (const c of ready) {
    if (c.repo.startsWith("ggml-org/Qwen3") && ready.some((r) => r.repo.startsWith("Qwen/"))) continue;
    try {
      const dest = await download(`https://huggingface.co/${c.repo}/resolve/${c.sha}/${c.path}`, join(work, c.path.split("/").pop()));
      const p = prefixes[c.kind];
      const P = embed(dest, fx.passages.map((x) => p.p(x.text)));
      const Q = embed(dest, fx.queries.map((x) => p.q(x.text)));
      for (const dims of [null, 512, 256]) {
        if (dims && dims >= P.vecs[0].length) continue;
        if (dims && c.kind !== "qwen3" && c.kind !== "gemma") continue; // only MRL-trained models truncate
        let hit1 = 0, rec = 0, mrr = 0;
        const lines = [];
        fx.queries.forEach((q, qi) => {
          const qv = norm(Q.vecs[qi], dims);
          const ranked = fx.passages
            .map((x, pi) => ({ id: x.id, s: dot(qv, norm(P.vecs[pi], dims)) }))
            .sort((a, b) => b.s - a.s);
          const k = Math.max(3, q.relevant.length);
          const top = ranked.slice(0, k).map((r) => r.id);
          rec += q.relevant.filter((r) => top.includes(r)).length / q.relevant.length;
          if (q.relevant.includes(ranked[0].id)) hit1++;
          const first = ranked.findIndex((r) => q.relevant.includes(r.id));
          mrr += 1 / (first + 1);
          lines.push(`  ${q.id} "${q.text}": ${ranked.slice(0, 3).map((r) => `${r.id} ${r.s.toFixed(3)}`).join(", ")}`);
        });
        const n = fx.queries.length;
        log(`## bench ${c.repo} ${c.path} dims=${dims ?? P.vecs[0].length}: hit@1 ${hit1}/${n}, recall@k ${(rec / n).toFixed(3)}, MRR ${(mrr / n).toFixed(3)}; embed ${fx.passages.length} passages ${P.ms} ms, ${n} queries ${Q.ms} ms`);
        if (!dims) for (const l of lines) log(l);
      }
    } catch (e) {
      log(`## bench ${c.repo}: ${(e.stderr ?? e.message).toString().slice(-800)}`);
    }
  }
}
writeFileSync(join(work, "report.md"), out.join("\n") + "\n");
