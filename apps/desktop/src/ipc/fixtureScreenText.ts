// Fixture text on screen (STATE.md F7.4, ADR-0039). Same shapes as the
// backend (`screen_text_commands.rs`). "Read for all videos" goes through
// the videos in timed steps; each read video shows its title in capitals
// from 0:04 to 0:09, so a library search finds it "on screen" apart from
// speech. The `no-ocr` scenario has no OCR language (MODEL_MISSING).
import contract from "@fixtures/ipc-contract.json";
import { IpcError } from "./errors";
import { highlight, terms, type FixtureRecord, type FixtureScenario } from "./fixture";
import type { JobDto, ModelClient, MomentTarget, ScreenTextDto, TranscriptMatchesDto } from "./types";

type ScreenClient = Pick<ModelClient, "getScreenText" | "getItemScreenText" | "readScreenText" | "cancelScreenText">;

export type FixtureScreenText = ScreenClient & {
  /** Where the search terms were read on screen in an item. */
  screenMatches(itemId: string, ts: string[]): TranscriptMatchesDto | null;
  /** A search moment of an on-screen line, or null when it is not one. */
  screenMoment(lineId: string): MomentTarget | null;
};

interface Line {
  id: string;
  startMs: number;
  endMs: number;
  text: string;
}

const err = (code: string, cmd: string) =>
  new IpcError(code, `${cmd}: error.${code.toLowerCase()}`, { code, messageKey: `error.${code.toLowerCase()}`, retryable: false, suggestedAction: null, diagnosticId: cmd, safeDetails: {} });

const isOpen = (j: JobDto | null) => j !== null && !["succeeded", "failed", "canceled"].includes(j.status);

export function createFixtureScreenText(records: FixtureRecord[], scenario: FixtureScenario = "default", stepMs = 300): FixtureScreenText {
  const available = scenario !== "no-ocr";
  const read = new Map<string, Line[]>();
  const sample = contract.commands.get_screen_text.response.job as JobDto;
  let job: JobDto | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let queue: string[] = [];
  let clock = 1_794_000_000_000;
  const now = () => (clock += 1000);

  const videos = () => records.filter((r) => !r.removed && r.fileName !== null);
  const pending = () => videos().filter((r) => !read.has(r.id));
  const view = (): ScreenTextDto =>
    structuredClone({
      available,
      language: available ? "tr" : null,
      videos: videos().length,
      read: videos().filter((r) => read.has(r.id)).length,
      failed: 0,
      pending: pending().length,
      lines: [...read.values()].reduce((n, l) => n + l.length, 0),
      job,
    });
  const readOne = (id: string) => {
    const r = records.find((x) => x.id === id);
    if (!r) return;
    read.set(id, [{ id: `00000000-0000-4000-8000-${String(0xf100 + read.size).padStart(12, "0")}`, startMs: 4000, endMs: 9000, text: r.title.toLocaleUpperCase("tr") }]);
  };
  const step = () => {
    if (!job || job.status !== "running") return;
    const id = queue.shift();
    if (id) readOne(id);
    job.unitsDone = (job.unitsTotal ?? 0) - queue.length;
    job.updatedAt = now();
    if (queue.length === 0) {
      job.status = "succeeded";
      job.finishedAt = job.updatedAt;
      job.canCancel = false;
      return;
    }
    timer = setTimeout(step, stepMs);
  };

  return {
    async getScreenText() {
      return view();
    },
    async getItemScreenText(itemId) {
      const r = records.find((x) => x.id === itemId);
      const lines = read.get(itemId);
      return structuredClone({
        available,
        language: available ? "tr" : null,
        hasVideo: !!r && !r.removed && r.fileName !== null,
        status: lines ? ("ready" as const) : null,
        current: !!lines,
        errorCode: null,
        lines: lines ?? [],
        job: null,
      });
    },
    async readScreenText(itemId) {
      if (!available) throw err("MODEL_MISSING", "read_screen_text");
      if (itemId !== null) {
        const r = records.find((x) => x.id === itemId);
        if (!r || r.fileName === null) throw err("FILE_MISSING", "read_screen_text");
        readOne(itemId);
        return view();
      }
      if (isOpen(job)) return view();
      queue = pending().map((r) => r.id);
      const t = now();
      job = { ...sample, id: `00000000-0000-4000-8000-${String(0xf000 + (clock % 1000)).padStart(12, "0")}`, status: "running", unitsDone: 0, unitsTotal: queue.length, createdAt: t, updatedAt: t, finishedAt: null, canCancel: true };
      timer = setTimeout(step, stepMs);
      return view();
    },
    async cancelScreenText() {
      clearTimeout(timer);
      if (job && isOpen(job)) {
        job.status = "canceled";
        job.canCancel = false;
        job.finishedAt = now();
      }
      return view();
    },
    screenMatches(itemId, ts) {
      const lines = (read.get(itemId) ?? []).filter((l) => {
        const words = terms(l.text);
        return ts.every((q, i) => words.some((w) => (i === ts.length - 1 ? w.startsWith(q) : w === q || w.startsWith(q))));
      });
      const hits = lines.flatMap((l) => {
        const s = highlight("on_screen", l.text, ts);
        return s ? [{ segmentId: l.id, startMs: l.startMs, endMs: l.endMs, snippet: s }] : [];
      });
      return hits.length ? { transcriptId: "", language: "tr", provenance: "ocr", partial: false, coveredMs: null, moments: hits.slice(0, 5), total: hits.length } : null;
    },
    screenMoment(lineId) {
      for (const [itemId, lines] of read) {
        const l = lines.find((x) => x.id === lineId);
        if (l) return { itemId, transcriptId: "", segmentId: l.id, startMs: l.startMs, endMs: l.endMs };
      }
      return null;
    },
  };
}
