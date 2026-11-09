// Fixture side of the download, job and storage commands (F3). A download
// walks through the same states the engine reports (queued → running with
// progress → merge → succeeded), a few hundred milliseconds per step, and
// sends the same change events. The real engine is tested in Rust
// (crates/vv-engine/tests); this drives the Downloads screen and Journey 6.

import contract from "@fixtures/ipc-contract.json";
import { IpcError } from "./errors";
import type { FixtureRecord, FixtureScenario } from "./fixture";
import type { BrowserProfileDto, DownloadClient, DownloadOptions, JobChanged, JobDto, Platform, PresetId, SessionClient, SessionDto, StorageUsage, ToolStatus } from "./types";

const MB = 1024 * 1024;
const PRESETS: PresetId[] = ["best", "1080", "720", "audio"];

const err = (code: string, cmd: string) =>
  new IpcError(code, `${cmd}: error.${code.toLowerCase()}`, { code, messageKey: `error.${code.toLowerCase()}`, retryable: false, suggestedAction: null, diagnosticId: cmd, safeDetails: {} });

export interface FixtureDownloads {
  client: DownloadClient;
  /** Sessions (F5.2). In the `login-required` scenario Instagram, VK and Reddit downloads wait for one. */
  sessions: SessionClient;
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

  const sessions: SessionDto[] = [];
  const browserProfiles: BrowserProfileDto[] = [
    { browser: "firefox", name: "default-release", isDefault: true, hasCookies: true },
    { browser: "firefox", name: "work", isDefault: false, hasCookies: false },
  ];
  const NEEDS_LOGIN = new Set<string>(scenario === "login-required" ? ["instagram", "vk", "reddit"] : []);
  const sessionFor = (platform: string | null) =>
    sessions.find((x) => x.provider === platform && x.consent.includes("download") && x.state !== "not_connected" && x.state !== "unsupported");
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
    // Audio has no known total: the bar is indeterminate (AC-09). "best"
    // reports only an estimate, as YouTube's chunked downloads do.
    const total = j.kind === "download" && j.preset !== "audio" && j.preset !== "best" ? 100 * MB : null;
    const estimate = j.kind === "download" && j.preset === "best" ? 100 * MB : null;
    const step = (k: number, f: () => void) =>
      setTimeout(() => {
        if (live()) f();
      }, stepMs * k);
    const login = j.kind === "download" && NEEDS_LOGIN.has(j.itemPlatform ?? "");
    const session = login ? sessionFor(j.itemPlatform) : undefined;
    if (login && !session) {
      step(1, () => set(j, { status: "blocked", blockedReason: "auth", phase: null }));
      return;
    }
    step(1, () => set(j, { status: "running", attempts: j.attempts + 1, phase: j.kind === "download" ? "download_video" : "copy", bytesDone: 0, bytesTotal: total, bytesEstimate: estimate }));
    step(2, () => set(j, { bytesDone: 50 * MB, partialBytes: j.kind === "download" ? Math.max(j.partialBytes, 50 * MB) : 0, unitsDone: j.kind === "storage_move" ? 1 : null }));
    if (scenario === "download-error" && j.kind === "download" && attempt === 1) {
      step(3, () => set(j, { status: "failed", phase: null, errorCode: "NETWORK_UNAVAILABLE", finishedAt: clock }));
      return;
    }
    step(3, () => set(j, { phase: j.kind === "download" ? (j.preset === "audio" ? "postprocess" : "merge") : "verify", bytesDone: total }));
    step(4, () => {
      set(j, { status: "succeeded", phase: null, partialBytes: 0, errorCode: null, finishedAt: clock, unitsDone: j.unitsTotal });
      if (session) Object.assign(session, { state: "session_available", lastVerifiedAt: clock });
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
      return { ...newJob({ kind: "download", itemId: r.id, itemTitle: r.title, itemPlatform: r.platform as Platform, preset, status: "queued", phase: null, bytesDone: null, bytesTotal: null, bytesEstimate: null }) };
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
  const findSession = (id: string, cmd: string) => {
    const x = sessions.find((y) => y.id === id);
    if (!x) throw err("INPUT_REJECTED", cmd);
    return x;
  };
  const resume = (platform: string) => {
    for (const j of jobs) if (j.status === "blocked" && j.blockedReason === "auth" && j.itemPlatform === platform) {
      set(j, { status: "queued", blockedReason: null });
      run(j);
    }
  };
  const sessionClient: SessionClient = {
    async listSessions() {
      return structuredClone({ sessions, browserProfiles, providers: (contract.commands.list_sessions.response.providers as Platform[]) });
    },
    async connectSession(provider, browser, browserProfile, scope) {
      if (browser !== "firefox" || provider === "generic" || provider === "local") throw err("INPUT_REJECTED", "connect_session");
      if (!browserProfiles.some((b) => b.name === browserProfile)) throw err("COOKIE_ACCESS_FAILED", "connect_session");
      if (scope.length === 0) throw err("INPUT_REJECTED", "connect_session");
      const name = provider.charAt(0).toUpperCase() + provider.slice(1);
      const x: SessionDto = {
        id: `00000000-0000-4000-8000-${String(0xd000 + ++seq).padStart(12, "0")}`,
        provider,
        label: `${name} · Firefox (${browserProfile})`,
        method: "browser_cookies",
        browser: "firefox",
        browserProfile,
        state: "needs_verification",
        consent: [...scope],
        consentAt: now(),
        lastVerifiedAt: null,
      };
      sessions.push(x);
      resume(provider);
      return structuredClone(x);
    },
    async setSessionConsent(id, scope) {
      const x = findSession(id, "set_session_consent");
      x.consent = [...scope];
      x.consentAt = scope.length ? now() : null;
      if (scope.length) resume(x.provider);
      return structuredClone(x);
    },
    async clearSession(id) {
      const x = findSession(id, "clear_session");
      Object.assign(x, { consent: [], consentAt: null, state: "not_connected", lastVerifiedAt: null });
      return structuredClone(x);
    },
    async deleteSession(id) {
      findSession(id, "delete_session");
      sessions.splice(sessions.findIndex((y) => y.id === id), 1);
    },
  };

  return { client, sessions: sessionClient, jobs: () => jobs.map((j) => ({ ...j })) };
}
