// Fixture stand-in for the backend's URL checks (fixture mode, Vitest and the
// fixture E2E only). The real rules live in Rust (`vv_domain::url_policy`,
// `vv_domain::normalize`, tested against tests/fixtures/normalize.json); this
// covers just enough of them to drive every Add URL state.

import type { Platform, UrlCheck } from "./types";

/** Same limit as `MAX_URL_LEN` in `capture_commands.rs`. */
export const MAX_URL_LEN = 8 * 1024;

const HOSTS: Array<[RegExp, Platform, string]> = [
  [/(^|\.)(youtube\.com|youtu\.be)$/, "youtube", "YouTube"],
  [/(^|\.)(facebook\.com|fb\.watch)$/, "facebook", "Facebook"],
  [/(^|\.)instagram\.com$/, "instagram", "Instagram"],
  [/(^|\.)tiktok\.com$/, "tiktok", "TikTok"],
  [/(^|\.)(x\.com|twitter\.com)$/, "x", "X"],
  [/(^|\.)(vk\.com|vkvideo\.ru)$/, "vk", "VK"],
  [/(^|\.)(dailymotion\.com|dai\.ly)$/, "dailymotion", "Dailymotion"],
  [/(^|\.)twitch\.tv$/, "twitch", "Twitch"],
];

const BLOCKED = /^(localhost|127\.|10\.|192\.168\.|169\.254\.|0\.0\.0\.0|\[::1\])/;

export type FixtureUrl = Omit<UrlCheck, "existing">;

/** Throws the §19 code as a string when the backend would refuse the input. */
export function fixtureParseUrl(input: string): FixtureUrl {
  const text = input.trim();
  if (!text || text.length > MAX_URL_LEN) throw "INVALID_URL";
  let u: URL;
  try {
    u = new URL(text);
  } catch {
    throw "INVALID_URL";
  }
  if (u.protocol !== "https:" && u.protocol !== "http:") throw "INVALID_URL";
  if (u.username || u.password) throw "URL_HAS_CREDENTIALS";
  const host = u.hostname.toLowerCase();
  if (BLOCKED.test(host)) throw "URL_BLOCKED";
  const [, platform, name] = HOSTS.find(([re]) => re.test(host)) ?? [null, "generic" as Platform, host.replace(/^www\./, "")];
  u.hash = "";
  for (const p of ["si", "utm_source", "utm_medium", "utm_campaign", "feature"]) u.searchParams.delete(p);

  if (platform === "youtube") {
    const id = host.endsWith("youtu.be") ? u.pathname.slice(1) : u.searchParams.get("v");
    if (id) return { platform, shape: "video", canonicalUrl: `https://www.youtube.com/watch?v=${id}`, title: `YouTube · ${id}`, warnings: [] };
    const list = u.searchParams.get("list");
    if (list) return { platform, shape: "collection", canonicalUrl: `https://www.youtube.com/playlist?list=${list}`, title: `YouTube · ${list}`, warnings: ["PLAYLIST_UNSUPPORTED"] };
  }
  const last = u.pathname.split("/").filter(Boolean).pop();
  const title = last ? `${name} · ${decodeURIComponent(last).slice(0, 80)}` : name;
  return { platform, shape: last ? "video" : "unknown", canonicalUrl: u.toString(), title, warnings: [] };
}
