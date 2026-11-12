import { useCallback, useEffect, useState } from "react";
import { errorCode } from "../ipc/errors";
import type { DownloadClient, ItemTranscripts, JobDto, LocalTranscription, ModelChoice, TranscribeLanguage, TranscriptClient, TranscriptDetail, TranscriptDto } from "../ipc/types";
import { formatBytes, formatDuration } from "../format";
import { formatDateTime, getLocale, t, tn, type MessageKey } from "../i18n";
import { errorMessage } from "../labels";
import { useOnline } from "../hooks/useEnv";
import { blockedText } from "./DownloadsView";

const openJob = (j: JobDto | null) => j !== null && !["succeeded", "failed", "canceled"].includes(j.status);

/** `tr` -> "Turkish" in the UI language; the tag itself when unknown. */
export function languageName(tag: string): string {
  try {
    const base = tag.replace(/-orig$/, "");
    return new Intl.DisplayNames([getLocale()], { type: "language" }).of(base) ?? tag;
  } catch {
    return tag;
  }
}

/** Where the text came from, in words (AI-A01: source and language shown). */
export function sourceLabel(tr: TranscriptDto): string {
  if (tr.provenance === "source_captions") return tr.captionKind === "automatic" ? t("transcripts.source.automatic") : t("transcripts.source.manual");
  if (tr.provenance === "generated_local") return t("transcripts.source.local", { model: modelName(tr.modelId) });
  return t("transcripts.source.imported");
}

/** `whisper-medium` -> "Medium": the catalog's label for its models. */
export function modelName(id: string | null): string {
  if (!id) return "?";
  const base = id.replace(/^whisper-/, "");
  return base.charAt(0).toUpperCase() + base.slice(1);
}

/** AI-A06: which model and version made a local transcript. */
export function modelVersionText(tr: TranscriptDto): string | null {
  if (tr.provenance !== "generated_local") return null;
  const rev = tr.modelVersion ? tr.modelVersion.slice(0, 7) : null;
  return rev ? t("transcripts.model_version", { model: modelName(tr.modelId), rev }) : t("transcripts.model_only", { model: modelName(tr.modelId) });
}

/** AI-A06: the processing state; a partial one says how far it goes (F6.3, AC-23). */
function statusText(tr: TranscriptDto): string {
  if (tr.status === "partial") return ` · ${t("transcripts.partial", { covered: formatDuration(tr.coveredMs) })}`;
  return ` · ${t(`transcripts.status.${tr.status}` as MessageKey)}`;
}

/** The model a new run starts with: the largest that fits this PC now (AI-A05), else the smallest. */
export function defaultModel(models: ModelChoice[]): ModelChoice | null {
  const fitting = models.filter((m) => m.fit === "ok" || m.fit === "unknown");
  return fitting[fitting.length - 1] ?? models[0] ?? null;
}

/** The memory warning for `m` on this PC, or null when it fits (AI-A05). */
export function memoryWarning(m: ModelChoice, local: LocalTranscription): string | null {
  const need = formatBytes(m.needMemoryMb * 1024 * 1024);
  if (m.fit === "too_big") return t("transcripts.local.too_big", { model: m.label, need, total: formatBytes(local.memory?.totalBytes ?? 0) });
  if (m.fit === "tight") return t("transcripts.local.tight", { model: m.label, need, free: formatBytes(local.memory?.availableBytes ?? 0) });
  return null;
}

/** What the open captions job is doing right now, in words. */
export function captionsJobText(j: JobDto): string {
  if (j.status === "queued") return t("transcripts.job.queued");
  if (j.status === "retry_wait") {
    const reason = errorMessage(j.errorCode ?? "INTERNAL_ERROR");
    return j.nextRetryAt ? t("transcripts.job.retry_at", { reason, time: formatDateTime(j.nextRetryAt) }) : t("transcripts.job.retry", { reason });
  }
  if (j.status === "blocked") return blockedText(j);
  if (j.status === "canceling") return t("transcripts.job.canceling");
  return j.attempts > 1 ? t("transcripts.job.running_attempt", { attempt: j.attempts, max: j.maxAttempts }) : t("transcripts.fetching");
}

/** "2:10 elapsed, about 6:30 left": the time left is a straight-line
 *  guess from whisper's percentage, shown once it reaches 5%. */
function timing(j: JobDto, elapsedMs: number | null): string | null {
  if (elapsedMs === null) return null;
  const done = t("transcripts.local.elapsed", { elapsed: formatDuration(elapsedMs) });
  const pct = j.phase === "transcribe" ? j.unitsDone : null;
  if (pct === null || pct < 5 || pct >= 100) return done;
  return t("transcripts.local.elapsed_left", { elapsed: formatDuration(elapsedMs), left: formatDuration((elapsedMs * (100 - pct)) / pct) });
}

/**
 * The item's transcripts in the details panel (STATE.md F6.2, SPEC §8.4,
 * AI-A01, AI-A02). The site's own captions come first; an item with a file
 * can also be transcribed on this PC with a model the user installed. Each
 * transcript says its language and source. Lines are shown with their
 * timecodes as plain text.
 */
export function Transcripts({ client, itemId }: { client: TranscriptClient & Partial<Pick<DownloadClient, "cancelJob">>; itemId: string }) {
  const online = useOnline();
  const [view, setView] = useState<ItemTranscripts | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [shown, setShown] = useState<TranscriptDetail | null>(null);
  const [language, setLanguage] = useState<TranscribeLanguage>("auto");
  const [modelId, setModelId] = useState<string | null>(null);
  // "Use it anyway" for a model that does not fit; asked again per model.
  const [anyway, setAnyway] = useState<string | null>(null);

  const fail = useCallback((e: unknown) => setError(errorMessage(errorCode(e))), []);
  const reload = useCallback(
    () =>
      client.listTranscripts(itemId).then((v) => {
        setView(v);
        return v;
      }, fail),
    [client, itemId, fail],
  );

  useEffect(() => {
    setView(null);
    setShown(null);
    setError(null);
    void reload();
  }, [reload]);

  // While a captions or transcription job is open, refresh once a second.
  const job = view?.captionsJob ?? null;
  const active = openJob(job);
  const asr = view?.transcribeJob ?? null;
  const asrModel = view?.transcribeModel ?? "?";
  const asrActive = openJob(asr);
  useEffect(() => {
    if (!active && !asrActive) return;
    const id = setInterval(() => void reload(), 1000);
    return () => clearInterval(id);
  }, [active, asrActive, reload]);

  // A transcript that is open is reloaded when it changes.
  const shownId = shown?.transcript.id;
  const shownAt = view?.transcripts.find((x) => x.id === shownId)?.updatedAt;
  useEffect(() => {
    if (!shownId || shownAt === undefined || shownAt === shown?.transcript.updatedAt) return;
    client.getTranscript(shownId).then(setShown, fail);
  }, [client, shownId, shownAt, shown?.transcript.updatedAt, fail]);

  const act = async (f: () => Promise<ItemTranscripts>) => {
    setBusy(true);
    setError(null);
    try {
      setView(await f());
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };

  const toggle = async (tr: TranscriptDto) => {
    if (shown?.transcript.id === tr.id) return setShown(null);
    try {
      setShown(await client.getTranscript(tr.id));
    } catch (e) {
      fail(e);
    }
  };

  if (!view) return null;
  const hasCaptions = view.transcripts.some((x) => x.provenance === "source_captions");
  const noneFound = job?.status === "succeeded" && !hasCaptions;
  const local = view.local;
  // Default: the largest installed model that fits this PC (smallest first).
  const model = local.models.find((m) => m.id === modelId) ?? defaultModel(local.models);
  const memWarn = model ? memoryWarning(model, local) : null;
  const needsAnyway = memWarn !== null;
  const confirmed = !needsAnyway || anyway === model?.id;
  const noRoom = local.diskNeedBytes !== null && local.freeBytes !== null && local.freeBytes < local.diskNeedBytes;
  const noSpeech = asr?.status === "succeeded" && !view.transcripts.some((x) => x.provenance === "generated_local");
  const asrTiming = asrActive && asr ? timing(asr, view.transcribeElapsedMs) : null;
  const asrStopped = asr !== null && (asr.status === "failed" || asr.status === "blocked");
  return (
    <section data-testid="inspector-transcripts">
      <h4>{t("transcripts.title")}</h4>
      {view.transcripts.length === 0 && !active && !asrActive && !noneFound && <p className="muted small">{t(local.reason === "no-media" ? "transcripts.empty" : "transcripts.empty_local")}</p>}
      {view.transcripts.length > 0 && (
        <ul className="transcript-list">
          {view.transcripts.map((tr) => (
            <li key={tr.id} data-testid="transcript-row">
              <div>
                <strong>{languageName(tr.language)}</strong> <span className="muted small">· {sourceLabel(tr)}</span>
              </div>
              <div className="muted small" data-testid="transcript-status" data-status={tr.status}>
                {tn("transcripts.lines", tr.segmentCount)}
                {statusText(tr)}
              </div>
              {modelVersionText(tr) && (
                <div className="muted small" data-testid="transcript-model">
                  {modelVersionText(tr)}
                </div>
              )}
              <div className="row-actions">
                <button type="button" className="link-button" aria-expanded={shown?.transcript.id === tr.id} onClick={() => void toggle(tr)} data-testid="transcript-show">
                  {shown?.transcript.id === tr.id ? t("transcripts.hide") : t("transcripts.show")}
                </button>
                <button
                  type="button"
                  className="link-button"
                  disabled={busy}
                  onClick={() => {
                    if (shown?.transcript.id === tr.id) setShown(null);
                    void act(() => client.deleteTranscript(tr.id));
                  }}
                  data-testid="transcript-delete"
                >
                  {t("transcripts.delete")}
                </button>
              </div>
              {shown?.transcript.id === tr.id && (
                <ol className="transcript-lines" aria-label={t("transcripts.lines_label", { language: languageName(tr.language) })} data-testid="transcript-lines">
                  {shown.segments.map((s, i) => (
                    <li key={i}>
                      <span className="timecode">{formatDuration(s.startMs)}</span>
                      <span className="transcript-text">{s.text}</span>
                    </li>
                  ))}
                  {shown.truncated && <li className="muted small">{t("transcripts.truncated")}</li>}
                </ol>
              )}
            </li>
          ))}
        </ul>
      )}
      {active && job && (
        <div className="row-actions">
          <p role="status" className="muted small" data-testid="captions-running" data-status={job.status}>
            {captionsJobText(job)}
          </p>
          {client.cancelJob && job.canCancel && (
            <button
              type="button"
              className="link-button"
              disabled={busy}
              onClick={() =>
                void act(async () => {
                  await client.cancelJob!(job.id);
                  return client.listTranscripts(itemId);
                })
              }
              data-testid="captions-cancel"
            >
              {t("transcripts.cancel")}
            </button>
          )}
        </div>
      )}
      {noneFound && !active && (
        <p className="muted small" data-testid="captions-none">
          {t("transcripts.none_found")}
        </p>
      )}
      {job?.status === "canceled" && !active && !hasCaptions && (
        <p className="muted small" data-testid="captions-canceled">
          {t("transcripts.canceled")}
        </p>
      )}
      {job?.status === "failed" && !active && (
        <p role="alert" className="small" data-testid="captions-failed">
          {t("transcripts.failed", { reason: errorMessage(job.errorCode ?? "INTERNAL_ERROR") })}
        </p>
      )}
      {error && (
        <p role="alert" className="small">
          {error}
        </p>
      )}
      {view.canFetchCaptions && !active && (
        <button
          type="button"
          className="button"
          disabled={busy || !online}
          title={online ? undefined : t("transcripts.offline")}
          onClick={() => void act(() => client.fetchCaptions(itemId))}
          data-testid="captions-fetch"
        >
          {hasCaptions || job ? t("transcripts.fetch_again") : t("transcripts.fetch")}
        </button>
      )}

      <div className="transcribe-local" data-testid="transcribe-local">
        {asrActive && asr && (
          <p role="status" className="muted small" data-testid="transcribe-running">
            {asr.phase === "transcribe" && asr.unitsDone !== null
              ? t("transcripts.local.running_pct", { pct: asr.unitsDone, model: asrModel })
              : asr.status === "queued"
                ? t("transcripts.local.queued", { model: asrModel })
                : t("transcripts.local.preparing", { model: asrModel })}
            {asrTiming && <span data-testid="transcribe-timing"> · {asrTiming}</span>}{" "}
            {asr.canCancel && client.cancelJob && (
              <button
                type="button"
                className="link-button"
                disabled={busy}
                onClick={() => void act(async () => (await client.cancelJob?.(asr.id), client.listTranscripts(itemId)))}
                data-testid="transcribe-cancel"
              >
                {t("transcripts.local.cancel")}
              </button>
            )}
          </p>
        )}
        {asr?.status === "succeeded" && !asrActive && view.transcribeElapsedMs !== null && (
          <p className="muted small" data-testid="transcribe-last-run">
            {t("transcripts.local.last_run", { model: asrModel, elapsed: formatDuration(view.transcribeElapsedMs) })}
          </p>
        )}
        {noSpeech && !asrActive && (
          <p className="muted small" data-testid="transcribe-no-speech">
            {t("transcripts.local.no_speech")}
          </p>
        )}
        {asrStopped && !asrActive && asr && (
          <p role="alert" className="small" data-testid="transcribe-failed">
            {t("transcripts.local.failed", { reason: errorMessage(asr.errorCode ?? "INTERNAL_ERROR") })}
          </p>
        )}
        {!asrActive && local.reason === "no-model" && (
          <p className="muted small" data-testid="transcribe-hint">
            {t("transcripts.local.no_model")}
          </p>
        )}
        {!asrActive && local.reason === "no-tool" && (
          <p className="muted small" data-testid="transcribe-hint">
            {t("transcripts.local.no_tool")}
          </p>
        )}
        {!asrActive && local.ready && model && (
          <div className="transcribe-controls">
            <label className="small">
              {t("transcripts.local.language")}{" "}
              <select value={language} onChange={(e) => setLanguage(e.target.value as TranscribeLanguage)} data-testid="transcribe-language">
                {local.languages.map((l) => (
                  <option key={l} value={l}>
                    {l === "auto" ? t("transcripts.local.auto") : languageName(l)}
                  </option>
                ))}
              </select>
            </label>
            {local.models.length > 1 && (
              <label className="small">
                {t("transcripts.local.model")}{" "}
                <select value={model.id} onChange={(e) => setModelId(e.target.value)} data-testid="transcribe-model">
                  {local.models.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.fit === "too_big" || m.fit === "tight" ? t("transcripts.local.model_needs_memory", { model: m.label }) : m.label}
                    </option>
                  ))}
                </select>
              </label>
            )}
            <button
              type="button"
              className="button"
              disabled={busy || !confirmed || noRoom}
              title={t("transcripts.local.hint", { model: model.label })}
              onClick={() => void act(() => client.transcribeLocally(itemId, model.id, language, needsAnyway && confirmed))}
              data-testid="transcribe-start"
            >
              {t("transcripts.local.start")}
            </button>
          </div>
        )}
        {!asrActive && local.ready && model && memWarn && (
          <div className="warning small" role="alert" data-testid="transcribe-memory" data-fit={model.fit}>
            <p>{memWarn}</p>
            <label>
              <input type="checkbox" checked={anyway === model.id} onChange={(e) => setAnyway(e.target.checked ? model.id : null)} data-testid="transcribe-anyway" /> {t("transcripts.local.anyway", { model: model.label })}
            </label>
          </div>
        )}
        {!asrActive && local.ready && model && noRoom && (
          <p className="warning small" role="alert" data-testid="transcribe-disk">
            {t("transcripts.local.no_room", { need: formatBytes(local.diskNeedBytes ?? 0), free: formatBytes(local.freeBytes ?? 0) })}
          </p>
        )}
      </div>
    </section>
  );
}
