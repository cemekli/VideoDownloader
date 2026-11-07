// Playback probe for the DEC-04 matrix (ADR-0004). Kept apart from the spec so
// the same probe can be pointed at any page, e.g. a plain Chromium for a dry run.

import type { Page } from "@playwright/test";

export interface MediaEntry {
  id: string;
  file: string;
  container: string;
  video: string;
  audio: string;
  mime: string;
  durationS: number;
}

export interface Probe {
  canPlayType: string;
  canPlayContainer: string;
  loaded: boolean;
  error: string | null;
  durationS: number | null;
  width: number;
  height: number;
  played: boolean;
  framesDecoded: number;
  audioBytesDecoded: number;
  seeked: boolean;
  seekLandedS: number | null;
  playsAfterSeek: boolean;
  firstFrameMs: number | null;
}

export const BASELINE = "mp4-h264-aac";
export const SEEK_TO_S = 2;

/** Plays one file in a fresh <video> element; runs inside the webview. */
export async function probe(page: Page, url: string, mime: string): Promise<Probe> {
  return page.evaluate(
    async ({ url, mime, seekTo }) => {
      const v = document.createElement("video");
      v.muted = true;
      v.preload = "auto";
      v.width = 320;
      document.body.appendChild(v);
      const t0 = performance.now();
      const until = (pred: () => boolean, ms: number) =>
        new Promise<boolean>((done) => {
          const end = performance.now() + ms;
          const tick = () => (pred() ? done(true) : performance.now() > end || v.error ? done(false) : setTimeout(tick, 50));
          tick();
        });
      const once = (ok: string, ms: number) =>
        new Promise<boolean>((done) => {
          const timer = setTimeout(() => done(false), ms);
          v.addEventListener(ok, () => (clearTimeout(timer), done(true)), { once: true });
          v.addEventListener("error", () => (clearTimeout(timer), done(false)), { once: true });
        });
      const result = {
        canPlayType: v.canPlayType(mime),
        canPlayContainer: v.canPlayType(mime.split(";")[0] ?? mime),
        loaded: false,
        error: null as string | null,
        durationS: null as number | null,
        width: 0,
        height: 0,
        played: false,
        framesDecoded: 0,
        audioBytesDecoded: 0,
        seeked: false,
        seekLandedS: null as number | null,
        playsAfterSeek: false,
        firstFrameMs: null as number | null,
      };
      // Names a CSP block directly instead of a bare media error.
      let cspBlock: string | null = null;
      const onCsp = (e: SecurityPolicyViolationEvent) => {
        if (e.blockedURI.startsWith(url.split("/").slice(0, 3).join("/"))) cspBlock = `blocked by CSP ${e.effectiveDirective}`;
      };
      document.addEventListener("securitypolicyviolation", onCsp);
      try {
        const loaded = once("loadeddata", 10_000);
        v.src = url;
        result.loaded = await loaded;
        if (!result.loaded) {
          result.error = cspBlock ?? (v.error ? `code ${v.error.code}: ${v.error.message}` : "timeout");
          return result;
        }
        result.firstFrameMs = Math.round(performance.now() - t0);
        result.durationS = Number.isFinite(v.duration) ? Number(v.duration.toFixed(3)) : null;
        result.width = v.videoWidth;
        result.height = v.videoHeight;
        await v.play().catch((e: unknown) => (result.error = `play(): ${String(e)}`));
        result.played = await until(() => v.currentTime > 0.5, 6_000);
        const seeked = once("seeked", 6_000);
        v.currentTime = seekTo;
        result.seeked = await seeked;
        result.seekLandedS = Number(v.currentTime.toFixed(3));
        result.playsAfterSeek = await until(() => v.currentTime > seekTo + 0.3, 6_000);
        result.framesDecoded = v.getVideoPlaybackQuality().totalVideoFrames;
        result.audioBytesDecoded = (v as unknown as { webkitAudioDecodedByteCount?: number }).webkitAudioDecodedByteCount ?? -1;
        if (v.error) result.error = `code ${v.error.code}: ${v.error.message}`;
        return result;
      } finally {
        document.removeEventListener("securitypolicyviolation", onCsp);
        v.pause();
        v.removeAttribute("src");
        v.load();
        v.remove();
      }
    },
    { url, mime, seekTo: SEEK_TO_S },
  );
}

export function verdict(p: Probe): "plays" | "partial" | "fails" {
  if (!p.loaded) return "fails";
  const seekOk = p.seeked && p.seekLandedS !== null && Math.abs(p.seekLandedS - SEEK_TO_S) < 0.5;
  const ok = p.played && p.framesDecoded > 0 && p.width > 0 && p.audioBytesDecoded !== 0 && seekOk && p.playsAfterSeek && !p.error;
  return ok ? "plays" : "partial";
}

