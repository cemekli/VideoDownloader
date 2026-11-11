// Fixture transcripts (STATE.md F6.2). Same shapes as the backend
// (`transcript_commands.rs`). Fetching captions runs a short timed job:
// YouTube items get the contract's Turkish manual captions, other linked
// items finish without captions (the site has none). "Transcribe on this
// PC" works for items with a local file (`local_ready`), with the Base
// model as if the user had installed it, and makes a `generated_local`
// transcript after a few progress steps.
import contract from "@fixtures/ipc-contract.json";
import { IpcError } from "./errors";
import type { FixtureRecord } from "./fixture";
import type { ItemTranscripts, JobDto, LocalTranscription, TranscriptClient, TranscriptDetail, TranscriptDto } from "./types";

/** Transcripts plus the cancel the fixture client routes here for transcription jobs. */
export type FixtureTranscripts = TranscriptClient & { cancelOwnJob(jobId: string): JobDto | null };

const FIXTURE_MODELS = [{ id: "whisper-base", label: "Base" }];

const err = (code: string, cmd: string) =>
  new IpcError(code, `${cmd}: error.${code.toLowerCase()}`, { code, messageKey: `error.${code.toLowerCase()}`, retryable: false, suggestedAction: null, diagnosticId: cmd, safeDetails: {} });

export function createFixtureTranscripts(records: FixtureRecord[], stepMs = 400): FixtureTranscripts {
  const sample = contract.commands.get_transcript.response as TranscriptDetail;
  const byItem = new Map<string, TranscriptDto[]>();
  const segments = new Map<string, TranscriptDetail["segments"]>();
  const jobs = new Map<string, JobDto>();
  const asr = new Map<string, JobDto>();
  const asrModel = new Map<string, string>();
  let clock = 1_794_100_000_000;
  let seq = 0;
  const now = () => (clock += 1_000);

  const item = (id: string, cmd: string) => records.find((r) => r.id === id && !r.removed) ?? (() => { throw err("INPUT_REJECTED", cmd); })();
  const view = (id: string): ItemTranscripts => {
    const r = item(id, "list_transcripts");
    const hasMedia = r.localState === "local_ready";
    const local: LocalTranscription = { ready: hasMedia, reason: hasMedia ? null : "no-media", models: FIXTURE_MODELS, languages: ["auto", "tr", "en"] };
    return structuredClone({ itemId: id, transcripts: byItem.get(id) ?? [], captionsJob: jobs.get(id) ?? null, canFetchCaptions: r.canonicalUrl !== null, transcribeJob: asr.get(id) ?? null, transcribeModel: asrModel.get(id) ?? null, local });
  };
  const isOpen = (j: JobDto | undefined) => j !== undefined && !["succeeded", "failed", "canceled"].includes(j.status);
  const step = (r: FixtureRecord, modelId: string, language: string, pct: number) => {
    const j = asr.get(r.id);
    if (!j || j.status !== "running") return;
    j.updatedAt = now();
    if (pct < 100) {
      j.phase = "transcribe";
      j.unitsDone = pct;
      j.unitsTotal = 100;
      setTimeout(() => step(r, modelId, language, pct + 50), stepMs);
      return;
    }
    j.status = "succeeded";
    j.phase = "transcribe_save";
    j.unitsDone = 100;
    j.finishedAt = j.updatedAt;
    j.canCancel = false;
    const lang = language === "auto" ? "tr" : language;
    const list = byItem.get(r.id) ?? [];
    const existing = list.find((t) => t.provenance === "generated_local" && t.language === lang && t.modelId === modelId);
    const t: TranscriptDto = existing ?? {
      ...sample.transcript,
      id: `00000000-0000-4000-8000-${String(0xc200 + ++seq).padStart(12, "0")}`,
      itemId: r.id,
      language: lang,
      provenance: "generated_local",
      provider: "whisper_cpp",
      captionKind: null,
      modelId,
      createdAt: j.finishedAt,
    };
    t.updatedAt = j.finishedAt;
    if (!existing) byItem.set(r.id, [...list, t]);
    segments.set(t.id, sample.segments);
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
    async transcribeLocally(itemId, modelId, language) {
      const r = item(itemId, "transcribe_locally");
      if (!["auto", "tr", "en"].includes(language)) throw err("INPUT_REJECTED", "transcribe_locally");
      if (r.localState !== "local_ready") throw err("FILE_MISSING", "transcribe_locally");
      if (!FIXTURE_MODELS.some((m) => m.id === modelId)) throw err("MODEL_MISSING", "transcribe_locally");
      if (isOpen(asr.get(itemId))) return view(itemId);
      const t = now();
      asrModel.set(itemId, FIXTURE_MODELS.find((m) => m.id === modelId)?.label ?? modelId);
      asr.set(itemId, {
        ...(contract.commands.start_download.response as JobDto),
        id: `00000000-0000-4000-8000-${String(0xc300 + ++seq).padStart(12, "0")}`,
        kind: "transcription",
        itemId,
        itemTitle: r.title,
        itemPlatform: r.platform,
        status: "running",
        phase: "transcribe_audio",
        preset: null,
        attempts: 1,
        maxAttempts: 1,
        createdAt: t,
        updatedAt: t,
        finishedAt: null,
        canCancel: true,
      });
      setTimeout(() => step(r, modelId, language, 0), stepMs);
      return view(itemId);
    },
    cancelOwnJob(jobId) {
      for (const j of asr.values()) {
        if (j.id !== jobId) continue;
        if (isOpen(j)) Object.assign(j, { status: "canceled", phase: null, canCancel: false, finishedAt: now() });
        return structuredClone(j);
      }
      return null;
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
