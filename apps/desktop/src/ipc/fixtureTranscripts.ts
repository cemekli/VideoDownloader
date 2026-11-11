// Fixture transcripts (STATE.md F6.2). Same shapes as the backend
// (`transcript_commands.rs`). Fetching captions runs a short timed job:
// YouTube items get the contract's Turkish manual captions, other linked
// items finish without captions (the site has none).
import contract from "@fixtures/ipc-contract.json";
import { IpcError } from "./errors";
import type { FixtureRecord } from "./fixture";
import type { ItemTranscripts, JobDto, TranscriptClient, TranscriptDetail, TranscriptDto } from "./types";

const err = (code: string, cmd: string) =>
  new IpcError(code, `${cmd}: error.${code.toLowerCase()}`, { code, messageKey: `error.${code.toLowerCase()}`, retryable: false, suggestedAction: null, diagnosticId: cmd, safeDetails: {} });

export function createFixtureTranscripts(records: FixtureRecord[], stepMs = 400): TranscriptClient {
  const sample = contract.commands.get_transcript.response as TranscriptDetail;
  const byItem = new Map<string, TranscriptDto[]>();
  const segments = new Map<string, TranscriptDetail["segments"]>();
  const jobs = new Map<string, JobDto>();
  let clock = 1_794_100_000_000;
  let seq = 0;
  const now = () => (clock += 1_000);

  const item = (id: string, cmd: string) => records.find((r) => r.id === id && !r.removed) ?? (() => { throw err("INPUT_REJECTED", cmd); })();
  const view = (id: string): ItemTranscripts => {
    const r = item(id, "list_transcripts");
    return structuredClone({ itemId: id, transcripts: byItem.get(id) ?? [], captionsJob: jobs.get(id) ?? null, canFetchCaptions: r.canonicalUrl !== null });
  };
  const finish = (r: FixtureRecord) => {
    const j = jobs.get(r.id);
    if (!j || j.status !== "running") return;
    j.status = "succeeded";
    j.phase = "captions_save";
    j.finishedAt = j.updatedAt = now();
    j.canCancel = false;
    if (r.platform !== "youtube") return;
    const existing = byItem.get(r.id)?.find((t) => t.provenance === "source_captions" && t.language === "tr");
    const t: TranscriptDto = existing ?? { ...sample.transcript, id: `00000000-0000-4000-8000-${String(0xc100 + ++seq).padStart(12, "0")}`, itemId: r.id };
    t.createdAt = existing?.createdAt ?? j.finishedAt;
    t.updatedAt = j.finishedAt;
    if (!existing) byItem.set(r.id, [...(byItem.get(r.id) ?? []), t]);
    segments.set(t.id, sample.segments);
  };

  return {
    async listTranscripts(itemId) {
      return view(itemId);
    },
    async getTranscript(transcriptId) {
      for (const list of byItem.values()) {
        const t = list.find((x) => x.id === transcriptId);
        if (t) return structuredClone({ transcript: t, segments: segments.get(t.id) ?? [], truncated: false });
      }
      throw err("INPUT_REJECTED", "get_transcript");
    },
    async fetchCaptions(itemId) {
      const r = item(itemId, "fetch_captions");
      if (r.canonicalUrl === null) throw err("INPUT_REJECTED", "fetch_captions");
      const open = jobs.get(itemId);
      if (open && !["succeeded", "failed", "canceled"].includes(open.status)) return view(itemId);
      const t = now();
      jobs.set(itemId, {
        ...(contract.commands.start_download.response as JobDto),
        id: `00000000-0000-4000-8000-${String(0xc000 + ++seq).padStart(12, "0")}`,
        kind: "fetch_captions",
        itemId,
        itemTitle: r.title,
        itemPlatform: r.platform,
        status: "running",
        phase: "captions_list",
        preset: null,
        attempts: 1,
        createdAt: t,
        updatedAt: t,
        finishedAt: null,
        canCancel: true,
      });
      setTimeout(() => finish(r), stepMs);
      return view(itemId);
    },
    async deleteTranscript(transcriptId) {
      for (const [itemId, list] of byItem) {
        if (list.some((t) => t.id === transcriptId)) {
          byItem.set(itemId, list.filter((t) => t.id !== transcriptId));
          segments.delete(transcriptId);
          return view(itemId);
        }
      }
      throw err("INPUT_REJECTED", "delete_transcript");
    },
  };
}
