import { invoke } from "@tauri-apps/api/core";
import type { AppStatus, BackendClient, Diagnostics, DomainError, LibraryItemDto, Pong } from "./types";
import libraryFixture from "@fixtures/library-small.json";
import contract from "@fixtures/ipc-contract.json";

/** Must match `CONTRACT_VERSION` in `src-tauri/src/commands.rs`. */
export const CONTRACT_VERSION = 1;

/** Error raised by every client call. `domain` is set when the backend sent a SPEC §18.3 DomainError. */
export class IpcError extends Error {
  readonly code: string;
  readonly domain: DomainError | null;
  constructor(code: string, message: string, domain: DomainError | null = null) {
    super(message);
    this.name = "IpcError";
    this.code = code;
    this.domain = domain;
  }
}

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

/**
 * Deterministic client backed by the shared fixture set (SPEC §25.2).
 * Used by `vite --mode fixture`, Vitest and Playwright E2E.
 */
export function createFixtureClient(items: LibraryItemDto[] = libraryFixture.items as LibraryItemDto[]): BackendClient {
  return {
    kind: "fixture",
    async listItems() {
      return structuredClone(items);
    },
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

/** Real Tauri IPC client (STATE.md F0.3). */
export function createTauriClient(): BackendClient {
  return {
    kind: "desktop",
    async listItems() {
      // There is no library storage before F1 (SQLite), so the desktop library
      // is genuinely empty. F1 replaces this with the `list_items` command.
      return [];
    },
    ping: (nonce) => call<Pong>("ping", { nonce }),
    getStatus: () => call<AppStatus>("get_status"),
    getDiagnostics: () => call<Diagnostics>("get_diagnostics"),
  };
}

export function createClient(mode: string = import.meta.env.MODE): BackendClient {
  return mode === "fixture" || mode === "test" ? createFixtureClient() : createTauriClient();
}
