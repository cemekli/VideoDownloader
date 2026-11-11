import { useCallback, useEffect, useState } from "react";
import { errorCode } from "../ipc/errors";
import type { DownloadClient, ItemTranscripts, JobDto, TranscriptClient, TranscriptDetail, TranscriptDto } from "../ipc/types";
import { formatDuration } from "../format";
import { formatDateTime, getLocale, t, tn } from "../i18n";
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
  if (tr.provenance === "generated_local") return t("transcripts.source.local", { model: tr.modelId ?? "?" });
  return t("transcripts.source.imported");
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

/**
 * The item's transcripts in the details panel (STATE.md F6.2, SPEC §8.4,
 * AI-A01). The site's own captions come first; each transcript says its
 * language and source. Lines are shown with their timecodes as plain text.
 */
export function Transcripts({ client, itemId }: { client: TranscriptClient & Partial<Pick<DownloadClient, "cancelJob">>; itemId: string }) {
  const online = useOnline();
  const [view, setView] = useState<ItemTranscripts | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [shown, setShown] = useState<TranscriptDetail | null>(null);

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

  // While the captions job is open, refresh once a second.
  const job = view?.captionsJob ?? null;
  const active = openJob(job);
  useEffect(() => {
    if (!active) return;
    const id = setInterval(() => void reload(), 1000);
    return () => clearInterval(id);
  }, [active, reload]);

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
  return (
    <section data-testid="inspector-transcripts">
      <h4>{t("transcripts.title")}</h4>
      {view.transcripts.length === 0 && !active && !noneFound && <p className="muted small">{t("transcripts.empty")}</p>}
      {view.transcripts.length > 0 && (
        <ul className="transcript-list">
          {view.transcripts.map((tr) => (
            <li key={tr.id} data-testid="transcript-row">
              <div>
                <strong>{languageName(tr.language)}</strong> <span className="muted small">· {sourceLabel(tr)}</span>
              </div>
              <div className="muted small">{tn("transcripts.lines", tr.segmentCount)}{tr.status !== "ready" ? ` · ${tr.status}` : ""}</div>
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
    </section>
  );
}
