// Frontend view of backend DTOs (SPEC §18). The UI never talks to the
// filesystem, shell or network directly; everything goes through a client.

export type Platform = "youtube" | "facebook" | "instagram" | "tiktok" | "x" | "vk" | "generic" | "local";

export type ItemKind = "bookmark" | "remote_video" | "local_video";

/** Derived from MediaAsset records on the backend, never from `kind` (INV-02). */
export type LocalState = "bookmark_only" | "local_ready" | "missing";

export interface LibraryItemDto {
  id: string;
  title: string;
  platform: Platform;
  kind: ItemKind;
  localState: LocalState;
  /** Unknown duration is null, never 0 (SPEC §11.1). */
  durationMs: number | null;
  tags: string[];
}

export interface LibraryClient {
  listItems(): Promise<LibraryItemDto[]>;
}

// ---- IPC contract v1 (tests/fixtures/ipc-contract.json, SPEC §18) ----

export interface Pong {
  nonce: number;
  contractVersion: number;
}

export interface AppStatus {
  appName: string;
  appVersion: string;
  contractVersion: number;
  os: string;
  arch: string;
  buildProfile: "debug" | "release";
}

export interface Diagnostics {
  /** SQLite compiled into the app (SPEC §16.1). */
  sqliteVersion: string;
  sqliteFts5: boolean;
  schemaVersion: number;
  /** This process holds the library lock (INV-13). */
  libraryLock: boolean;
  /** Launches handed over by a second instance (AC-38). */
  secondLaunches: number;
}

/** Where the UI gets its data from. Shown to the user so fixture data is never mistaken for a real library. */
export type BackendKind = "desktop" | "fixture";

/** SPEC §18.3. Codes are the stable strings from SPEC §19. */
export interface DomainError {
  code: string;
  messageKey: string;
  retryable: boolean;
  suggestedAction: string | null;
  diagnosticId: string;
  safeDetails: Record<string, string>;
}

export interface StatusClient {
  readonly kind: BackendKind;
  ping(nonce: number): Promise<Pong>;
  getStatus(): Promise<AppStatus>;
  getDiagnostics(): Promise<Diagnostics>;
}

export type BackendClient = LibraryClient & StatusClient;
