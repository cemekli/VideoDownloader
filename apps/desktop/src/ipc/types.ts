// Frontend view of backend DTOs (SPEC §18). The UI never talks to the
// filesystem, shell or network directly; everything goes through a client.

export type Platform = "youtube" | "facebook" | "instagram" | "tiktok" | "x" | "vk" | "generic" | "local";
export const PLATFORMS: Platform[] = ["youtube", "facebook", "instagram", "tiktok", "x", "vk", "generic", "local"];

export type ItemKind = "bookmark" | "remote_video" | "local_video";
export const ITEM_KINDS: ItemKind[] = ["bookmark", "remote_video", "local_video"];

/**
 * Derived from MediaAsset records and their storage root on the backend,
 * never from `kind` (INV-02). `disconnected`: the drive holding the file is
 * not reachable now, which is not the same as the file being gone (DATA-09).
 */
export type LocalState = "bookmark_only" | "local_ready" | "disconnected" | "missing";

export interface SnippetPart {
  text: string;
  highlight: boolean;
}

/** A real substring of one field. Rendered as text, never as HTML (SEA-03). */
export interface Snippet {
  field: "title" | "description" | "creator" | "notes" | "tags";
  parts: SnippetPart[];
  cutStart: boolean;
  cutEnd: boolean;
}

export interface LibraryItemDto {
  id: string;
  title: string;
  platform: Platform;
  kind: ItemKind;
  localState: LocalState;
  /** Unknown duration is null, never 0 (SPEC §11.1). */
  durationMs: number | null;
  tags: string[];
  /** Local thumbnail protocol URL; null when there is none yet. */
  thumbnailUrl: string | null;
  addedAt: number;
  isFavorite: boolean;
  archived: boolean;
  /** Set while the item is in Recently removed (§7.3). */
  removedAt: number | null;
  /** Only with a search text. */
  snippet: Snippet | null;
}

/** `inbox`: not archived and in no manual collection; `removed`: Recently removed (§7.3). */
export type LibraryView = "all" | "inbox" | "favorites" | "recent" | "archive" | "removed";
export type SortKey = "relevance" | "added" | "title" | "duration" | "published";

export interface ListQuery {
  text: string;
  view: LibraryView;
  platforms: Platform[];
  kinds: ItemKind[];
  /** Items carrying any of these tags. */
  tagIds?: string[];
  /** Direct members of a manual collection. */
  collectionId?: string | null;
  /** Its stored rules replace the filters above; chips narrow further. */
  smartCollectionId?: string | null;
  sort: SortKey;
  descending: boolean;
  offset: number;
  limit: number;
}

export interface ItemPage {
  total: number;
  offset: number;
  items: LibraryItemDto[];
  /** `no_searchable_terms`: the text had no letter or digit; the filtered library is shown (SEA-04). */
  notice: "no_searchable_terms" | null;
}

export interface AssetDto {
  id: string;
  /** Last path component only; full paths stay in the backend. */
  fileName: string;
  rootLabel: string;
  rootStatus: "unknown" | "available" | "disconnected" | "inaccessible";
  integrityState: "unknown" | "verified" | "missing" | "corrupt" | "modified";
  sizeBytes: number | null;
  container: string | null;
  videoCodec: string | null;
  width: number | null;
  height: number | null;
  durationMs: number | null;
}

export interface ItemDetail {
  item: LibraryItemDto;
  canonicalUrl: string | null;
  creator: string | null;
  description: string | null;
  notes: string;
  publishedAt: number | null;
  publishedDate: string | null;
  metadataStatus: "pending" | "fetching" | "ready" | "partial" | "failed";
  updatedAt: number;
  archived: boolean;
  assets: AssetDto[];
  tagRefs: TagRef[];
  collectionIds: string[];
  /** Sent back with `setNotes`; REVISION_CONFLICT when it moved. */
  revision: number;
}

export type ImportSource = "files" | "folder";

export interface ImportSummary {
  /** false when the dialog was closed without a choice. */
  picked: boolean;
  imported: number;
  alreadyImported: number;
  duplicates: number;
  rejected: number;
  skipped: number;
  ignored: number;
  canceled: boolean;
  /** SPEC §19 code of the first refused file. */
  firstError: string | null;
}

/** Imports the UI did not start itself (files dropped on the window). */
export type ImportStatus = { phase: "started" } | { phase: "finished"; summary: ImportSummary } | { phase: "failed"; error: DomainError };

export type Unlisten = () => void;

/** An item that already holds the content of a URL (CAP-06). */
export interface ExistingItem {
  item: LibraryItemDto;
  /** In Recently removed: offer Restore / Cancel (§7.3, AC-45). */
  removed: boolean;
  matchedBy: "canonical_url" | "alias" | "source_id";
}

/** `check_url`: offline validation as the user types. Nothing is written. */
export interface UrlCheck {
  platform: Platform;
  shape: "video" | "collection" | "short_link" | "unknown";
  /** Canonical form only; the typed text is never echoed back (CAP-08). */
  canonicalUrl: string;
  /** Title a new bookmark gets until metadata arrives (CAP-04). */
  title: string;
  existing: ExistingItem | null;
  /** Non-blocking §19 codes, e.g. PLAYLIST_UNSUPPORTED. */
  warnings: string[];
}

export interface SaveUrlResult {
  /** `existing`: nothing was written; the URL belongs to `item`. */
  outcome: "created" | "existing";
  item: LibraryItemDto;
  removed: boolean;
  matchedBy: ExistingItem["matchedBy"] | null;
  warnings: string[];
}

// ---- Organize (STATE.md F2.1–F2.3, SPEC §7.3) ----

export interface TagRef {
  id: string;
  name: string;
}

export interface UndoInfo {
  kind: string;
  size: number;
}

/** Counts of a bulk or single operation (ORG-05). */
export interface OpResult {
  changed: number;
  /** Already in that state. */
  unchanged: number;
  /** Gone or in Recently removed. */
  failed: number;
  /** Set when Undo now reverts this operation. */
  undo: UndoInfo | null;
}

export interface UndoResult {
  kind: string;
  applied: number;
  /** Changed again since; left alone. */
  skipped: number;
}

export interface CollectionDto {
  id: string;
  name: string;
  parentId: string | null;
  depth: number;
  items: number;
}

/** Rules v1: every condition must hold (§7.3). */
export interface SmartRules {
  text: string;
  platforms: Platform[];
  kinds: ItemKind[];
  tagIds: string[];
  collectionIds: string[];
  favoriteOnly: boolean;
  includeArchived: boolean;
  addedWithinDays: number | null;
  minDurationMs: number | null;
  maxDurationMs: number | null;
}

export interface SmartCollectionDto {
  id: string;
  name: string;
  rules: SmartRules;
  /** A tag or collection it names is gone. */
  invalid: boolean;
}

export interface TagDto {
  id: string;
  name: string;
  items: number;
}

export interface Sidebar {
  collections: CollectionDto[];
  smartCollections: SmartCollectionDto[];
  tags: TagDto[];
  counts: { inbox: number; removed: number; duplicates: number; orphans: number };
}

export type ChildPolicy = "move_up" | "delete_all";

export interface NamedRef {
  id: string;
  name: string;
}

export interface DeleteResult {
  items: number;
  /** Managed originals kept in Orphaned media. */
  orphaned: number;
  files: { deleted: number; failed: number };
}

export interface DuplicateDto {
  id: string;
  reason: string;
  createdAt: number;
  a: LibraryItemDto;
  b: LibraryItemDto;
}

/** What a merge will do, shown before it runs (ORG-11). */
export interface MergePreview {
  target: LibraryItemDto;
  source: LibraryItemDto;
  notesAfter: string;
  tagsToAdd: TagRef[];
  collectionsToAdd: NamedRef[];
  mediaToMove: string[];
  aliasUrl: string | null;
  becomesFavorite: boolean;
}

export interface OrphanDto {
  id: string;
  fileName: string;
  rootLabel: string;
  rootStatus: string;
  sizeBytes: number | null;
  formerTitle: string | null;
  orphanedAt: number;
}

export interface OrganizeClient {
  getSidebar(): Promise<Sidebar>;
  undo(): Promise<UndoResult | null>;
  tagItems(itemIds: string[], name: string): Promise<OpResult>;
  untagItems(itemIds: string[], tagId: string): Promise<OpResult>;
  addToCollection(itemIds: string[], collectionId: string): Promise<OpResult>;
  removeFromCollection(itemIds: string[], collectionId: string): Promise<OpResult>;
  setFavorite(itemIds: string[], value: boolean): Promise<OpResult>;
  setArchived(itemIds: string[], value: boolean): Promise<OpResult>;
  removeItems(itemIds: string[]): Promise<OpResult>;
  restoreItems(itemIds: string[]): Promise<OpResult>;
  deleteItemsPermanently(itemIds: string[], deleteMedia: boolean): Promise<DeleteResult>;
  renameTag(id: string, name: string): Promise<OpResult>;
  mergeTags(sourceId: string, targetId: string): Promise<OpResult>;
  deleteTag(id: string): Promise<OpResult>;
  createCollection(name: string, parentId: string | null): Promise<NamedRef>;
  renameCollection(id: string, name: string): Promise<void>;
  moveCollection(id: string, parentId: string | null): Promise<void>;
  deleteCollection(id: string, policy: ChildPolicy): Promise<number>;
  saveSmartCollection(id: string | null, name: string, rules: SmartRules): Promise<NamedRef>;
  deleteSmartCollection(id: string): Promise<void>;
  setNotes(id: string, revision: number, notes: string): Promise<ItemDetail>;
  listDuplicates(): Promise<DuplicateDto[]>;
  previewMerge(candidateId: string, targetId: string): Promise<MergePreview>;
  mergeItems(candidateId: string, targetId: string): Promise<OpResult>;
  dismissDuplicate(candidateId: string): Promise<void>;
  listOrphans(): Promise<OrphanDto[]>;
  reimportOrphan(id: string): Promise<LibraryItemDto>;
  deleteOrphan(id: string): Promise<{ deleted: number; failed: number }>;
}

export interface LibraryClient extends OrganizeClient {
  listItems(query: ListQuery): Promise<ItemPage>;
  getItem(id: string): Promise<ItemDetail>;
  importMedia(source: ImportSource): Promise<ImportSummary>;
  onImportStatus(cb: (s: ImportStatus) => void): Promise<Unlisten>;
  checkUrl(url: string): Promise<UrlCheck>;
  saveUrl(url: string): Promise<SaveUrlResult>;
  restoreItem(id: string): Promise<LibraryItemDto>;
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

// ---- Player, settings, portability (STATE.md F2.4–F2.8) ----

export type Theme = "dark" | "light" | "system";
export type UpdateCheck = "manual" | "automatic";

export interface Settings {
  offlineMode: boolean;
  theme: Theme;
  updateCheck: UpdateCheck;
  locale: string;
  onboardingCompletedAt: number | null;
  legalAckAt: number | null;
  activeDownloadRootId: string | null;
}

export type SettingsPatch = Partial<Pick<Settings, "offlineMode" | "theme" | "updateCheck" | "locale">>;

export interface StorageOverview {
  /** Folder name only; paths stay in the backend. */
  label: string;
  isDefault: boolean;
  freeBytes: number | null;
  /** Inside OneDrive or a similar synced folder (§14.2, DATA-10). */
  cloudSynced: boolean;
}

export interface ExportResult {
  saved: boolean;
  items: number;
  mediaReferences: number;
  bytes: number;
  fileName: string | null;
}

export interface ImportRootPreview {
  id: string;
  kind: string;
  label: string;
  hasPath: boolean;
  references: number;
}

export interface ImportPreview {
  schemaVersion: number;
  exportedAt: string;
  appVersion: string;
  itemsNew: number;
  itemsMatched: number;
  itemsSkipped: number;
  removed: number;
  collections: number;
  smartCollections: number;
  tags: number;
  mediaReferences: number;
  absolutePaths: number;
  roots: ImportRootPreview[];
  unknownFields: number;
}

export interface PendingImport {
  token: string;
  fileName: string;
  preview: ImportPreview;
}

export type ConflictPolicy = "keep_existing" | "replace_user_fields";

export interface ApplyImport {
  token: string;
  policy: ConflictPolicy;
  useOriginalLocations: boolean;
}

export interface LibraryImportReport {
  itemsAdded: number;
  itemsMatched: number;
  itemsSkipped: number;
  removedReset: number;
  collectionsAdded: number;
  tagsAdded: number;
  smartCollectionsAdded: number;
  mediaLinked: number;
  mediaMissing: number;
  placeholderRoots: number;
  externalRoots: number;
  searchDocuments: number;
}

export interface BackupDto {
  name: string;
  kind: "manual" | "pre_restore" | "pre_migration";
  createdAt: number;
  sizeBytes: number;
}

export interface BackupPreview {
  /** Set when the file came from the dialog; pass it to `restoreBackup`. */
  token: string | null;
  name: string;
  schemaVersion: number;
  items: number;
  removed: number;
  collections: number;
  tags: number;
  mediaFiles: number;
}

export type RestoreRequest = { name: string; token?: undefined } | { token: string; name?: undefined };

export interface RestoreResult {
  items: number;
  removed: number;
  preRestoreBackup: string;
  migrated: number[];
  mediaMissing: number;
}

export type CodecSupport = "supported" | "unsupported" | "unknown";

export interface PlaybackSource {
  assetId: string;
  url: string;
  role: "original" | "preview";
  mediaKind: "video" | "audio";
  container: string | null;
  videoCodec: string | null;
  durationMs: number | null;
  available: boolean;
  support: CodecSupport;
}

export interface PlaybackState {
  assetId: string;
  positionMs: number;
  durationMs: number | null;
  lastPlayedAt: number;
  completed: boolean;
  resumable: boolean;
}

export interface PlaybackInfo {
  itemId: string;
  title: string;
  sources: PlaybackSource[];
  state: PlaybackState | null;
  hasSourceUrl: boolean;
}

export interface DataClient {
  getSettings(): Promise<Settings>;
  updateSettings(patch: SettingsPatch): Promise<Settings>;
  acknowledgeLegal(): Promise<Settings>;
  finishOnboarding(): Promise<Settings>;
  getStorageOverview(): Promise<StorageOverview>;
  chooseMediaFolder(): Promise<StorageOverview | null>;
  exportLibrary(includeAbsolutePaths: boolean): Promise<ExportResult>;
  previewLibraryImport(): Promise<PendingImport | null>;
  mapImportRoot(token: string, rootId: string): Promise<string | null>;
  applyLibraryImport(request: ApplyImport): Promise<LibraryImportReport>;
  cancelPending(token: string): Promise<void>;
  backupLibrary(): Promise<BackupDto>;
  listBackups(): Promise<BackupDto[]>;
  inspectBackup(name: string | null): Promise<BackupPreview | null>;
  restoreBackup(request: RestoreRequest): Promise<RestoreResult>;
  getPlayback(itemId: string): Promise<PlaybackInfo>;
  savePlaybackPosition(itemId: string, assetId: string, positionMs: number, durationMs: number | null): Promise<PlaybackState>;
  openExternally(assetId: string): Promise<void>;
  openSource(itemId: string): Promise<void>;
  relinkAsset(assetId: string): Promise<ItemDetail | null>;
}

export type BackendClient = LibraryClient & StatusClient & DataClient;
