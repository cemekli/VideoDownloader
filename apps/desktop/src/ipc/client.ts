import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  ApplyImport,
  AppStatus,
  BackupDto,
  BackupPreview,
  DataClient,
  ExportResult,
  LibraryImportReport,
  PendingImport,
  PlaybackInfo,
  PlaybackState,
  RestoreResult,
  Settings,
  StorageOverview,
  BackendClient,
  DeleteResult,
  Diagnostics,
  DomainError,
  DuplicateDto,
  ImportStatus,
  ImportSummary,
  ItemDetail,
  ItemPage,
  LibraryItemDto,
  MergePreview,
  NamedRef,
  OpResult,
  OrganizeClient,
  OrphanDto,
  Pong,
  SaveUrlResult,
  Sidebar,
  UndoResult,
  UrlCheck,
} from "./types";
import contract from "@fixtures/ipc-contract.json";
import { IpcError } from "./errors";
import { createFixtureLibrary, scenarioRecords, type FixtureLibrary, type FixtureRecord, type FixtureScenario } from "./fixture";
import { createFixtureData } from "./fixtureData";

export { IpcError } from "./errors";

/** Must match `CONTRACT_VERSION` in `src-tauri/src/commands.rs`. */
export const CONTRACT_VERSION = 1;

/** Event the backend emits for imports started by a drop on the window (`library.rs`). */
export const IMPORT_EVENT = "import-status";

function isDomainError(v: unknown): v is DomainError {
  return typeof v === "object" && v !== null && typeof (v as DomainError).code === "string" && typeof (v as DomainError).messageKey === "string";
}

/**
 * Tauri rejects with a DomainError from our commands, or with a plain string
 * for its own failures (unknown command, ACL denial, bad arguments). The raw
 * string is not shown to users; it only goes into `message` for diagnostics.
 */
export function toIpcError(cmd: string, reason: unknown): IpcError {
  if (isDomainError(reason)) return new IpcError(reason.code, `${cmd}: ${reason.messageKey}`, reason);
  return new IpcError("IPC_FAILED", `${cmd}: ${typeof reason === "string" ? reason : "invoke failed"}`);
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (reason) {
    throw toIpcError(cmd, reason);
  }
}

export type FixtureClient = BackendClient & Pick<FixtureLibrary, "emitImportStatus">;

/**
 * Deterministic client backed by the shared fixture set (SPEC §25.2).
 * Used by `vite --mode fixture`, Vitest and Playwright E2E.
 */
export function createFixtureClient(records?: FixtureRecord[], scenario: FixtureScenario = "default"): FixtureClient {
  const recs = records ?? scenarioRecords(scenario);
  const library = createFixtureLibrary(recs, scenario);
  return {
    ...createFixtureData(recs, scenario, library),
    kind: "fixture",
    listItems: (q) => library.listItems(q),
    getItem: (id) => library.getItem(id),
    importMedia: (s) => library.importMedia(s),
    onImportStatus: (cb) => library.onImportStatus(cb),
    checkUrl: (u) => library.checkUrl(u),
    saveUrl: (u) => library.saveUrl(u),
    restoreItem: (id) => library.restoreItem(id),
    ...organizeOf(library),
    emitImportStatus: (s) => library.emitImportStatus(s),
    async ping(nonce) {
      return { nonce, contractVersion: CONTRACT_VERSION };
    },
    async getStatus() {
      return structuredClone(contract.commands.get_status.response) as AppStatus;
    },
    async getDiagnostics() {
      return structuredClone(contract.commands.get_diagnostics.response) as Diagnostics;
    },
  };
}

const ORGANIZE_METHODS = [
  "getSidebar", "undo", "tagItems", "untagItems", "addToCollection", "removeFromCollection", "setFavorite", "setArchived",
  "removeItems", "restoreItems", "deleteItemsPermanently", "renameTag", "mergeTags", "deleteTag", "createCollection",
  "renameCollection", "moveCollection", "deleteCollection", "saveSmartCollection", "deleteSmartCollection", "setNotes",
  "listDuplicates", "previewMerge", "mergeItems", "dismissDuplicate", "listOrphans", "reimportOrphan", "deleteOrphan",
] as const satisfies ReadonlyArray<keyof OrganizeClient>;

function organizeOf(lib: OrganizeClient): OrganizeClient {
  return Object.fromEntries(ORGANIZE_METHODS.map((m) => [m, (lib[m] as (...a: unknown[]) => unknown).bind(lib)])) as unknown as OrganizeClient;
}

/** Organize commands (F2.1–F2.3); argument names are the Rust ones in camelCase. */
const organizeCommands: OrganizeClient = {
  getSidebar: () => call<Sidebar>("get_sidebar"),
  undo: () => call<UndoResult | null>("undo"),
  tagItems: (itemIds, name) => call<OpResult>("tag_items", { itemIds, name }),
  untagItems: (itemIds, tagId) => call<OpResult>("untag_items", { itemIds, tagId }),
  addToCollection: (itemIds, collectionId) => call<OpResult>("add_to_collection", { itemIds, collectionId }),
  removeFromCollection: (itemIds, collectionId) => call<OpResult>("remove_from_collection", { itemIds, collectionId }),
  setFavorite: (itemIds, value) => call<OpResult>("set_favorite", { itemIds, value }),
  setArchived: (itemIds, value) => call<OpResult>("set_archived", { itemIds, value }),
  removeItems: (itemIds) => call<OpResult>("remove_items", { itemIds }),
  restoreItems: (itemIds) => call<OpResult>("restore_items", { itemIds }),
  deleteItemsPermanently: (itemIds, deleteMedia) => call<DeleteResult>("delete_items_permanently", { itemIds, deleteMedia }),
  renameTag: (id, name) => call<OpResult>("rename_tag", { id, name }),
  mergeTags: (sourceId, targetId) => call<OpResult>("merge_tags", { sourceId, targetId }),
  deleteTag: (id) => call<OpResult>("delete_tag", { id }),
  createCollection: (name, parentId) => call<NamedRef>("create_collection", { name, parentId }),
  renameCollection: (id, name) => call<void>("rename_collection", { id, name }),
  moveCollection: (id, parentId) => call<void>("move_collection", { id, parentId }),
  deleteCollection: (id, policy) => call<number>("delete_collection", { id, policy }),
  saveSmartCollection: (id, name, rules) => call<NamedRef>("save_smart_collection", { id, name, rules }),
  deleteSmartCollection: (id) => call<void>("delete_smart_collection", { id }),
  setNotes: (id, revision, notes) => call<ItemDetail>("set_notes", { id, revision, notes }),
  listDuplicates: () => call<DuplicateDto[]>("list_duplicates"),
  previewMerge: (candidateId, targetId) => call<MergePreview>("preview_merge", { candidateId, targetId }),
  mergeItems: (candidateId, targetId) => call<OpResult>("merge_items", { candidateId, targetId }),
  dismissDuplicate: (candidateId) => call<void>("dismiss_duplicate", { candidateId }),
  listOrphans: () => call<OrphanDto[]>("list_orphans"),
  reimportOrphan: (id) => call<LibraryItemDto>("reimport_orphan", { id }),
  deleteOrphan: (id) => call<{ deleted: number; failed: number }>("delete_orphan", { id }),
};

/** Player, settings, export/import and backups (F2.4–F2.8). */
const dataCommands: DataClient = {
  getSettings: () => call<Settings>("get_settings"),
  updateSettings: (patch) => call<Settings>("update_settings", { patch }),
  acknowledgeLegal: () => call<Settings>("acknowledge_legal"),
  finishOnboarding: () => call<Settings>("finish_onboarding"),
  getStorageOverview: () => call<StorageOverview>("get_storage_overview"),
  chooseMediaFolder: () => call<StorageOverview | null>("choose_media_folder"),
  exportLibrary: (includeAbsolutePaths) => call<ExportResult>("export_library", { includeAbsolutePaths }),
  previewLibraryImport: () => call<PendingImport | null>("preview_library_import"),
  mapImportRoot: (token, rootId) => call<string | null>("map_import_root", { token, rootId }),
  applyLibraryImport: (request: ApplyImport) => call<LibraryImportReport>("apply_library_import", { request }),
  cancelPending: (token) => call<void>("cancel_pending", { token }),
  backupLibrary: () => call<BackupDto>("backup_library"),
  listBackups: () => call<BackupDto[]>("list_backups"),
  inspectBackup: (name) => call<BackupPreview | null>("inspect_backup", { name }),
  restoreBackup: (request) => call<RestoreResult>("restore_backup", { request }),
  getPlayback: (itemId) => call<PlaybackInfo>("get_playback", { itemId }),
  savePlaybackPosition: (itemId, assetId, positionMs, durationMs) => call<PlaybackState>("save_playback_position", { itemId, assetId, positionMs, durationMs }),
  openExternally: (assetId) => call<void>("open_externally", { assetId }),
  openSource: (itemId) => call<void>("open_source", { itemId }),
  relinkAsset: (assetId) => call<ItemDetail | null>("relink_asset", { assetId }),
};

/** Real Tauri IPC client (STATE.md F0.3, F1.7). */
export function createTauriClient(): BackendClient {
  return {
    kind: "desktop",
    listItems: (query) => call<ItemPage>("list_items", { query }),
    getItem: (id) => call<ItemDetail>("get_item", { id }),
    importMedia: (source) => call<ImportSummary>("import_media", { source }),
    checkUrl: (url) => call<UrlCheck>("check_url", { url }),
    saveUrl: (url) => call<SaveUrlResult>("save_url", { url }),
    restoreItem: (id) => call<LibraryItemDto>("restore_item", { id }),
    ...organizeCommands,
    ...dataCommands,
    async onImportStatus(cb) {
      try {
        return await listen<ImportStatus>(IMPORT_EVENT, (e) => cb(e.payload));
      } catch (reason) {
        throw toIpcError("listen", reason);
      }
    },
    ping: (nonce) => call<Pong>("ping", { nonce }),
    getStatus: () => call<AppStatus>("get_status"),
    getDiagnostics: () => call<Diagnostics>("get_diagnostics"),
  };
}

const SCENARIOS: FixtureScenario[] = ["default", "empty", "many", "error", "slow", "first-run"];

/** `?fixture=empty|many|error|slow|first-run` picks a fixture data set (fixture mode only). */
export function scenarioFromUrl(search: string = typeof location === "undefined" ? "" : location.search): FixtureScenario {
  const s = new URLSearchParams(search).get("fixture");
  return SCENARIOS.includes(s as FixtureScenario) ? (s as FixtureScenario) : "default";
}

export function createClient(mode: string = import.meta.env.MODE): BackendClient {
  return mode === "fixture" || mode === "test" ? createFixtureClient(undefined, scenarioFromUrl()) : createTauriClient();
}
