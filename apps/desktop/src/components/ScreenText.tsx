import { useCallback, useEffect, useState } from "react";
import { errorCode } from "../ipc/errors";
import type { ItemScreenTextDto, JobDto, ModelClient, ScreenTextDto } from "../ipc/types";
import { formatDuration } from "../format";
import { t, tn } from "../i18n";
import { errorMessage } from "../labels";
import { ProgressBar } from "./DownloadsView";
import { languageName } from "./Transcripts";

type Notify = (text: string, tone?: "info" | "error") => void;

const openJob = (j: JobDto | null) => j !== null && !["succeeded", "failed", "canceled"].includes(j.status);

/**
 * Text on screen (STATE.md F7.4, SPEC AI-C01…C04, ADR-0039). Windows' own
 * OCR reads a few frames of each video (scene changes and every few
 * seconds, at most 300), in the user's Windows language; nothing is
 * downloaded or sent. "Read for all videos" goes through the videos not
 * read with their current file, one at a time; cancel keeps what was read.
 * Searches then also find words shown on screen, listed apart from speech.
 */
export function ScreenTextBlock({ client, notify }: { client: Pick<ModelClient, "getScreenText" | "readScreenText" | "cancelScreenText">; notify: Notify }) {
  const [view, setView] = useState<ScreenTextDto | null>(null);
  const [busy, setBusy] = useState(false);
  const fail = useCallback((e: unknown) => notify(errorCode(e) === "MODEL_MISSING" ? t("screen_text.unavailable") : errorMessage(errorCode(e)), "error"), [notify]);
  const reload = useCallback(() => client.getScreenText().then(setView, fail), [client, fail]);
  useEffect(() => {
    void reload();
  }, [reload]);

  const running = view !== null && openJob(view.job);
  useEffect(() => {
    if (!running) return;
    const id = setInterval(() => void reload(), 1000);
    return () => clearInterval(id);
  }, [running, reload]);

  const run = async (f: () => Promise<ScreenTextDto>) => {
    setBusy(true);
    try {
      setView(await f());
    } catch (e) {
      fail(e);
      await reload();
    } finally {
      setBusy(false);
    }
  };

  if (!view) return <p role="status">{t("ai.loading")}</p>;
  const job = view.job;
  return (
    <div className="search-index" data-testid="screen-text">
      <p className="muted small">{t("screen_text.intro")}</p>
      {view.available ? (
        <p className="small" data-testid="screen-text-ocr">
          {t("screen_text.reader", { language: languageName(view.language ?? "und") })}
        </p>
      ) : (
        <p className="warning small" data-testid="screen-text-unavailable">
          {t("screen_text.unavailable")}
        </p>
      )}
      <p className="small" data-testid="screen-text-summary">
        {tn("screen_text.summary", view.videos, { read: view.read })} {view.lines > 0 && tn("screen_text.lines", view.lines)}
      </p>
      {view.failed > 0 && <p className="muted small">{tn("screen_text.failed_videos", view.failed)}</p>}
      {running && job ? (
        <>
          <p className="muted small" data-testid="screen-text-progress">
            {job.status === "queued" ? t("screen_text.queued") : t("screen_text.reading", { done: job.unitsDone ?? 0, total: job.unitsTotal ?? 0 })}
          </p>
          <ProgressBar job={job} />
          <div className="actions">
            <button type="button" className="button" disabled={busy} onClick={() => void run(() => client.cancelScreenText())} data-testid="screen-text-cancel">
              {t("common.cancel")}
            </button>
          </div>
        </>
      ) : (
        <div className="actions">
          <button type="button" className="button" disabled={busy || !view.available || view.pending === 0} onClick={() => void run(() => client.readScreenText(null))} data-testid="screen-text-read">
            {view.pending > 0 ? tn("screen_text.read_all", view.pending) : t("screen_text.all_read")}
          </button>
        </div>
      )}
      {!running && job?.status === "failed" && (
        <p className="warning small" data-testid="screen-text-failed">
          {t("screen_text.failed", { reason: job.errorCode === "MODEL_MISSING" ? t("screen_text.unavailable") : errorMessage(job.errorCode ?? "INTERNAL_ERROR") })}
        </p>
      )}
      {!running && job?.status === "canceled" && <p className="muted small">{t("screen_text.canceled")}</p>}
    </div>
  );
}

const SHOWN_LINES = 12;

/**
 * Text on screen of one video, in Details (F7.4): read it now without
 * waiting for a run over the whole library, see the lines with their
 * times, and open one where it appeared.
 */
export function ItemScreenText({
  client,
  itemId,
  onOpen,
}: {
  client: Pick<ModelClient, "getItemScreenText" | "readScreenText">;
  itemId: string;
  onOpen?: (lineId: string) => void;
}) {
  const [view, setView] = useState<ItemScreenTextDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [all, setAll] = useState(false);
  const reload = useCallback(() => client.getItemScreenText(itemId).then(setView, () => undefined), [client, itemId]);
  useEffect(() => {
    setView(null);
    setError(null);
    setAll(false);
    void reload();
  }, [reload]);

  const running = view !== null && openJob(view.job);
  useEffect(() => {
    if (!running) return;
    const id = setInterval(() => void reload(), 1000);
    return () => clearInterval(id);
  }, [running, reload]);

  if (!view || !view.hasVideo) return null;
  const read = async () => {
    setBusy(true);
    setError(null);
    try {
      await client.readScreenText(itemId);
    } catch (e) {
      setError(errorCode(e) === "MODEL_MISSING" ? t("screen_text.unavailable") : errorMessage(errorCode(e)));
    } finally {
      await reload();
      setBusy(false);
    }
  };
  const job = view.job;
  const lines = all ? view.lines : view.lines.slice(0, SHOWN_LINES);
  return (
    <section data-testid="inspector-screen-text">
      <h4>{t("screen_text.title")}</h4>
      {!view.available && <p className="warning small">{t("screen_text.unavailable")}</p>}
      {running ? (
        <>
          <p className="muted small" data-testid="item-screen-text-progress">
            {job?.status === "queued" ? t("screen_text.item_queued") : t("screen_text.item_reading")}
          </p>
          {job && <ProgressBar job={job} />}
        </>
      ) : (
        <>
          {view.status === null && <p className="muted small">{t("screen_text.item_none")}</p>}
          {view.status === "failed" && <p className="warning small">{t("screen_text.item_failed", { reason: errorMessage(view.errorCode ?? "INTERNAL_ERROR") })}</p>}
          {view.status === "ready" && view.lines.length === 0 && (
            <p className="muted small" data-testid="item-screen-text-empty">
              {t("screen_text.item_empty")}
            </p>
          )}
          {view.status !== null && !view.current && <p className="muted small">{t("screen_text.item_changed")}</p>}
          {!running && job?.status === "failed" && job.errorCode !== view.errorCode && (
            <p className="warning small">{t("screen_text.item_failed", { reason: job.errorCode === "MODEL_MISSING" ? t("screen_text.unavailable") : errorMessage(job.errorCode ?? "INTERNAL_ERROR") })}</p>
          )}
          {error && <p className="warning small">{error}</p>}
          <div className="actions">
            <button type="button" className="button" disabled={busy || !view.available} onClick={() => void read()} data-testid="item-screen-text-read">
              {view.status === null ? t("screen_text.item_read") : t("screen_text.item_again")}
            </button>
          </div>
        </>
      )}
      {view.lines.length > 0 && (
        <>
          <p className="muted small">
            {languageName(view.language ?? "und")} · {tn("screen_text.lines", view.lines.length)}
          </p>
          <ol className="moments" data-testid="item-screen-text-lines">
            {lines.map((l) => {
              const body = (
                <>
                  <span className="timecode">{formatDuration(l.startMs)}</span> <span className="moment-text">{l.text}</span>
                </>
              );
              return (
                <li key={l.id} data-testid="item-screen-line" data-start-ms={l.startMs}>
                  {onOpen ? (
                    <button type="button" className="moment-button" onClick={() => onOpen(l.id)} aria-label={t("search.moment_label", { time: formatDuration(l.startMs), text: l.text })}>
                      {body}
                    </button>
                  ) : (
                    <span className="moment-static">{body}</span>
                  )}
                </li>
              );
            })}
          </ol>
          {view.lines.length > SHOWN_LINES && (
            <button type="button" className="link-button" onClick={() => setAll(!all)} data-testid="item-screen-text-more">
              {all ? t("screen_text.item_fewer") : tn("screen_text.item_more", view.lines.length)}
            </button>
          )}
        </>
      )}
    </section>
  );
}
