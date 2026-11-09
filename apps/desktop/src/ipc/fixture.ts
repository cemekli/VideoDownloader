// Deterministic in-memory backend for `vite --mode fixture`, Vitest and the
// Playwright fixture E2E (SPEC §25.2). It mirrors what `list_items` does on
// the backend closely enough to drive every Library state; the real query
// logic is tested in Rust (vv-persistence search tests, F1.6).

import libraryFixture from "@fixtures/library-small.json";
import contract from "@fixtures/ipc-contract.json";
import { IpcError } from "./errors";
import { fixtureMedia } from "./fixtureData";
import { fixtureParseUrl } from "./fixtureUrl";
import { createFixtureOrganize, createOrganizeState, matchesRules, tagKey, tagRefs, type OrganizeState } from "./fixtureOrganize";
import type {
  AssetDto,
  ImportStatus,
  ImportSummary,
  ItemDetail,
  ItemKind,
  ItemPage,
  LibraryClient,
  LibraryItemDto,
  ListQuery,
  SaveUrlResult,
  UrlCheck,
  LocalState,
  Platform,
  Snippet,
} from "./types";

/** 2026-11-01T00:00:00Z: "now" for fixture data, so Recently Added is stable. */
export const FIXTURE_NOW = 1_793_491_200_000;
const DAY = 86_400_000;
export const RECENT_DAYS = 30;

/** Named data sets, picked with `?fixture=<name>` in fixture mode. */
/** The one asset of a fixture item: its id with the last group marked "a". */
export const fixtureAssetId = (itemId: string) => `${itemId.slice(0, 24)}a${itemId.slice(25)}`;

/** `download-error`: the first try of every download fails on the network; a retry succeeds (Journey 6). */
export type FixtureScenario = "default" | "empty" | "many" | "error" | "slow" | "first-run" | "download-error";

export interface FixtureRecord extends Omit<LibraryItemDto, "snippet" | "removedAt"> {
  creator: string | null;
  description: string | null;
  canonicalUrl: string | null;
  notes: string;
  fileName: string | null;
  archived: boolean;
  /** In Recently removed: hidden from lists, offered back by Add URL (AC-45). */
  removed: boolean;
  removedAt: number | null;
  collectionIds: string[];
  revision: number;
}

interface RawFixtureItem {
  id: string;
  title: string;
  platform: string;
  kind: string;
  localState: string;
  durationMs: number | null;
  tags: string[];
  addedAt: number;
  isFavorite: boolean;
  creator: string | null;
  description: string | null;
  canonicalUrl: string | null;
  notes: string;
  fixtureThumbnail: boolean;
  fileName: string | null;
  removed?: boolean;
}

/** A plain coloured 16:9 tile (no text, no network) standing in for a thumbnail. */
function thumb(seed: string): string {
  let x = 0x811c9dc5;
  for (const ch of seed) x = Math.imul(x ^ ch.charCodeAt(0), 0x01000193) >>> 0;
  const h = x % 360;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 9"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="hsl(${h},45%,38%)"/><stop offset="1" stop-color="hsl(${(h + 50) % 360},50%,22%)"/></linearGradient></defs><rect width="16" height="9" fill="url(#g)"/><circle cx="11.5" cy="3" r="1.4" fill="hsl(${h},60%,70%)" opacity=".5"/></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

export function fixtureRecords(): FixtureRecord[] {
  return (libraryFixture.items as RawFixtureItem[]).map((r) => ({
    id: r.id,
    title: r.title,
    platform: r.platform as Platform,
    kind: r.kind as ItemKind,
    localState: r.localState as LocalState,
    durationMs: r.durationMs,
    tags: r.tags,
    thumbnailUrl: r.fixtureThumbnail ? thumb(r.id) : null,
    addedAt: r.addedAt,
    isFavorite: r.isFavorite,
    creator: r.creator,
    description: r.description,
    canonicalUrl: r.canonicalUrl,
    notes: r.notes,
    fileName: r.fileName,
    archived: false,
    removed: r.removed ?? false,
    removedAt: r.removed ? r.addedAt + DAY : null,
    collectionIds: [],
    revision: 1,
  }));
}

/** `n` synthetic items for the large-library state (virtualized grid). */
export function manyRecords(n: number): FixtureRecord[] {
  const platforms: Platform[] = ["youtube", "instagram", "tiktok", "x", "vk", "local"];
  const states: LocalState[] = ["bookmark_only", "local_ready", "bookmark_only", "missing", "local_ready", "disconnected"];
  return Array.from({ length: n }, (_, i) => {
    const platform = platforms[i % platforms.length]!;
    const kind: ItemKind = platform === "local" ? "local_video" : i % 3 === 0 ? "remote_video" : "bookmark";
    const localState: LocalState = platform === "local" ? (i % 2 ? "local_ready" : "disconnected") : kind === "bookmark" ? "bookmark_only" : states[i % states.length]!;
    const id = `00000000-0000-4000-9000-${String(i + 1).padStart(12, "0")}`;
    return {
      id,
      title: `Sample video ${i + 1}`,
      platform,
      kind,
      localState,
      durationMs: i % 7 === 0 ? null : ((i * 37) % 5400) * 1000,
      tags: [],
      thumbnailUrl: i % 5 === 0 ? null : thumb(id),
      addedAt: FIXTURE_NOW - i * 3_600_000,
      isFavorite: i % 11 === 0,
      creator: null,
      description: null,
      canonicalUrl: kind === "local_video" ? null : `https://example.com/v/${i + 1}`,
      notes: "",
      fileName: localState === "bookmark_only" ? null : `sample-${i + 1}.mp4`,
      archived: false,
      removed: false,
      removedAt: null,
      collectionIds: [],
      revision: 1,
    };
  });
}

/** Case, diacritics and Turkish dotless ı folded, like the FTS index (F1.6). */
export function fold(s: string): string {
  return s
    .toLocaleLowerCase("tr")
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .replace(/ı/g, "i");
}

function terms(text: string): string[] {
  return fold(text)
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean);
}

/** Highlights where any term starts a word in `value`; null if none does. */
function highlight(field: Snippet["field"], value: string, ts: string[]): Snippet | null {
  const folded = fold(value);
  // NFD folding can change length; only highlight when it did not.
  if (folded.length !== value.length) {
    return ts.some((t) => new RegExp(`(^|[^\\p{L}\\p{N}])${escape(t)}`, "u").test(folded)) ? { field, parts: [{ text: value, highlight: false }], cutStart: false, cutEnd: false } : null;
  }
  const marks = new Array<boolean>(value.length).fill(false);
  let any = false;
  for (const t of ts) {
    const re = new RegExp(`(^|[^\\p{L}\\p{N}])(${escape(t)})`, "gu");
    for (const m of folded.matchAll(re)) {
      const start = m.index! + m[1]!.length;
      for (let i = start; i < start + t.length; i++) marks[i] = true;
      any = true;
    }
  }
  if (!any) return null;
  const parts: Snippet["parts"] = [];
  for (let i = 0; i < value.length; i++) {
    const last = parts[parts.length - 1];
    if (last && last.highlight === marks[i]) last.text += value[i];
    else parts.push({ text: value[i]!, highlight: marks[i]! });
  }
  return { field, parts, cutStart: false, cutEnd: false };
}

function escape(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function matchRecord(r: FixtureRecord, ts: string[]): Snippet | null | undefined {
  const fields: Array<[Snippet["field"], string]> = [
    ["title", r.title],
    ["notes", r.notes],
    ["tags", r.tags.join(" ")],
    ["description", r.description ?? ""],
    ["creator", r.creator ?? ""],
  ];
  const words = new Set(fields.flatMap(([, v]) => terms(v)));
  const all = ts.every((t, i) => [...words].some((w) => (i === ts.length - 1 ? w.startsWith(t) : w === t || w.startsWith(t))));
  if (!all) return undefined;
  for (const [f, v] of fields) {
    const s = highlight(f, v, ts);
    if (s) return s;
  }
  return null;
}

export function queryRecords(records: FixtureRecord[], q: ListQuery, now = FIXTURE_NOW, org?: OrganizeState): ItemPage {
  const smart = q.smartCollectionId ? org?.smart.find((x) => x.id === q.smartCollectionId)?.rules : undefined;
  if (q.smartCollectionId && !smart) throw new IpcError("INTERNAL_ERROR", "list_items: error.internal");
  const text = smart?.text.trim() ? `${smart.text} ${q.text}` : q.text;
  const ts = terms(text);
  const notice = text.trim() !== "" && ts.length === 0 ? "no_searchable_terms" : null;
  const tagNames = (q.tagIds ?? []).map((id) => org?.tags.get(id)).filter((n): n is string => !!n);
  let rows = records
    .filter((r) => (q.view === "removed" ? r.removed : !r.removed))
    .filter((r) => q.view === "removed" || (smart ? true : q.view === "archive" ? r.archived : !r.archived))
    .filter((r) => !smart || !org || matchesRules(org, r, smart, now))
    .filter((r) => q.view !== "inbox" || r.collectionIds.length === 0)
    .filter((r) => !q.tagIds?.length || r.tags.some((t) => tagNames.some((n) => tagKey(n) === tagKey(t))))
    .filter((r) => !q.collectionId || r.collectionIds.includes(q.collectionId))
    .filter((r) => q.view !== "favorites" || r.isFavorite)
    .filter((r) => q.view !== "recent" || r.addedAt >= now - RECENT_DAYS * DAY)
    .filter((r) => q.platforms.length === 0 || q.platforms.includes(r.platform))
    .filter((r) => q.kinds.length === 0 || q.kinds.includes(r.kind))
    .map((r) => ({ r, snippet: ts.length ? matchRecord(r, ts) : null }))
    .filter((x) => x.snippet !== undefined);
  const dir = q.descending ? -1 : 1;
  const nullsLast = (a: number | null, b: number | null) => (a === null ? (b === null ? 0 : 1) : b === null ? -1 : dir * (a - b));
  rows = rows.sort((a, b) => {
    switch (q.sort) {
      case "title":
        return dir * a.r.title.localeCompare(b.r.title, "en") || a.r.id.localeCompare(b.r.id);
      case "duration":
        return nullsLast(a.r.durationMs, b.r.durationMs) || a.r.id.localeCompare(b.r.id);
      case "relevance":
        if (ts.length) {
          const rank = (s: Snippet | null | undefined) => (s?.field === "title" ? 0 : 1);
          return rank(a.snippet) - rank(b.snippet) || b.r.addedAt - a.r.addedAt;
        }
        return b.r.addedAt - a.r.addedAt;
      default:
        return dir * (a.r.addedAt - b.r.addedAt) || a.r.id.localeCompare(b.r.id);
    }
  });
  const page = rows.slice(q.offset, q.offset + q.limit);
  return {
    total: rows.length,
    offset: q.offset,
    notice,
    items: page.map(({ r, snippet }) => toDto(r, snippet ?? null)),
  };
}

function toDto(r: FixtureRecord, snippet: Snippet | null): LibraryItemDto {
  return {
    id: r.id,
    title: r.title,
    platform: r.platform,
    kind: r.kind,
    localState: r.localState,
    durationMs: r.durationMs,
    tags: [...r.tags],
    thumbnailUrl: r.thumbnailUrl,
    addedAt: r.addedAt,
    isFavorite: r.isFavorite,
    archived: r.archived,
    removedAt: r.removed ? r.removedAt : null,
    snippet,
  };
}

function detail(r: FixtureRecord, org: OrganizeState): ItemDetail {
  const assets: AssetDto[] = r.fileName
    ? [
        {
          id: fixtureAssetId(r.id),
          fileName: r.fileName,
          rootLabel: r.localState === "disconnected" ? "Backup drive" : "Videos",
          rootStatus: r.localState === "disconnected" ? "disconnected" : "available",
          integrityState: r.localState === "missing" ? "missing" : "verified",
          sizeBytes: r.durationMs ? Math.round(r.durationMs * 250) : null,
          container: r.fileName.split(".").pop() ?? null,
          videoCodec: fixtureMedia(r.fileName)?.codec ?? "h264",
          width: 1920,
          height: 1080,
          durationMs: r.durationMs,
        },
      ]
    : [];
  return {
    item: toDto(r, null),
    canonicalUrl: r.canonicalUrl,
    creator: r.creator,
    description: r.description,
    notes: r.notes,
    publishedAt: null,
    publishedDate: null,
    metadataStatus: r.kind === "local_video" ? "ready" : "pending",
    updatedAt: r.addedAt,
    archived: r.archived,
    assets,
    tagRefs: tagRefs(org, r),
    collectionIds: [...r.collectionIds].sort(),
    revision: r.revision,
  };
}

const delay = (ms: number) => new Promise<void>((done) => setTimeout(done, ms));

export interface FixtureLibrary extends LibraryClient {
  /** Test hook: plays an import status as if the backend had sent it. */
  emitImportStatus(s: ImportStatus): void;
}

export function createFixtureLibrary(records: FixtureRecord[], scenario: FixtureScenario = "default"): FixtureLibrary {
  const listeners = new Set<(s: ImportStatus) => void>();
  const latency = scenario === "slow" ? 60_000 : 0;
  let saved = 0;
  const parse = (cmd: string, url: string) => {
    try {
      return fixtureParseUrl(url);
    } catch (code) {
      throw new IpcError(String(code), `${cmd}: error.${String(code).toLowerCase()}`);
    }
  };
  const existing = (canonicalUrl: string): UrlCheck["existing"] => {
    const r = records.find((x) => x.canonicalUrl === canonicalUrl);
    return r ? { item: toDto(r, null), removed: r.removed, matchedBy: "canonical_url" } : null;
  };
  const fail = () => Promise.reject(new IpcError("INTERNAL_ERROR", "list_items: error.internal"));
  const org = createOrganizeState(records, scenario === "default" || scenario === "first-run");
  let clock = FIXTURE_NOW + 3_600_000;
  const organize = createFixtureOrganize(records, org, (r) => toDto(r, null), () => (clock += 1000));
  return {
    ...organize,
    async listItems(q) {
      if (scenario === "error") return fail();
      if (latency) await delay(latency);
      return structuredClone(queryRecords(records, q, FIXTURE_NOW, org));
    },
    async getItem(id) {
      const r = records.find((x) => x.id === id && !x.removed);
      if (!r) throw new IpcError("INTERNAL_ERROR", "get_item: error.internal");
      return structuredClone(detail(r, org));
    },
    async setNotes(id, revision, notes) {
      const r = records.find((x) => x.id === id && !x.removed);
      if (!r) throw new IpcError("INTERNAL_ERROR", "set_notes: error.internal");
      if (r.revision !== revision) throw new IpcError("REVISION_CONFLICT", "set_notes: error.revision_conflict");
      r.notes = notes;
      r.revision += 1;
      return structuredClone(detail(r, org));
    },
    async importMedia() {
      await delay(300);
      return structuredClone(contract.commands.import_media.response) as ImportSummary;
    },
    async checkUrl(url) {
      const p = parse("check_url", url);
      return structuredClone({ ...p, existing: existing(p.canonicalUrl) });
    },
    async saveUrl(url) {
      const p = parse("save_url", url);
      const ex = existing(p.canonicalUrl);
      if (ex) return structuredClone({ outcome: "existing", item: ex.item, removed: ex.removed, matchedBy: ex.matchedBy, warnings: p.warnings } satisfies SaveUrlResult);
      saved += 1;
      const r: FixtureRecord = {
        id: `00000000-0000-4000-a000-${String(saved).padStart(12, "0")}`,
        title: p.title,
        platform: p.platform,
        kind: "bookmark",
        localState: "bookmark_only",
        durationMs: null,
        tags: [],
        thumbnailUrl: null,
        // Newer than every fixture item, so it lands first in "Newest first".
        addedAt: FIXTURE_NOW + saved * 1000,
        isFavorite: false,
        creator: null,
        description: null,
        canonicalUrl: p.canonicalUrl,
        notes: "",
        fileName: null,
        archived: false,
        removed: false,
        removedAt: null,
        collectionIds: [],
        revision: 1,
      };
      records.push(r);
      return structuredClone({ outcome: "created", item: toDto(r, null), removed: false, matchedBy: null, warnings: p.warnings } satisfies SaveUrlResult);
    },
    async restoreItem(id) {
      const r = records.find((x) => x.id === id);
      if (!r) throw new IpcError("INTERNAL_ERROR", "restore_item: error.internal");
      r.removed = false;
      r.removedAt = null;
      return structuredClone(toDto(r, null));
    },
    async onImportStatus(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    emitImportStatus(s) {
      for (const l of listeners) l(s);
    },
  };
}

export function scenarioRecords(s: FixtureScenario): FixtureRecord[] {
  if (s === "empty") return [];
  if (s === "many") return manyRecords(2000);
  return fixtureRecords();
}
