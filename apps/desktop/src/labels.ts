// User-facing names for backend values. Kept in one place so F2.8 can turn
// them into translation keys (SPEC §9.6).

import type { ItemKind, LibraryView, LocalState, Platform } from "./ipc/types";

export const PLATFORM_LABEL: Record<Platform, string> = {
  youtube: "YouTube",
  facebook: "Facebook",
  instagram: "Instagram",
  tiktok: "TikTok",
  x: "X",
  vk: "VK",
  generic: "Web",
  local: "Local file",
};

export const KIND_LABEL: Record<ItemKind, string> = {
  bookmark: "Bookmarks",
  remote_video: "Online videos",
  local_video: "Local files",
};

/** Text and a symbol for each state, so it never depends on colour alone (§9.4). */
export const LOCAL_STATE: Record<LocalState, { label: string; icon: string; hint: string }> = {
  bookmark_only: { label: "Bookmark only", icon: "🔖", hint: "Saved link; no file on this computer." },
  local_ready: { label: "Downloaded", icon: "✓", hint: "The file is on this computer." },
  disconnected: { label: "Drive disconnected", icon: "⏏", hint: "The drive or folder with this file is not connected. Nothing was deleted." },
  missing: { label: "File missing", icon: "!", hint: "The file is no longer where it was imported from." },
};

export const VIEW_LABEL: Record<LibraryView, string> = {
  all: "All Items",
  favorites: "Favorites",
  recent: "Recently Added",
  archive: "Archive",
};

const IMPORT_REASON: Record<string, string> = {
  INPUT_REJECTED: "it is a playlist, manifest or other unsupported input",
  MEDIA_CORRUPT: "it is damaged or not a video or audio file",
  TOOL_MISSING: "the media tools are missing",
  TIMEOUT: "reading it took too long",
  FILE_MISSING: "it could not be found",
};

export function importReason(code: string | null): string {
  return (code && IMPORT_REASON[code]) || "it could not be read";
}

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

export function importMessage(s: { imported: number; alreadyImported: number; duplicates: number; rejected: number; skipped: number; canceled: boolean; firstError: string | null }): string {
  const parts: string[] = [];
  if (s.imported) parts.push(`Imported ${plural(s.imported, "file", "files")}.`);
  if (s.duplicates) parts.push(`${plural(s.duplicates, "file looks", "files look")} like a copy of an existing item; nothing was merged.`);
  if (s.alreadyImported) parts.push(`${plural(s.alreadyImported, "file was", "files were")} already in the library.`);
  if (s.rejected) parts.push(`${plural(s.rejected, "file was", "files were")} not imported: ${importReason(s.firstError)}.`);
  if (s.skipped) parts.push(`${plural(s.skipped, "item was", "items were")} skipped (links, cloud-only or unreadable).`);
  if (s.canceled) parts.push("Import was canceled; finished files were kept.");
  return parts.join(" ") || "No video or audio files were found.";
}

export function importErrorMessage(code: string): string {
  if (code === "TOOL_MISSING") return "Import needs the bundled media tools (ffprobe), which were not found.";
  return `Import failed (${code}).`;
}

const URL_ERROR: Record<string, string> = {
  INVALID_URL: "This is not a web link. Paste a link that starts with https://.",
  URL_BLOCKED: "Links to this computer or a local network can't be saved.",
  URL_HAS_CREDENTIALS: "This link contains a password or a temporary access key. Copy the normal link of the video instead.",
};

/** Field error for an Add URL refusal (§8.1: a fixable field error). */
export function urlErrorMessage(code: string): string {
  return URL_ERROR[code] ?? `The link could not be saved (${code}).`;
}

export function urlWarningMessage(code: string): string {
  if (code === "PLAYLIST_UNSUPPORTED") return "This is a playlist, channel or profile. It is saved as one bookmark; its videos are not added one by one.";
  return code;
}
