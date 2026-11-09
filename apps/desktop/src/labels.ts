// User-facing names for backend values, resolved through translation keys
// (SPEC §9.6, `src/i18n`). The backend sends codes; the text lives here.

import { t, tn, type MessageKey } from "./i18n";
import { en } from "./i18n/en";
import type { ItemKind, LibraryView, LocalState, OpResult, Platform } from "./ipc/types";

const PLATFORMS: Platform[] = ["youtube", "facebook", "instagram", "tiktok", "x", "vk", "dailymotion", "twitch", "reddit", "generic", "local"];
const KINDS: ItemKind[] = ["bookmark", "remote_video", "local_video"];
const STATES: LocalState[] = ["bookmark_only", "local_ready", "disconnected", "missing"];
const VIEWS: LibraryView[] = ["all", "inbox", "removed", "favorites", "recent", "archive"];

/** A record whose values are read through `t()` at access time, so a locale change shows up without a reload. */
function keyed<K extends string, V>(keys: readonly K[], value: (k: K) => V): Record<K, V> {
  const out = {} as Record<K, V>;
  for (const k of keys) Object.defineProperty(out, k, { enumerable: true, get: () => value(k) });
  return out;
}

export const PLATFORM_LABEL: Record<Platform, string> = keyed(PLATFORMS, (p) => t(`platform.${p}` as MessageKey));

export const KIND_LABEL: Record<ItemKind, string> = keyed(KINDS, (k) => t(`kind.${k}` as MessageKey));

const STATE_ICON: Record<LocalState, string> = { bookmark_only: "🔖", local_ready: "✓", disconnected: "⏏", missing: "!" };

/** Text and a symbol for each state, so it never depends on colour alone (§9.4). */
export const LOCAL_STATE: Record<LocalState, { label: string; icon: string; hint: string }> = keyed(STATES, (s) => ({
  label: t(`state.${s}.label` as MessageKey),
  icon: STATE_ICON[s],
  hint: t(`state.${s}.hint` as MessageKey),
}));

export const VIEW_LABEL: Record<LibraryView, string> = keyed(VIEWS, (v) => t(`view.${v}` as MessageKey));

const IMPORT_REASONS = new Set(["INPUT_REJECTED", "MEDIA_CORRUPT", "TOOL_MISSING", "TIMEOUT", "FILE_MISSING"]);

export function importReason(code: string | null): string {
  return code && IMPORT_REASONS.has(code) ? t(`import.reason.${code}` as MessageKey) : t("import.reason.other");
}

export function importMessage(s: { imported: number; alreadyImported: number; duplicates: number; rejected: number; skipped: number; canceled: boolean; firstError: string | null }): string {
  const parts: string[] = [];
  if (s.imported) parts.push(tn("import.done", s.imported));
  if (s.duplicates) parts.push(tn("import.duplicates", s.duplicates));
  if (s.alreadyImported) parts.push(tn("import.already", s.alreadyImported));
  if (s.rejected) parts.push(tn("import.rejected", s.rejected, { reason: importReason(s.firstError) }));
  if (s.skipped) parts.push(tn("import.skipped", s.skipped));
  if (s.canceled) parts.push(t("import.canceled"));
  return parts.join(" ") || t("import.nothing");
}

export function importErrorMessage(code: string): string {
  if (code === "TOOL_MISSING") return t("import.error.tool_missing");
  return t("import.error.other", { code });
}

const URL_ERRORS = new Set(["INVALID_URL", "URL_BLOCKED", "URL_HAS_CREDENTIALS"]);

/** Field error for an Add URL refusal (§8.1: a fixable field error). */
export function urlErrorMessage(code: string): string {
  return URL_ERRORS.has(code) ? t(`url.error.${code}` as MessageKey) : t("url.error.other", { code });
}

export function urlWarningMessage(code: string): string {
  if (code === "PLAYLIST_UNSUPPORTED") return t("url.warning.PLAYLIST_UNSUPPORTED");
  return code;
}

/**
 * Result of a bulk or single operation, with honest counts (ORG-05):
 * `done` says what happened to the changed items; `{n}` is "3 items".
 */
export function opMessage(done: string, r: OpResult, unchangedReason = t("op.unchanged.default")): string {
  const parts = [r.changed ? done.replace("{n}", tn("op.items", r.changed)) : t("op.nothing")];
  if (r.unchanged) parts.push(tn("op.unchanged", r.unchanged, { reason: r.unchanged === 1 ? unchangedReason.replace(/^were/, "was") : unchangedReason }));
  if (r.failed) parts.push(tn("op.failed", r.failed));
  return parts.join(" ");
}

const UNDO_KINDS = new Set(["tag_items", "untag_items", "add_to_collection", "remove_from_collection", "set_favorite", "set_archived", "remove", "rename_tag", "merge_tags", "merge_items"]);

export function undoMessage(r: { kind: string; applied: number; skipped: number }): string {
  const head = UNDO_KINDS.has(r.kind) ? t(`undo.${r.kind}` as MessageKey) : t("undo.other");
  return r.skipped ? `${head} ${tn("undo.skipped", r.skipped)}` : head;
}

const ORG_ERROR: Record<string, MessageKey> = {
  "tag-name-taken": "org.error.tag_name_taken",
  "collection-cycle": "org.error.collection_cycle",
  "collection tree too deep": "org.error.collection_depth",
  "root-disconnected": "org.error.root_disconnected",
  "item is not in Recently removed": "org.error.not_removed",
};

/** Message for a refused organize step; `diagnosticId` carries the reason. */
export function orgErrorMessage(e: unknown): string {
  const d = (e as { domain?: { diagnosticId?: string } | null })?.domain?.diagnosticId;
  const code = (e as { code?: string })?.code;
  if (code === "REVISION_CONFLICT") return t("org.error.revision_conflict");
  const key = d ? ORG_ERROR[d] : undefined;
  return key ? t(key) : t("org.error.other", { code: code ?? "IPC_FAILED" });
}

/** Codes with their own sentence: every SPEC §19 code (see labels.test.ts). */
export const ERROR_KEYS: ReadonlySet<string> = new Set(
  Object.keys(en)
    .filter((k) => /^error\.[A-Z_]+$/.test(k))
    .map((k) => k.slice("error.".length)),
);

/** A short sentence for a §19 error code. */
export function errorMessage(code: string): string {
  return ERROR_KEYS.has(code) ? t(`error.${code}` as MessageKey) : t("error.other", { code });
}
