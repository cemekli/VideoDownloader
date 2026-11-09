// Fixture side of the download, job and storage commands (F3). A download
// walks through the same states the engine reports (queued → running with
// progress → merge → succeeded), a few hundred milliseconds per step, and
// sends the same change events. The real engine is tested in Rust
// (crates/vv-engine/tests); this drives the Downloads screen and Journey 6.

import contract from "@fixtures/ipc-contract.json";
import { IpcError } from "./errors";
import type { FixtureRecord, FixtureScenario } from "./fixture";
import type { DownloadClient, DownloadOptions, JobChanged, JobDto, PresetId, StorageUsage, ToolStatus } from "./types";

const MB = 1024 * 1024;
const PRESETS: PresetId[] = ["best", "1080", "720", "audio"];

const err = (code: string, cmd: string) =>
  new IpcError(code, `${cmd}: error.${code.toLowerCase()}`, { code, messageKey: `error.${code.toLowerCase()}`, retryable: false, suggestedAction: null, diagnosticId: cmd, safeDetails: {} });

export interface FixtureDownloads {
  client: DownloadClient;
  /** Jobs as they are now (tests). */
  jobs(): JobDto[];
}

export function createFixtureDownloads(records: FixtureRecord[], scenario: FixtureScenario, stepMs = 300): FixtureDownloads {
  const jobs: JobDto[] = [];
  /** Run number per job: a retry or cancel makes older timers stale. */
  const runs = new Map<string, number>();
  const tries = new Map<string, number>();
  const listeners = new Set<(e: JobChanged) => void>();
  let storage: StorageUsage = { ...(contract.commands.get_storage_usage.response as StorageUsage), quotaBytes: null };
  let clock = 1_793_500_000_000;
  let seq = 0;
  const now = () => (clock += 1_000);

  const emit = (j: JobDto) => listeners.forEach((cb) => cb({ jobId: j.id, itemId: j.itemId }));
  const find = (id: string, cmd: string) => {
    const j = jobs.find((x) => x.id === id);
    if (!j) throw err("INPUT_REJECTED", cmd);
    return j;
  };
  const record = (id: string, cmd: string) => {
    const r = records.find((x) => x.id === id && !x.removed);
    if (!r || !r.canonicalUrl) throw err("INPUT_REJECTED", cmd);
    return r;
  };
  const actions = (j: JobDto): JobDto => {
    const terminal = j.status === "succeeded" || j.status === "failed" || j.status === "canceled";
    j.canRetry = j.status === "failed" || j.status === "canceled" || j.status === "blocked";
    j.canCancel = !terminal && j.status !== "canceling";
    j.canRemovePartial = j.partialBytes > 0 && (j.status === "failed" || j.status === "canceled");
    j.updatedAt = now();
    return j;
  };
  const set = (j: JobDto, patch: Partial<JobDto>) => {
    Object.assign(j, patch);
    actions(j);
    emit(j);
  };

  const run = (j: JobDto) => {
    const n = (runs.get(j.id) ?? 0) + 1;
    runs.set(j.id, n);
    const attempt = (tries.get(j.id) ?? 0) + 1;
    tries.set(j.id, attempt);
    const live = () => runs.get(j.id) === n && j.status !== "canceled";
    // Audio has no known total: the bar is indeterminate (AC-09).
    const total = j.kind === "download" && j.preset !== "audio" ? 100 * MB : null;
    const step = (k: number, f: () => void) =>
      setTimeout(() => {
        if (live()) f();
      }, stepMs * k);
    step(1, () => set(j, { status: "running", attempts: j.attempts + 1, phase: j.kind === "download" ? "download_video" : "copy", bytesDone: 0, bytesTotal: total }));
    step(2, () => set(j, { bytesDone: 50 * MB, partialBytes: j.kind === "download" ? Math.max(j.partialBytes, 50 * MB) : 0, unitsDone: j.kind === "storage_move" ? 1 : null }));
    if (scenario === "download-error" && j.kind === "download" && attempt === 1) {
      step(3, () => set(j, { status: "failed", phase: null, errorCode: "NETWORK_UNAVAILABLE", finishedAt: clock }));
      return;
    }
    step(3, () => set(j, { phase: j.kind === "download" ? (j.preset === "audio" ? "postprocess" : "merge") : "verify", bytesDone: total }));
    step(4, () => {
      set(j, { status: "succeeded", phase: null, partialBytes: 0, errorCode: null, finishedAt: clock, unitsDone: j.unitsTotal });
      const r = j.itemId ? records.find((x) => x.id === j.itemId) : undefined;
      if (r) {
        r.localState = "local_ready";
        r.fileName = j.preset === "audio" ? "downloaded.m4a" : "downloaded.mp4";
        storage = { ...storage, managedMediaBytes: storage.managedMediaBytes + 100 * MB };
        listeners.forEach((cb) => cb({ jobId: "", itemId: r.id }));
      }
    });
  };

  const newJob = (patch: Partial<JobDto>): JobDto => {
    const id = `00000000-0000-4000-8000-${String(0xc000 + ++seq).padStart(12, "0")}`;
    const t = now();
    const j = actions({
      ...(contract.commands.start_download.response as JobDto),
      id,
      createdAt: t,
      updatedAt: t,
      attempts: 0,
      errorCode: null,
      finishedAt: null,
      partialBytes: 0,
      ...patch,
    });
    jobs.unshift(j);
    emit(j);
    run(j);
    return j;
  };

  const client: DownloadClient = {
    async getToolHealth() {
      return structuredClone(contract.commands.get_tool_health.response) as ToolStatus;
    },
    async listJobs() {
      return jobs.filter((j) => j.kind !== "metadata").map((j) => ({ ...j }));
    },
    async getDownloadOptions(itemId) {
      const r = record(itemId, "get_download_options");
      const base = contract.commands.get_download_options.response as DownloadOptions;
      const open = jobs.find((j) => j.itemId === r.id && j.kind === "download" && !["succeeded", "failed", "canceled"].includes(j.status));
      return { ...structuredClone(base), itemId: r.id, storage: { ...storage }, activeJobId: open?.id ?? null };
    },
    async startDownload(itemId, preset) {
      if (!PRESETS.includes(preset)) throw err("INPUT_REJECTED", "start_download");
      const r = record(itemId, "start_download");
      if (storage.quotaBytes !== null && storage.managedMediaBytes >= storage.quotaBytes) throw err("QUOTA_EXCEEDED", "start_download");
      const open = jobs.find((j) => j.itemId === r.id && j.preset === preset && !["succeeded", "failed", "canceled"].includes(j.status));
      if (open) return { ...open };
      return { ...newJob({ kind: "download", itemId: r.id, itemTitle: r.title, preset, status: "queued", phase: null, bytesDone: null, bytesTotal: null }) };
    },
    async fetchMetadata(itemId) {
      record(itemId, "fetch_metadata");
      return `00000000-0000-4000-8000-${String(0xd000 + ++seq).padStart(12, "0")}`;
    },
    async cancelJob(jobId) {
      const j = find(jobId, "cancel_job");
      if (!j.canCancel) throw err("INPUT_REJECTED", "cancel_job");
      set(j, { status: "canceled", phase: null, finishedAt: clock });
      return { ...j };
    },
    async retryJob(jobId) {
      const j = find(jobId, "retry_job");
      if (!j.canRetry) throw err("INPUT_REJECTED", "retry_job");
      set(j, { status: "queued", attempts: 0, errorCode: null, finishedAt: null });
      run(j);
      return { ...j };
    },
    async removePartial(jobId) {
      const j = find(jobId, "remove_partial");
      if (j.status !== "failed" && j.status !== "canceled") throw err("INPUT_REJECTED", "remove_partial");
      const freedBytes = j.partialBytes;
      set(j, { partialBytes: 0 });
      return { freedBytes };
    },
    async getStorageUsage() {
      return { ...storage };
    },
    async setStorageQuota(bytes) {
      if (bytes === 0) throw err("INPUT_REJECTED", "set_storage_quota");
      storage = { ...storage, quotaBytes: bytes, quotaExceeded: bytes !== null && storage.managedMediaBytes >= bytes };
      return { ...storage };
    },
    async moveMediaFolder() {
      const m = contract.commands.move_media_folder.response as JobDto;
      return { ...newJob({ ...m, status: "queued", phase: null, unitsDone: 0 }) };
    },
    async onJobChanged(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
  };
  return { client, jobs: () => jobs.map((j) => ({ ...j })) };
}
