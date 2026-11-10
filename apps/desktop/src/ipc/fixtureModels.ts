// Fixture transcription models (STATE.md F6.1). Same shapes as the backend
// (`model_commands.rs`); a download needs `agreed: true` (AC-20) and runs
// in a few timed steps, like the fixture downloads.
import contract from "@fixtures/ipc-contract.json";
import { IpcError } from "./errors";
import type { FixtureScenario } from "./fixture";
import type { JobDto, ModelClient, ModelDto, ModelOverview } from "./types";

const err = (code: string, cmd: string) =>
  new IpcError(code, `${cmd}: error.${code.toLowerCase()}`, { code, messageKey: `error.${code.toLowerCase()}`, retryable: false, suggestedAction: null, diagnosticId: cmd, safeDetails: {} });

export function createFixtureModels(_scenario: FixtureScenario, stepMs = 300): ModelClient {
  const base = contract.commands.list_models.response as ModelOverview;
  const models: ModelDto[] = base.models.map((m) => ({ ...m, status: "not_installed", consentAt: null, verifiedAt: null, job: null }));
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
