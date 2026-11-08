// Fixture side of the player, settings and backup commands (F2.4–F2.8).
// Media plays from the real files of the playback matrix
// (`tests/fixtures/media`), which the Vite dev server serves under
// `/fixture-media/` in fixture mode only (vite.config.ts).

import contract from "@fixtures/ipc-contract.json";
import { IpcError } from "./errors";
import { fixtureAssetId, type FixtureLibrary, type FixtureRecord, type FixtureScenario } from "./fixture";
import type { BackupDto, DataClient, PlaybackState, Settings, StorageOverview } from "./types";

/** Which matrix file stands in for a fixture file name, and its codec. */
const MEDIA: Record<string, { file: string; codec: string }> = {
  "holiday.mp4": { file: "mp4-h264-aac.mp4", codec: "h264" },
  "postgres-replication.mp4": { file: "mp4-hevc-aac.mp4", codec: "hevc" },
};

export function fixtureMedia(fileName: string | null): { file: string; codec: string } | null {
  return (fileName && MEDIA[fileName]) || null;
}

export const FIXTURE_MEDIA_PATH = "/fixture-media/";
const THEME_KEY = "vv.theme";

function storedTheme(): Settings["theme"] {
  try {
    const v = localStorage.getItem(THEME_KEY);
    return v === "light" || v === "system" ? v : "dark";
  } catch {
    return "dark";
  }
}

const assetOf = (r: FixtureRecord) => fixtureAssetId(r.id);

export function createFixtureData(records: FixtureRecord[], scenario: FixtureScenario, library: FixtureLibrary): DataClient {
  const base = contract.commands.get_settings.response as Settings;
  const done = scenario !== "first-run";
  let settings: Settings = { ...base, theme: storedTheme(), onboardingCompletedAt: done ? 1_793_491_200_000 : null, legalAckAt: done ? 1_793_491_200_000 : null };
  let storage: StorageOverview = structuredClone(contract.commands.get_storage_overview.response) as StorageOverview;
  const playback = new Map<string, PlaybackState>();
  // The default library has one saved position, so the resume prompt can be
  // tried on the 3-second matrix file (a real save needs 5 s or more).
  const holiday = scenario === "default" ? records.find((r) => r.fileName === "holiday.mp4") : undefined;
  if (holiday) playback.set(holiday.id, { assetId: assetOf(holiday), positionMs: 2_000, durationMs: 3_000, lastPlayedAt: 1_793_400_000_000, completed: false, resumable: true });
  const backups: BackupDto[] = [];
  const pending = new Set<string>();
  let clock = 1_793_500_000_000;
  const now = () => (clock += 60_000);
  const record = (id: string) => {
    const r = records.find((x) => x.id === id && !x.removed);
    if (!r) throw new IpcError("INTERNAL_ERROR", "fixture: error.internal");
    return r;
  };
  const err = (code: string, cmd: string) => new IpcError(code, `${cmd}: error.${code.toLowerCase()}`, { code, messageKey: `error.${code.toLowerCase()}`, retryable: false, suggestedAction: null, diagnosticId: cmd, safeDetails: {} });

  return {
    async getSettings() {
      return { ...settings };
    },
    async updateSettings(patch) {
      if (patch.locale && patch.locale !== "en") throw err("INPUT_REJECTED", "settings-invalid");
      settings = { ...settings, ...patch };
      return { ...settings };
    },
    async acknowledgeLegal() {
      settings = { ...settings, legalAckAt: now() };
      return { ...settings };
    },
    async finishOnboarding() {
      settings = { ...settings, onboardingCompletedAt: now() };
      return { ...settings };
    },
    async getStorageOverview() {
      return { ...storage };
    },
    async chooseMediaFolder() {
      storage = { label: "Videos", isDefault: false, freeBytes: 120_000_000_000, cloudSynced: false };
      return { ...storage };
    },
    async exportLibrary() {
      return structuredClone(contract.commands.export_library.response);
    },
    async previewLibraryImport() {
      const p = structuredClone(contract.commands.preview_library_import.response) as Awaited<ReturnType<DataClient["previewLibraryImport"]>>;
      if (p) pending.add(p.token);
      return p;
    },
    async mapImportRoot(token) {
      if (!pending.has(token)) throw err("INTERNAL_ERROR", "import-token");
      return "Videos";
    },
    async applyLibraryImport(req) {
      if (!pending.delete(req.token)) throw err("INTERNAL_ERROR", "import-token");
      return structuredClone(contract.commands.apply_library_import.response);
    },
    async cancelPending(token) {
      pending.delete(token);
    },
    async backupLibrary() {
      const t = now();
      const b: BackupDto = { name: `backup-${t}.sqlite`, kind: "manual", createdAt: t, sizeBytes: 409_600 };
      backups.unshift(b);
      return { ...b };
    },
    async listBackups() {
      return backups.map((b) => ({ ...b }));
    },
    async inspectBackup(name) {
      const b = name ? backups.find((x) => x.name === name) : null;
      if (name && !b) throw err("FILE_MISSING", "backup-not-listed");
      const live = records.filter((r) => !r.removed);
      return { token: name ? null : "fixture-picked", name: name ?? "picked.sqlite", schemaVersion: 4, items: live.length, removed: records.length - live.length, collections: 0, tags: 0, mediaFiles: records.filter((r) => r.fileName).length };
    },
    async restoreBackup(req) {
      const t = now();
      backups.unshift({ name: `pre-restore-${t}.sqlite`, kind: "pre_restore", createdAt: t, sizeBytes: 409_600 });
      void req;
      return { items: records.filter((r) => !r.removed).length, removed: records.filter((r) => r.removed).length, preRestoreBackup: `pre-restore-${t}.sqlite`, migrated: [], mediaMissing: records.filter((r) => r.localState === "missing" || r.localState === "disconnected").length };
    },
    async getPlayback(itemId) {
      const r = record(itemId);
      const m = fixtureMedia(r.fileName);
      const sources = r.fileName
        ? [
            {
              assetId: assetOf(r),
              url: m ? `${FIXTURE_MEDIA_PATH}${m.file}` : `${FIXTURE_MEDIA_PATH}missing.mp4`,
              role: "original" as const,
              mediaKind: "video" as const,
              container: r.fileName.split(".").pop() ?? null,
              videoCodec: m?.codec ?? "h264",
              durationMs: r.durationMs,
              available: r.localState === "local_ready",
              support: m?.codec === "hevc" ? ("unsupported" as const) : ("supported" as const),
            },
          ]
        : [];
      return { itemId, title: r.title, sources, state: playback.get(itemId) ?? null, hasSourceUrl: !!r.canonicalUrl };
    },
    async savePlaybackPosition(itemId, assetId, positionMs, durationMs) {
      const r = record(itemId);
      if (assetId !== assetOf(r) || positionMs < 0) throw err("INPUT_REJECTED", "playback-save-invalid");
      const pos = durationMs ? Math.min(positionMs, durationMs) : positionMs;
      const completed = !!durationMs && (pos >= durationMs * 0.95 || (durationMs >= 60_000 && durationMs - pos <= 10_000));
      const s: PlaybackState = { assetId, positionMs: pos, durationMs, lastPlayedAt: now(), completed, resumable: !completed && pos >= 5_000 };
      playback.set(itemId, s);
      return { ...s };
    },
    async openExternally(assetId) {
      if (!records.some((r) => assetOf(r) === assetId && r.localState === "local_ready")) throw err("FILE_MISSING", "media-file-missing");
    },
    async openSource(itemId) {
      if (settings.offlineMode) throw err("OFFLINE_MODE", "offline-mode");
      if (!record(itemId).canonicalUrl) throw err("INTERNAL_ERROR", "open-source-no-url");
    },
    async relinkAsset(assetId) {
      const r = records.find((x) => assetOf(x) === assetId);
      if (!r) throw err("FILE_MISSING", "relink-unknown-asset");
      r.localState = "local_ready";
      return library.getItem(r.id);
    },
  };
}
