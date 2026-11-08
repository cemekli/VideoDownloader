import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AppStatus, BackendClient, Diagnostics, DomainError, ImportStatus, ImportSummary, ItemDetail, ItemPage, Pong } from "./types";
import contract from "@fixtures/ipc-contract.json";
import { IpcError } from "./errors";
import { createFixtureLibrary, scenarioRecords, type FixtureLibrary, type FixtureRecord, type FixtureScenario } from "./fixture";

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
  const library = createFixtureLibrary(records ?? scenarioRecords(scenario), scenario);
  return {
    kind: "fixture",
    listItems: (q) => library.listItems(q),
    getItem: (id) => library.getItem(id),
    importMedia: (s) => library.importMedia(s),
    onImportStatus: (cb) => library.onImportStatus(cb),
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

/** Real Tauri IPC client (STATE.md F0.3, F1.7). */
export function createTauriClient(): BackendClient {
  return {
    kind: "desktop",
    listItems: (query) => call<ItemPage>("list_items", { query }),
    getItem: (id) => call<ItemDetail>("get_item", { id }),
    importMedia: (source) => call<ImportSummary>("import_media", { source }),
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

const SCENARIOS: FixtureScenario[] = ["default", "empty", "many", "error", "slow"];

/** `?fixture=empty|many|error|slow` picks a fixture data set (fixture mode only). */
export function scenarioFromUrl(search: string = typeof location === "undefined" ? "" : location.search): FixtureScenario {
  const s = new URLSearchParams(search).get("fixture");
  return SCENARIOS.includes(s as FixtureScenario) ? (s as FixtureScenario) : "default";
}

export function createClient(mode: string = import.meta.env.MODE): BackendClient {
  return mode === "fixture" || mode === "test" ? createFixtureClient(undefined, scenarioFromUrl()) : createTauriClient();
}
