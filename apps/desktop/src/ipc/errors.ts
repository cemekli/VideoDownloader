import type { DomainError } from "./types";

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

/** The §19 code of any error a client call threw. */
export function errorCode(e: unknown): string {
  return e instanceof IpcError ? e.code : "IPC_FAILED";
}
