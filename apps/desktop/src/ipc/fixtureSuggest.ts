// Fixture tag suggestions (STATE.md F7.3). Same shapes as the backend
// (`suggest_commands.rs`). No chat API until the user sets one; an address
// with "unreachable" in it does not answer, and another host needs
// `allowRemote`. Suggestions come from the item's title words, so tests
// can predict them; an accept goes through the fixture `tagItems`, so it
// is an ordinary undoable "add tag".
import { IpcError } from "./errors";
import type { FixtureRecord } from "./fixture";
import type { OrganizeClient, ProposalDto, ProposalGroupDto, SuggestApiDto, SuggestClient, SuggestRunDto, TagSuggestDto } from "./types";

const err = (code: string, cmd: string) =>
  new IpcError(code, `${cmd}: error.${code.toLowerCase()}`, { code, messageKey: `error.${code.toLowerCase()}`, retryable: false, suggestedAction: null, diagnosticId: cmd, safeDetails: {} });

const isLocal = (u: string) => /^https?:\/\/(localhost|127\.[0-9.]+|\[::1\])(:[0-9]+)?(\/|$)/i.test(u);
const norm = (s: string) => s.trim().toLocaleLowerCase("tr");

/** Up to three words of four letters or more from the title, lower case. */
export function fixtureTags(title: string | null): string[] {
  const words = (title ?? "").split(/[^\p{L}\p{N}]+/u).filter((w) => w.length >= 4);
  return [...new Set(words.map((w) => w.toLocaleLowerCase("tr")))].slice(0, 3);
}

export function createFixtureSuggest(records: FixtureRecord[], organize: Pick<OrganizeClient, "tagItems" | "getSidebar">, stepMs = 200): SuggestClient {
  let api: SuggestApiDto | null = null;
  let run: SuggestRunDto | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let clock = 1_793_700_000_000;
  const now = () => (clock += 1_000);
  /** Every proposal ever made, with its status. */
  const all: (ProposalDto & { status: "pending" | "accepted" | "rejected" })[] = [];
  const asked = new Set<string>();
  let seq = 0;

  const pending = (itemId: string) => all.filter((p) => p.itemId === itemId && p.status === "pending").map(({ status: _s, ...p }) => p);
  const liveIds = () => new Set(records.filter((r) => !r.removed).map((r) => r.id));
  const view = (): TagSuggestDto => {
    const live = liveIds();
    return structuredClone({ api, run, pending: all.filter((p) => p.status === "pending" && live.has(p.itemId)).length });
  };

  const suggest = (r: FixtureRecord, force: boolean): "asked" | "current" | "no_text" => {
    if (!api) throw err("MODEL_MISSING", "suggest_tags");
    if (!r.title) return "no_text";
    if (asked.has(r.id) && !force) return "current";
    asked.add(r.id);
    const settled = new Set([...all.filter((p) => p.itemId === r.id && p.status !== "pending").map((p) => norm(p.value)), ...r.tags.map(norm)]);
    for (let i = all.length - 1; i >= 0; i--) if (all[i]!.itemId === r.id && all[i]!.status === "pending") all.splice(i, 1);
    for (const v of fixtureTags(r.title)) {
      if (settled.has(norm(v))) continue;
      all.push({ id: `00000000-0000-4000-8000-0000000a${String(++seq).padStart(4, "0")}`, itemId: r.id, type: "tag", value: v, modelId: api.model, status: "pending" });
    }
    return "asked";
  };

  const step = (items: FixtureRecord[], i: number, force: boolean) => {
    if (!run || !run.running) return;
    if (i >= items.length) {
      run = { ...run, running: false, finishedAt: now() };
      return;
    }
    const before = all.length;
    const o = suggest(items[i]!, force);
    run = { ...run, done: i + 1, asked: run.asked + (o === "asked" ? 1 : 0), suggested: run.suggested + Math.max(0, all.length - before) };
    timer = setTimeout(() => step(items, i + 1, force), stepMs);
  };

  return {
    async getTagSuggest() {
      return view();
    },
    async setSuggestApi(url, model, key, allowRemote) {
      let u: URL;
      try {
        u = new URL(url.trim());
      } catch {
        throw err("INVALID_URL", "set_suggest_api");
      }
      if (!/^https?:$/.test(u.protocol)) throw err("INVALID_URL", "set_suggest_api");
      if (u.username || u.password) throw err("URL_HAS_CREDENTIALS", "set_suggest_api");
      const base = url.trim().replace(/\/+$/, "");
      if (!isLocal(base) && !allowRemote) throw err("URL_BLOCKED", "set_suggest_api");
      if (base.includes("unreachable")) throw err("NETWORK_UNAVAILABLE", "set_suggest_api");
      if (model.trim() === "missing") throw err("MODEL_MISSING", "set_suggest_api");
      const hasKey = key === null ? (api?.hasKey ?? false) : key !== "";
      api = { url: base, model: model.trim(), hasKey, local: isLocal(base), checkedAt: now() };
      return view();
    },
    async clearSuggestApi() {
      api = null;
      if (run?.running) run = { ...run, running: false, canceled: true, finishedAt: now() };
      return view();
    },
    async suggestAll(force) {
      if (!api) throw err("MODEL_MISSING", "suggest_all");
      if (!run?.running) {
        const items = records.filter((r) => !r.removed);
        run = { running: true, done: 0, total: items.length, asked: 0, suggested: 0, failed: 0, errorCode: null, canceled: false, startedAt: now(), finishedAt: null };
        timer = setTimeout(() => step(items, 0, force), stepMs);
      }
      return view();
    },
    async cancelSuggestAll() {
      if (timer) clearTimeout(timer);
      if (run?.running) run = { ...run, running: false, canceled: true, finishedAt: now() };
      return view();
    },
    async listAiProposals(itemId) {
      return pending(itemId);
    },
    async suggestTags(itemId, force) {
      const r = records.find((x) => x.id === itemId);
      if (!r) throw err("INTERNAL_ERROR", "suggest_tags");
      const outcome = suggest(r, force);
      return { outcome, proposals: pending(itemId) };
    },
    async decideAiProposal(id, accept, name) {
      const p = all.find((x) => x.id === id);
      if (!p || p.status !== "pending") throw err("INTERNAL_ERROR", "decide_ai_proposal");
      const op = accept ? await organize.tagItems([p.itemId], (name ?? "").trim() || p.value) : null;
      p.status = accept ? "accepted" : "rejected";
      return { proposals: pending(p.itemId), op };
    },
    async listAiProposalGroups() {
      const live = liveIds();
      const tags = new Map((await organize.getSidebar()).tags.map((x) => [norm(x.name), x.name]));
      const groups = new Map<string, ProposalGroupDto>();
      // Newest item first, like the backend.
      const order = [...records].sort((a, b) => b.addedAt - a.addedAt).map((r) => r.id);
      const pend = all.filter((p) => p.status === "pending" && live.has(p.itemId)).sort((a, b) => order.indexOf(a.itemId) - order.indexOf(b.itemId));
      for (const p of pend) {
        const key = norm(p.value);
        let g = groups.get(key);
        if (!g) {
          g = { value: tags.get(key) ?? p.value, normalizedValue: key, tagExists: tags.has(key), items: [] };
          groups.set(key, g);
        }
        g.items.push({ proposalId: p.id, itemId: p.itemId, title: records.find((r) => r.id === p.itemId)?.title ?? null });
      }
      return [...groups.values()].sort((a, b) => b.items.length - a.items.length || a.normalizedValue.localeCompare(b.normalizedValue));
    },
    async decideAiProposals(ids, accept, name) {
      const ps = [...new Set(ids)].map((id) => all.find((x) => x.id === id));
      if (ps.some((p) => !p || p.status !== "pending")) throw err("INTERNAL_ERROR", "decide_ai_proposals");
      const list = ps as (typeof all)[number][];
      if (list.length === 0) return { decided: 0, op: null };
      const op = accept ? await organize.tagItems([...new Set(list.map((p) => p.itemId))], (name ?? "").trim() || list[0]!.value) : null;
      for (const p of list) p.status = accept ? "accepted" : "rejected";
      return { decided: list.length, op };
    },
  };
}
