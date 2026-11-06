import type { LibraryClient, LibraryItemDto } from "./types";
import libraryFixture from "@fixtures/library-small.json";

/**
 * Deterministic client backed by the shared fixture set (SPEC §25.2).
 * Used by `vite --mode fixture`, Vitest and Playwright E2E.
 */
export function createFixtureClient(items: LibraryItemDto[] = libraryFixture.items as LibraryItemDto[]): LibraryClient {
  return {
    async listItems() {
      return structuredClone(items);
    },
  };
}

/**
 * The Tauri IPC client arrives in F0.3 (Tauri shell). Until then the
 * non-fixture build reports that honestly instead of pretending to work.
 */
export function createTauriClient(): LibraryClient {
  return {
    async listItems() {
      throw new Error("Tauri IPC is not implemented yet (STATE.md F0.3).");
    },
  };
}

export function createClient(mode: string = import.meta.env.MODE): LibraryClient {
  return mode === "fixture" || mode === "test" ? createFixtureClient() : createTauriClient();
}
