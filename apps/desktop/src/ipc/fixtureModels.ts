// Fixture transcription models (STATE.md F6.1). Same shapes as the backend
// (`model_commands.rs`); a download needs `agreed: true` (AC-20) and runs
// in a few timed steps, like the fixture downloads.
import contract from "@fixtures/ipc-contract.json";
import { IpcError } from "./errors";
import type { FixtureScenario } from "./fixture";
import type { CpuUse, JobDto, MachineDto, MemoryFit, MemoryInfo, ModelClient, ModelDto, ModelOverview } from "./types";

const MB = 1024 * 1024;

/** Same estimate as the backend (`resources::need_mb`). */
export const needMb = (upstreamMb: number) => upstreamMb + Math.floor(upstreamMb / 4) + 128;

/** Same rule as the backend (`resources::fit`): 1.5 GB stays for the system. */
export function fitOf(need: number, mem: MemoryInfo | null): MemoryFit {
  if (!mem) return "unknown";
  if (mem.totalBytes < (need + 1536) * MB) return "too_big";
  return mem.availableBytes < need * MB ? "tight" : "ok";
}

/** `low-memory`: a 4 GB PC with 2.5 GB free, where Medium does not fit (AI-A05). Otherwise 16 GB. */
export const fixtureMemory = (scenario: FixtureScenario): MemoryInfo =>
  scenario === "low-memory" ? { totalBytes: 4096 * MB, availableBytes: 2500 * MB } : { totalBytes: 16384 * MB, availableBytes: 9000 * MB };

export function fixtureMachine(scenario: FixtureScenario, cpuUse: CpuUse): MachineDto {
  const cores = 8;
  const full = Math.min(8, Math.max(1, cores - 1));
  const threads = cpuUse === "light" ? Math.min(full, 2) : cpuUse === "balanced" ? Math.max(1, Math.min(full, Math.floor(cores / 2))) : full;
  return { memory: fixtureMemory(scenario), cores, cpuUse, threads, transcriber: "ok", stagingFreeBytes: 120_000_000_000 };
}

/** Medium as the catalog has it, for the low-memory scenario. */
const MEDIUM = { id: "whisper-medium", label: "Medium", sizeBytes: 1533763059, upstreamMemoryMb: 2100 };

const err = (code: string, cmd: string) =>
  new IpcError(code, `${cmd}: error.${code.toLowerCase()}`, { code, messageKey: `error.${code.toLowerCase()}`, retryable: false, suggestedAction: null, diagnosticId: cmd, safeDetails: {} });

export function createFixtureModels(scenario: FixtureScenario, stepMs = 300): Omit<ModelClient, "getAiProcessing"> {
  const base = contract.commands.list_models.response as ModelOverview;
  const mem = fixtureMemory(scenario);
  const models: ModelDto[] = base.models.map((m) => ({ ...m, needMemoryMb: needMb(m.upstreamMemoryMb), fit: fitOf(needMb(m.upstreamMemoryMb), mem), status: "not_installed", consentAt: null, verifiedAt: null, job: null }));
  if (scenario === "low-memory") {
    // Base and Medium installed; Medium needs more than this PC has.
    const b = models[1]!;
    const ready = { status: "ready" as const, consentAt: 1_793_000_000_000, verifiedAt: 1_793_000_100_000 };
    models[1] = { ...b, ...ready };
    models.push({ ...b, ...ready, ...MEDIUM, needMemoryMb: needMb(MEDIUM.upstreamMemoryMb), fit: fitOf(needMb(MEDIUM.upstreamMemoryMb), mem) });
  }
  let clock = 1_793_600_000_000;
  const now = () => (clock += 1_000);
  const timers = new Map<string, ReturnType<typeof setTimeout>>();
  const view = (): ModelOverview => structuredClone({ models, freeBytes: base.freeBytes });
  const find = (id: string, cmd: string) => models.find((m) => m.id === id) ?? (() => { throw err("INPUT_REJECTED", cmd); })();

  const step = (m: ModelDto) => {
    const j = m.job;
    if (!j || j.status !== "running" || m.sizeBytes === null) return;
    const done = Math.min(m.sizeBytes, (j.bytesDone ?? 0) + Math.ceil(m.sizeBytes / 4));
    j.bytesDone = done;
    j.updatedAt = now();
    if (done >= m.sizeBytes) {
      j.status = "succeeded";
      j.phase = "verify";
      j.finishedAt = j.updatedAt;
      j.canCancel = false;
      m.status = "ready";
      m.verifiedAt = j.updatedAt;
      return;
    }
    timers.set(m.id, setTimeout(() => step(m), stepMs));
  };

  return {
    async listModels() {
      return view();
    },
    async downloadModel(id, agreed) {
      if (!agreed) throw err("INPUT_REJECTED", "download_model");
      const m = find(id, "download_model");
      if (m.sizeBytes === null) throw err("INPUT_REJECTED", "download_model");
      if (m.status === "ready") return view();
      if (m.job && !["succeeded", "failed", "canceled"].includes(m.job.status)) return view();
      const t = now();
      m.status = "pending";
      m.consentAt = m.consentAt ?? t;
      const job: JobDto = {
        ...(contract.commands.start_download.response as JobDto),
        id: `00000000-0000-4000-8000-${String(0xd000 + models.indexOf(m)).padStart(12, "0")}`,
        kind: "model_download",
        itemId: null,
        itemTitle: null,
        itemPlatform: null,
        status: "running",
        phase: "download",
        bytesDone: 0,
        bytesTotal: m.sizeBytes,
        bytesEstimate: null,
        preset: null,
        attempts: 1,
        errorCode: null,
        blockedReason: null,
        partialBytes: 0,
        createdAt: t,
        updatedAt: t,
        finishedAt: null,
        canRetry: false,
        canCancel: true,
        canRemovePartial: false,
      };
      m.job = job;
      timers.set(m.id, setTimeout(() => step(m), stepMs));
      return view();
    },
    async verifyModel(id) {
      const m = find(id, "verify_model");
      if (m.status === "not_installed") throw err("MODEL_MISSING", "verify_model");
      m.verifiedAt = now();
      return view();
    },
    async removeModel(id) {
      const m = find(id, "remove_model");
      clearTimeout(timers.get(id));
      if (m.job && !["succeeded", "failed", "canceled"].includes(m.job.status)) {
        m.job.status = "canceled";
        m.job.canCancel = false;
      }
      m.status = "not_installed";
      m.consentAt = null;
      m.verifiedAt = null;
      return view();
    },
  };
}
