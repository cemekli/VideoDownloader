// In-memory organize backend for the fixture client (STATE.md F2.1–F2.3).
// It behaves like the Rust commands closely enough to drive every screen
// state. Undo here restores a snapshot of what the operation could touch;
// the real per-field rules (skip what changed since, keep background
// updates) are tested in Rust (vv-persistence tests/organize.rs).

import { IpcError } from "./errors";
import { cleanName, tagKeyOf as tagKey } from "../tags";
import type { ChildPolicy, CollectionDto, DuplicateDto, LibraryItemDto, MergePreview, OpResult, OrganizeClient, OrphanDto, SmartRules, TagRef } from "./types";
import type { FixtureRecord } from "./fixture";

export interface FixtureCollection {
  id: string;
  name: string;
  parentId: string | null;
}

export interface FixtureSmart {
  id: string;
  name: string;
  rules: SmartRules;
}

interface Candidate {
  id: string;
  reason: string;
  createdAt: number;
  a: string;
  b: string;
  status: "open" | "merged" | "dismissed";
}

interface Snapshot {
  records: Array<Pick<FixtureRecord, "id" | "tags" | "collectionIds" | "isFavorite" | "archived" | "removed" | "removedAt" | "notes">>;
  tags: Array<[string, string]>;
  candidates: Candidate[];
}

export interface OrganizeState {
  /** Tag id → name. Records carry names; ids are looked up here. */
  tags: Map<string, string>;
  collections: FixtureCollection[];
  smart: FixtureSmart[];
  candidates: Candidate[];
  orphans: OrphanDto[];
  undo: { kind: string; size: number; snapshot: Snapshot } | null;
  seq: number;
}

export { tagKeyOf as tagKey } from "../tags";

export function createOrganizeState(records: FixtureRecord[], seedDuplicates: boolean): OrganizeState {
  const tags = new Map<string, string>();
  for (const r of records) for (const t of r.tags) if (![...tags.values()].some((n) => tagKey(n) === tagKey(t))) tags.set(`00000000-0000-4000-b000-${String(tags.size + 1).padStart(12, "0")}`, t);
  const s: OrganizeState = {
    tags,
    collections: [],
    smart: [],
    candidates: [],
    orphans: [],
    undo: null,
    seq: 0,
  };
  const a = records.find((r) => r.id.endsWith("0001"));
  const b = records.find((r) => r.id.endsWith("0006"));
  if (seedDuplicates && a && b)
    s.candidates.push({
      id: "00000000-0000-4000-d000-000000000001",
      reason: "similar_url",
      createdAt: a.addedAt,
      a: a.id,
      b: b.id,
      status: "open",
    });
  return s;
}

export function tagIdOf(s: OrganizeState, name: string): string | undefined {
  return [...s.tags].find(([, n]) => tagKey(n) === tagKey(name))?.[0];
}

export function tagRefs(s: OrganizeState, r: FixtureRecord): TagRef[] {
  return r.tags.map((name) => ({ id: tagIdOf(s, name) ?? name, name }));
}

/** Rules → does the record match (the fixture's `SmartRules::filters`). */
export function matchesRules(s: OrganizeState, r: FixtureRecord, rules: SmartRules, now: number): boolean {
  const names = rules.tagIds.map((id) => s.tags.get(id)).filter((n): n is string => !!n);
  return (
    (rules.platforms.length === 0 || rules.platforms.includes(r.platform)) &&
    (rules.kinds.length === 0 || rules.kinds.includes(r.kind)) &&
    (rules.tagIds.length === 0 || r.tags.some((t) => names.some((n) => tagKey(n) === tagKey(t)))) &&
    (rules.collectionIds.length === 0 || r.collectionIds.some((c) => rules.collectionIds.includes(c))) &&
    (!rules.favoriteOnly || r.isFavorite) &&
    (rules.includeArchived || !r.archived) &&
    (rules.addedWithinDays === null || r.addedAt >= now - rules.addedWithinDays * 86_400_000) &&
    (rules.minDurationMs === null || (r.durationMs !== null && r.durationMs >= rules.minDurationMs)) &&
    (rules.maxDurationMs === null || (r.durationMs !== null && r.durationMs <= rules.maxDurationMs))
  );
}

const fail = (cmd: string, id = "internal") =>
  new IpcError("INTERNAL_ERROR", `${cmd}: error.internal`, {
    code: "INTERNAL_ERROR",
    messageKey: "error.internal",
    retryable: false,
    suggestedAction: null,
    diagnosticId: id,
    safeDetails: {},
  });

export function createFixtureOrganize(records: FixtureRecord[], s: OrganizeState, toDto: (r: FixtureRecord) => LibraryItemDto, now: () => number): OrganizeClient {
  const nextId = (block: string) => `00000000-0000-4000-${block}-${String(++s.seq).padStart(12, "0")}`;
  const byId = (id: string) => records.find((r) => r.id === id);
  const live = (ids: string[]) => [...new Set(ids)].map(byId).filter((r): r is FixtureRecord => !!r && !r.removed);
  const snapshot = (): Snapshot => ({
    records: records.map((r) => ({
      id: r.id,
      tags: [...r.tags],
      collectionIds: [...r.collectionIds],
      isFavorite: r.isFavorite,
      archived: r.archived,
      removed: r.removed,
      removedAt: r.removedAt,
      notes: r.notes,
    })),
    tags: [...s.tags],
    candidates: s.candidates.map((c) => ({ ...c })),
  });
  /** Runs one operation; logs it for Undo when it changed something. */
  const op = (kind: string, ids: string[], change: (r: FixtureRecord) => boolean, undoable = true): OpResult => {
    const snap = snapshot();
    const rs = live(ids);
    const changed = rs.filter(change).length;
    const undo = undoable && changed > 0 ? { kind, size: changed } : null;
    if (undo) s.undo = { ...undo, snapshot: snap };
    return {
      changed,
      unchanged: rs.length - changed,
      failed: new Set(ids).size - rs.length,
      undo,
    };
  };
  const single = (kind: string, run: () => void, size = 1): OpResult => {
    const snap = snapshot();
    run();
    s.undo = { kind, size, snapshot: snap };
    return { changed: size, unchanged: 0, failed: 0, undo: { kind, size } };
  };
  const tagName = (id: string, cmd: string) => {
    const n = s.tags.get(id);
    if (n === undefined) throw fail(cmd, `${cmd}-not-found`);
    return n;
  };
  const checkName = (name: string, max: number, cmd: string) => {
    const n = cleanName(name);
    if (!n || [...n].length > max) throw fail(cmd, "name-invalid");
    return n;
  };
  const subtree = (id: string): string[] => [id, ...s.collections.filter((c) => c.parentId === id).flatMap((c) => subtree(c.id))];
  const depthOf = (id: string | null): number => (id === null ? 0 : 1 + depthOf(s.collections.find((c) => c.id === id)?.parentId ?? null));

  return {
    async getSidebar() {
      const ordered: CollectionDto[] = [];
      const walk = (parent: string | null, depth: number) => {
        for (const c of s.collections.filter((x) => x.parentId === parent).sort((a, b) => a.name.localeCompare(b.name, "tr"))) {
          ordered.push({
            id: c.id,
            name: c.name,
            parentId: c.parentId,
            depth,
            items: records.filter((r) => !r.removed && r.collectionIds.includes(c.id)).length,
          });
          walk(c.id, depth + 1);
        }
      };
      walk(null, 0);
      return structuredClone({
        collections: ordered,
        smartCollections: s.smart.map((x) => ({
          ...x,
          invalid: x.rules.tagIds.some((t) => !s.tags.has(t)) || x.rules.collectionIds.some((c) => !s.collections.some((y) => y.id === c)),
        })),
        tags: [...s.tags]
          .map(([id, name]) => ({
            id,
            name,
            items: records.filter((r) => !r.removed && r.tags.some((t) => tagKey(t) === tagKey(name))).length,
          }))
          .sort((a, b) => a.name.localeCompare(b.name, "tr")),
        counts: {
          inbox: records.filter((r) => !r.removed && !r.archived && r.collectionIds.length === 0).length,
          removed: records.filter((r) => r.removed).length,
          duplicates: s.candidates.filter((c) => c.status === "open" && !byId(c.a)?.removed && !byId(c.b)?.removed).length,
          orphans: s.orphans.length,
        },
      });
    },
    async undo() {
      const u = s.undo;
      if (!u) return null;
      s.undo = null;
      for (const saved of u.snapshot.records) {
        const r = byId(saved.id);
        if (r) Object.assign(r, structuredClone(saved));
      }
      s.tags = new Map(u.snapshot.tags);
      s.candidates = u.snapshot.candidates;
      return { kind: u.kind, applied: u.size, skipped: 0 };
    },
    async tagItems(ids, name) {
      const n = checkName(name, 100, "tag_items");
      let id = tagIdOf(s, n);
      const snapTags = [...s.tags];
      if (!id) {
        id = nextId("b000");
        s.tags.set(id, n);
      }
      const canonical = s.tags.get(id)!;
      const r = op("tag_items", ids, (x) => {
        if (x.tags.some((t) => tagKey(t) === tagKey(canonical))) return false;
        x.tags.push(canonical);
        return true;
      });
      if (r.undo) s.undo!.snapshot.tags = snapTags;
      return r;
    },
    async untagItems(ids, tagId) {
      const name = tagName(tagId, "untag_items");
      return op("untag_items", ids, (x) => {
        const before = x.tags.length;
        x.tags = x.tags.filter((t) => tagKey(t) !== tagKey(name));
        return x.tags.length !== before;
      });
    },
    async addToCollection(ids, collectionId) {
      if (!s.collections.some((c) => c.id === collectionId)) throw fail("add_to_collection");
      return op("add_to_collection", ids, (x) => !x.collectionIds.includes(collectionId) && !!x.collectionIds.push(collectionId));
    },
    async removeFromCollection(ids, collectionId) {
      return op("remove_from_collection", ids, (x) => {
        const before = x.collectionIds.length;
        x.collectionIds = x.collectionIds.filter((c) => c !== collectionId);
        return x.collectionIds.length !== before;
      });
    },
    async setFavorite(ids, value) {
      return op("set_favorite", ids, (x) => x.isFavorite !== value && ((x.isFavorite = value), true));
    },
    async setArchived(ids, value) {
      return op("set_archived", ids, (x) => x.archived !== value && ((x.archived = value), true));
    },
    async removeItems(ids) {
      const at = now();
      return op("remove", ids, (x) => ((x.removed = true), (x.removedAt = at), true));
    },
    async restoreItems(ids) {
      const rs = [...new Set(ids)].map(byId).filter((r): r is FixtureRecord => !!r && r.removed);
      for (const r of rs) {
        r.removed = false;
        r.removedAt = null;
      }
      return {
        changed: rs.length,
        unchanged: 0,
        failed: new Set(ids).size - rs.length,
        undo: null,
      };
    },
    async deleteItemsPermanently(ids, deleteMedia) {
      const rs = [...new Set(ids)].map(byId).filter((r): r is FixtureRecord => !!r);
      if (rs.some((r) => !r.removed)) throw fail("delete_items_permanently", "item is not in Recently removed");
      let orphaned = 0;
      let deleted = 0;
      for (const r of rs) {
        // Downloaded (managed) media; imported files are only referenced.
        const managed = r.fileName && r.kind !== "local_video";
        if (managed && deleteMedia) deleted++;
        else if (managed) {
          orphaned++;
          s.orphans.push({
            id: nextId("e000"),
            fileName: r.fileName!,
            rootLabel: "VideoVault",
            rootStatus: "available",
            sizeBytes: r.durationMs ? Math.round(r.durationMs * 250) : null,
            formerTitle: r.title,
            orphanedAt: now(),
          });
        }
        records.splice(records.indexOf(r), 1);
      }
      s.undo = null;
      return { items: rs.length, orphaned, files: { deleted, failed: 0 } };
    },
    async renameTag(id, name) {
      const old = tagName(id, "rename_tag");
      const n = checkName(name, 100, "rename_tag");
      const other = tagIdOf(s, n);
      if (other && other !== id) throw fail("rename_tag", "tag-name-taken");
      if (old === n) return { changed: 0, unchanged: 1, failed: 0, undo: null };
      return single("rename_tag", () => {
        s.tags.set(id, n);
        for (const r of records) r.tags = r.tags.map((t) => (tagKey(t) === tagKey(old) ? n : t));
      });
    },
    async mergeTags(sourceId, targetId) {
      const src = tagName(sourceId, "merge_tags");
      const dst = tagName(targetId, "merge_tags");
      if (sourceId === targetId) throw fail("merge_tags", "merge-into-itself");
      return single("merge_tags", () => {
        s.tags.delete(sourceId);
        for (const r of records) {
          if (!r.tags.some((t) => tagKey(t) === tagKey(src))) continue;
          r.tags = r.tags.filter((t) => tagKey(t) !== tagKey(src));
          if (!r.tags.some((t) => tagKey(t) === tagKey(dst))) r.tags.push(dst);
        }
      });
    },
    async deleteTag(id) {
      const name = tagName(id, "delete_tag");
      let n = 0;
      for (const r of records) {
        const before = r.tags.length;
        r.tags = r.tags.filter((t) => tagKey(t) !== tagKey(name));
        n += before - r.tags.length;
      }
      s.tags.delete(id);
      return { changed: n, unchanged: 0, failed: 0, undo: null };
    },
    async createCollection(name, parentId) {
      const n = checkName(name, 200, "create_collection");
      if (parentId && !s.collections.some((c) => c.id === parentId)) throw fail("create_collection");
      if (depthOf(parentId) >= 8) throw fail("create_collection", "collection tree too deep");
      const c = { id: nextId("c000"), name: n, parentId };
      s.collections.push(c);
      return { id: c.id, name: c.name };
    },
    async renameCollection(id, name) {
      const c = s.collections.find((x) => x.id === id);
      if (!c) throw fail("rename_collection");
      c.name = checkName(name, 200, "rename_collection");
    },
    async moveCollection(id, parentId) {
      const c = s.collections.find((x) => x.id === id);
      if (!c) throw fail("move_collection");
      if (parentId && subtree(id).includes(parentId)) throw fail("move_collection", "collection-cycle");
      c.parentId = parentId;
    },
    async deleteCollection(id, policy: ChildPolicy) {
      const c = s.collections.find((x) => x.id === id);
      if (!c) throw fail("delete_collection");
      let doomed = [id];
      if (policy === "move_up") {
        for (const x of s.collections) if (x.parentId === id) x.parentId = c.parentId;
      } else doomed = subtree(id);
      s.collections = s.collections.filter((x) => !doomed.includes(x.id));
      for (const r of records) r.collectionIds = r.collectionIds.filter((x) => !doomed.includes(x));
      return doomed.length;
    },
    async saveSmartCollection(id, name, rules) {
      const n = checkName(name, 200, "save_smart_collection");
      const empty =
        !rules.text.trim() &&
        !rules.platforms.length &&
        !rules.kinds.length &&
        !rules.tagIds.length &&
        !rules.collectionIds.length &&
        !rules.favoriteOnly &&
        rules.addedWithinDays === null &&
        rules.minDurationMs === null &&
        rules.maxDurationMs === null;
      if (empty) throw fail("save_smart_collection", "no condition");
      if (id) {
        const x = s.smart.find((y) => y.id === id);
        if (!x) throw fail("save_smart_collection");
        Object.assign(x, { name: n, rules: structuredClone(rules) });
        return { id, name: n };
      }
      const x = { id: nextId("f000"), name: n, rules: structuredClone(rules) };
      s.smart.push(x);
      return { id: x.id, name: n };
    },
    async deleteSmartCollection(id) {
      s.smart = s.smart.filter((x) => x.id !== id);
    },
    async setNotes() {
      // Replaced by the library, which owns item details.
      throw fail("set_notes");
    },
    async listDuplicates() {
      return structuredClone(
        s.candidates
          .filter((c) => c.status === "open")
          .map((c) => ({ c, a: byId(c.a), b: byId(c.b) }))
          .filter((x) => x.a && x.b && !x.a.removed && !x.b.removed)
          .map(
            ({ c, a, b }): DuplicateDto => ({
              id: c.id,
              reason: c.reason,
              createdAt: c.createdAt,
              a: toDto(a!),
              b: toDto(b!),
            }),
          ),
      );
    },
    async previewMerge(candidateId, targetId) {
      const { t, src } = pair(candidateId, targetId);
      const p: MergePreview = {
        target: toDto(t),
        source: toDto(src),
        notesAfter: [t.notes, src.notes].filter((n) => n.trim()).join("\n\n"),
        tagsToAdd: src.tags.filter((x) => !t.tags.some((y) => tagKey(x) === tagKey(y))).map((name) => ({ id: tagIdOf(s, name) ?? name, name })),
        collectionsToAdd: src.collectionIds
          .filter((x) => !t.collectionIds.includes(x))
          .map((id) => ({
            id,
            name: s.collections.find((c) => c.id === id)?.name ?? id,
          })),
        mediaToMove: src.fileName ? [src.fileName] : [],
        aliasUrl: src.canonicalUrl && src.canonicalUrl !== t.canonicalUrl ? src.canonicalUrl : null,
        becomesFavorite: src.isFavorite && !t.isFavorite,
      };
      return structuredClone(p);
    },
    async mergeItems(candidateId, targetId) {
      const { c, t, src } = pair(candidateId, targetId);
      const r = single(
        "merge_items",
        () => {
          t.notes = [t.notes, src.notes].filter((n) => n.trim()).join("\n\n");
          for (const x of src.tags) if (!t.tags.some((y) => tagKey(x) === tagKey(y))) t.tags.push(x);
          for (const x of src.collectionIds) if (!t.collectionIds.includes(x)) t.collectionIds.push(x);
          t.isFavorite ||= src.isFavorite;
          src.removed = true;
          src.removedAt = now();
          c.status = "merged";
        },
        1,
      );
      return { ...r, changed: 2 };
    },
    async dismissDuplicate(candidateId) {
      const c = s.candidates.find((x) => x.id === candidateId && x.status === "open");
      if (!c) throw fail("dismiss_duplicate");
      c.status = "dismissed";
    },
    async listOrphans() {
      return structuredClone(s.orphans);
    },
    async reimportOrphan(id) {
      const o = s.orphans.find((x) => x.id === id);
      if (!o) throw fail("reimport_orphan");
      s.orphans = s.orphans.filter((x) => x.id !== id);
      const r: FixtureRecord = {
        id: nextId("a100"),
        title: o.formerTitle ?? o.fileName,
        platform: "local",
        kind: "local_video",
        localState: "local_ready",
        durationMs: null,
        tags: [],
        thumbnailUrl: null,
        addedAt: now(),
        isFavorite: false,
        creator: null,
        description: null,
        canonicalUrl: null,
        notes: "",
        fileName: o.fileName,
        archived: false,
        removed: false,
        removedAt: null,
        collectionIds: [],
        revision: 1,
      };
      records.push(r);
      return structuredClone(toDto(r));
    },
    async deleteOrphan(id) {
      if (!s.orphans.some((x) => x.id === id)) throw fail("delete_orphan");
      s.orphans = s.orphans.filter((x) => x.id !== id);
      return { deleted: 1, failed: 0 };
    },
  };

  function pair(candidateId: string, targetId: string) {
    const c = s.candidates.find((x) => x.id === candidateId && x.status === "open");
    if (!c || (c.a !== targetId && c.b !== targetId)) throw fail("merge_items", "candidate is not open");
    const t = byId(targetId);
    const src = byId(c.a === targetId ? c.b : c.a);
    if (!t || !src || t.removed || src.removed) throw fail("merge_items", "an item of the candidate was removed");
    return { c, t, src };
  }
}
