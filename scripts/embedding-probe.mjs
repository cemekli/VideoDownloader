#!/usr/bin/env node
// Embedding benchmark for DEC-12 / ADR-0035 (F7.1). Not part of CI: it
// downloads the newest llama.cpp CPU build and about 2 GB of candidate GGUF
// models. Run it on Linux with network access (the cloud container cannot
// reach GitHub release assets or Hugging Face; it ran once in CI on
// 14 Nov 2026, results in docs/decisions.md ADR-0035):
//
// 1. the newest llama.cpp release: both CPU archives, llama-server and its
//    libraries with SHA-256 (to pin tools/manifest.json);
// 2. candidate multilingual embedding models: commit, files, sizes, SHA-256;
// 3. with --bench, each model in a loopback llama-server as the app runs it
//    (random port and key; the routes without the key) over the TR/EN fixture
//    tests/fixtures/semantic-tr-en.json: hit@1, recall@k, MRR, time.
//
//   node scripts/embedding-probe.mjs [--bench]   (report: test-artifacts/embedding-probe.md)

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { createWriteStream, existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
// Downloads stay out of test-artifacts (CI uploads that folder).
const work = join(tmpdir(), "vv-embedding-probe");
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
// Since b10878 (Sep 2026) or earlier no release ships `llama-embedding`;
// embeddings come from `llama-server --embedding`. Pin the newest release:
// both CPU archives, the server and every library with its SHA-256.
const gh = process.env.GITHUB_TOKEN ? { Authorization: `Bearer ${process.env.GITHUB_TOKEN}` } : {};
let llamaBin = null;
const linRe = /^llama-b\d+-bin-ubuntu-x64\.(zip|tar\.gz)$/;
const winRe = /^llama-b\d+-bin-win-cpu-x64\.zip$/;
function unpack(file, dir) {
  mkdirSync(dir, { recursive: true });
  if (file.endsWith(".zip")) execFileSync("unzip", ["-o", "-q", file, "-d", dir]);
  else execFileSync("tar", ["-xzf", file, "-C", dir]);
  const files = [];
  const walk = (d) => {
    for (const n of readdirSync(d)) {
      const p = join(d, n);
      if (statSync(p).isDirectory()) walk(p);
      else files.push(p);
    }
  };
  walk(dir);
  return files;
}
try {
  const rels = await getJson("https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=10", gh);
  const rel = rels.find((r) => !r.draft && r.assets.some((a) => linRe.test(a.name)) && r.assets.some((a) => winRe.test(a.name)));
  log(`## llama.cpp ${rel.tag_name} (${rel.published_at}) prerelease=${rel.prerelease}`);
  log(rel.html_url);
  for (const pat of [linRe, winRe]) {
    const a = rel.assets.find((x) => pat.test(x.name));
    const dest = await download(a.browser_download_url, join(work, a.name));
    log(`### ${a.name} ${a.browser_download_url} sha256=${sha256(dest)} size=${statSync(dest).size} digest=${a.digest ?? "?"}`);
    const dir = join(work, pat === linRe ? "llama" : "llama-win");
    const files = unpack(dest, dir);
    for (const f of files) {
      const rel2 = f.slice(dir.length + 1).replaceAll("\\", "/");
      if (pat === winRe && !/\.dll$|llama-server\.exe$|LICENSE/i.test(rel2)) continue;
      if (pat === linRe && !/llama-server$|\.so|LICENSE/.test(rel2)) continue;
      log(`    ${rel2} size=${statSync(f).size} sha256=${sha256(f)}`);
    }
    if (pat === linRe) llamaBin = files.find((f) => /[\\/]llama-server$/.test(f)) ?? null;
  }
  if (llamaBin) {
    execFileSync("chmod", ["+x", llamaBin]);
    for (const f of readdirSync(dirname(llamaBin))) if (!f.includes(".")) execFileSync("chmod", ["+x", join(dirname(llamaBin), f)]);
    for (const args of [["--version"], ["--help"]]) {
      let t;
      try {
        const r = execFileSync(llamaBin, args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], timeout: 20000 });
        t = `(exit 0, stdout)\n${r}`;
      } catch (e) {
        t = `(exit ${e.status})\n${e.stdout ?? ""}\n[stderr]\n${e.stderr ?? ""}`;
      }
      const keep = args[0] === "--version" ? () => true : (l) => /embd|embed|pool|api-key|--host|--port|webui|parallel|threads|ctx-size|batch|warmup|log-|offline|--path|metrics|slots|props|reuse/i.test(l);
      log(`  --- llama-server ${args[0]} ${t.split("\n").filter(keep).slice(0, 90).join("\n")}`);
    }
    try {
      log("  ldd:\n" + execFileSync("ldd", [llamaBin], { encoding: "utf8" }));
    } catch {}
  }
} catch (e) {
  log(`llama.cpp: ${e.message}`);
}

// ---- 2. candidate models ------------------------------------------------------
const candidates = [
  { repo: "Qwen/Qwen3-Embedding-0.6B-GGUF", file: /Q8_0\.gguf$/, kind: "qwen3" },
  { repo: "ggml-org/embeddinggemma-300M-GGUF", file: /Q8_0\.gguf$/, kind: "gemma" },
  { repo: "ggml-org/bge-m3-Q8_0-GGUF", file: /\.gguf$/, kind: "plain" },
  { repo: "ggml-org/embeddinggemma-2-GGUF", file: /Q8_0\.gguf$/, kind: "gemma" },
  { repo: "cstr/multilingual-e5-small-GGUF", file: /q8_0\.gguf$/i, kind: "e5" },
];
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
// One server per model, as the app runs it: loopback, a random port and key.
import { spawn } from "node:child_process";
async function startServer(model) {
  const port = 20000 + Math.floor(Math.random() * 20000);
  const key = createHash("sha256").update(String(Math.random())).digest("hex");
  const args = ["-m", model, "--embedding", "--embd-normalize", "2", "--host", "127.0.0.1", "--port", String(port), "--api-key", key, "-c", "1024", "-b", "1024", "-ub", "1024", "-t", "4", "--parallel", "1", "--no-webui"];
  log(`  start: llama-server ${args.join(" ").replace(key, "<key>")}`);
  const child = spawn(llamaBin, args, { stdio: ["ignore", "pipe", "pipe"] });
  let errText = "";
  child.stdout.on("data", (d) => (errText += d));
  child.stderr.on("data", (d) => (errText += d));
  const t0 = Date.now();
  for (;;) {
    if (child.exitCode !== null) throw new Error(`server exited ${child.exitCode}: ${errText.slice(-1500)}`);
    try {
      const r = await fetch(`http://127.0.0.1:${port}/health`);
      const body = await r.text();
      if (r.status === 200) {
        log(`  ready in ${Date.now() - t0} ms; /health ${r.status} ${body.slice(0, 200)}; headers ${JSON.stringify([...r.headers])}`);
        break;
      }
    } catch {}
    if (Date.now() - t0 > 120000) throw new Error(`no health after 120 s: ${errText.slice(-1500)}`);
    await new Promise((r) => setTimeout(r, 200));
  }
  // Without the key, and an unknown route.
  for (const [path, init] of [["/v1/embeddings", { method: "POST", body: "{}" }], ["/v1/models", {}], ["/props", {}], ["/slots", {}], ["/metrics", {}], ["/", {}]]) {
    try {
      const r = await fetch(`http://127.0.0.1:${port}${path}`, init);
      log(`  no key ${init.method ?? "GET"} ${path}: ${r.status} ${(await r.text()).slice(0, 160).replace(/\s+/g, " ")}`);
    } catch (e) {
      log(`  no key ${path}: ${e.message}`);
    }
  }
  return {
    child,
    log: () => errText,
    async embed(texts) {
      const t1 = Date.now();
      const r = await fetch(`http://127.0.0.1:${port}/v1/embeddings`, {
        method: "POST",
        headers: { "Content-Type": "application/json", Authorization: `Bearer ${key}` },
        body: JSON.stringify({ input: texts, encoding_format: "float" }),
      });
      const raw = await r.text();
      if (r.status !== 200) throw new Error(`embeddings ${r.status}: ${raw.slice(0, 500)}`);
      const j = JSON.parse(raw);
      if (!this.shown) {
        this.shown = true;
        log(`  response headers ${JSON.stringify([...r.headers])}; keys ${Object.keys(j)}; data[0] keys ${Object.keys(j.data[0])}; usage ${JSON.stringify(j.usage)}; dims ${j.data[0].embedding.length}; first ${j.data[0].embedding.slice(0, 3)}`);
      }
      return { vecs: j.data.sort((a, b) => a.index - b.index).map((d) => d.embedding), ms: Date.now() - t1 };
    },
    stop() {
      child.kill();
    },
  };
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
        try {
      const dest = await download(`https://huggingface.co/${c.repo}/resolve/${c.sha}/${c.path}`, join(work, c.path.split("/").pop()));
      const p = prefixes[c.kind];
      const srv = await startServer(dest);
      let P, Q;
      try {
        P = await srv.embed(fx.passages.map((x) => p.p(x.text)));
        Q = await srv.embed(fx.queries.map((x) => p.q(x.text)));
        // A long input: does the server cut it or refuse it?
        try {
          await srv.embed(["ş".repeat(5000)]);
          log("  5000-char input: accepted");
        } catch (e) {
          log(`  5000-char input: ${e.message.slice(0, 300)}`);
        }
      } finally {
        log(`  server log tail: ${srv.log().split("\n").filter((l) => /pooling|n_embd|ctx|error|warn|model type|threads/i.test(l)).slice(0, 25).join(" | ")}`);
        srv.stop();
      }
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
mkdirSync(join(root, "test-artifacts"), { recursive: true });
writeFileSync(join(root, "test-artifacts", "embedding-probe.md"), out.join("\n") + "\n");
