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
