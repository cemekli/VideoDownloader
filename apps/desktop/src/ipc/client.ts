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
  DownloadClient,
  DownloadOptions,
  JobChanged,
  JobDto,
  StorageUsage,
  ToolStatus,
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
  SessionClient,
  SessionDto,
  SessionOverview,
  VerifyDto,
  ModelClient,
  ModelOverview,
  TranscriptClient,
  ItemTranscripts,
  TranscriptDetail,
  MomentTarget,
  NotePreview,
  NoteExport,
} from "./types";
import contract from "@fixtures/ipc-contract.json";
import { IpcError } from "./errors";
import { createFixtureLibrary, scenarioRecords, type FixtureLibrary, type FixtureRecord, type FixtureScenario } from "./fixture";
import { createFixtureData } from "./fixtureData";
import { createFixtureDownloads } from "./fixtureDownloads";
import { createFixtureModels } from "./fixtureModels";
import { createFixtureTranscripts } from "./fixtureTranscripts";

export { IpcError } from "./errors";

/** Must match `CONTRACT_VERSION` in `src-tauri/src/commands.rs`. */
export const CONTRACT_VERSION = 1;

/** Event the backend emits for imports started by a drop on the window (`library.rs`). */
export const IMPORT_EVENT = "import-status";

/** Jobs and items changed (`engine.rs`); ids only, details come from commands. */
export const JOB_EVENT = "job-changed";
export const ITEM_EVENT = "item-changed";
export const TOOLS_EVENT = "tools-checked";

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

export type FixtureClient = BackendClient & Pick<FixtureLibrary, "emitImportStatus"> & { downloads: ReturnType<typeof createFixtureDownloads> };

/**
 * Deterministic client backed by the shared fixture set (SPEC §25.2).
 * Used by `vite --mode fixture`, Vitest and Playwright E2E.
 */
export function createFixtureClient(records?: FixtureRecord[], scenario: FixtureScenario = "default"): FixtureClient {
  const recs = records ?? scenarioRecords(scenario);
  const transcripts = createFixtureTranscripts(recs, undefined, scenario);
  const library = createFixtureLibrary(recs, scenario, transcripts.spokenMatches);
  const downloads = createFixtureDownloads(recs, scenario);
  return {
    ...createFixtureData(recs, scenario, library),
    ...downloads.client,
    ...downloads.sessions,
    ...createFixtureModels(scenario),
    ...transcripts,
    // Transcription jobs live with the fixture transcripts; the rest are downloads.
    cancelJob: async (jobId) => transcripts.cancelOwnJob(jobId) ?? downloads.client.cancelJob(jobId),
    downloads,
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

/** Downloads, jobs and storage (F3). The UI names a preset, never a format. */
const downloadCommands: DownloadClient = {
  getToolHealth: () => call<ToolStatus>("get_tool_health"),
  listJobs: () => call<JobDto[]>("list_jobs"),
  getDownloadOptions: (itemId) => call<DownloadOptions>("get_download_options", { itemId }),
  startDownload: (itemId, preset) => call<JobDto>("start_download", { itemId, preset }),
  fetchMetadata: (itemId) => call<string>("fetch_metadata", { itemId }),
  cancelJob: (jobId) => call<JobDto>("cancel_job", { jobId }),
  retryJob: (jobId) => call<JobDto>("retry_job", { jobId }),
  removePartial: (jobId) => call<{ freedBytes: number }>("remove_partial", { jobId }),
  getStorageUsage: () => call<StorageUsage>("get_storage_usage"),
  setStorageQuota: (bytes) => call<StorageUsage>("set_storage_quota", { bytes }),
  moveMediaFolder: () => call<JobDto | null>("move_media_folder"),
  async onJobChanged(cb) {
    try {
      const a = await listen<JobChanged>(JOB_EVENT, (e) => cb(e.payload));
      const b = await listen<{ itemId: string }>(ITEM_EVENT, (e) => cb({ jobId: "", itemId: e.payload.itemId }));
      const c = await listen(TOOLS_EVENT, () => cb({ jobId: "", itemId: null }));
      return () => {
        a();
        b();
        c();
      };
    } catch (reason) {
      throw toIpcError("listen", reason);
    }
  },
};

/** Sessions (F5.2). The UI names a Firefox profile; the backend finds its folder. */
const sessionCommands: SessionClient = {
  listSessions: () => call<SessionOverview>("list_sessions"),
  connectSession: (provider, browser, browserProfile, scope) => call<SessionDto>("connect_session", { provider, browser, browserProfile, scope }),
  connectSessionFile: (provider, scope) => call<SessionDto | null>("connect_session_file", { provider, scope }),
  startAppSignIn: (provider) => call<void>("start_app_sign_in", { provider }),
  finishAppSignIn: (provider, scope) => call<SessionDto>("finish_app_sign_in", { provider, scope }),
  cancelAppSignIn: () => call<void>("cancel_app_sign_in"),
  reconnectSession: (id, scope) => call<SessionDto>("reconnect_session", { id, scope }),
  verifySession: (id) => call<VerifyDto>("verify_session", { id }),
  openSignIn: (provider) => call<void>("open_sign_in", { provider }),
  setSessionConsent: (id, scope) => call<SessionDto>("set_session_consent", { id, scope }),
  clearSession: (id) => call<SessionDto>("clear_session", { id }),
  deleteSession: (id) => call<void>("delete_session", { id }),
};

/** Transcription models (F6.1). */
const modelCommands: ModelClient = {
  listModels: () => call<ModelOverview>("list_models"),
  downloadModel: (modelId, agreed) => call<ModelOverview>("download_model", { modelId, agreed }),
  verifyModel: (modelId) => call<ModelOverview>("verify_model", { modelId }),
  removeModel: (modelId) => call<ModelOverview>("remove_model", { modelId }),
};

/** Transcripts (F6.2). */
const transcriptCommands: TranscriptClient = {
  listTranscripts: (itemId) => call<ItemTranscripts>("list_transcripts", { itemId }),
  getTranscript: (transcriptId) => call<TranscriptDetail>("get_transcript", { transcriptId }),
  fetchCaptions: (itemId) => call<ItemTranscripts>("fetch_captions", { itemId }),
  transcribeLocally: (itemId, modelId, language) => call<ItemTranscripts>("transcribe_locally", { itemId, modelId, language }),
  deleteTranscript: (transcriptId) => call<ItemTranscripts>("delete_transcript", { transcriptId }),
  getMoment: (segmentId) => call<MomentTarget>("get_moment", { segmentId }),
  previewNote: (itemId, segmentIds) => call<NotePreview>("preview_note", { itemId, segmentIds }),
  exportNote: (itemId, segmentIds) => call<NoteExport>("export_note", { itemId, segmentIds }),
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
    ...downloadCommands,
    ...sessionCommands,
    ...modelCommands,
    ...transcriptCommands,
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

const SCENARIOS: FixtureScenario[] = ["default", "empty", "many", "error", "slow", "first-run", "download-error", "login-required", "captions-error", "transcripts"];

/** `?fixture=empty|many|error|slow|first-run|download-error|login-required|captions-error|transcripts` picks a fixture data set (fixture mode only). */
export function scenarioFromUrl(search: string = typeof location === "undefined" ? "" : location.search): FixtureScenario {
  const s = new URLSearchParams(search).get("fixture");
  return SCENARIOS.includes(s as FixtureScenario) ? (s as FixtureScenario) : "default";
}

export function createClient(mode: string = import.meta.env.MODE): BackendClient {
  return mode === "fixture" || mode === "test" ? createFixtureClient(undefined, scenarioFromUrl()) : createTauriClient();
}
