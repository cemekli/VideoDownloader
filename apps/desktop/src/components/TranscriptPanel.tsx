import { useEffect, useMemo, useRef, useState } from "react";
import { errorCode } from "../ipc/errors";
import type { NotePreview, TranscriptClient, TranscriptDetail, TranscriptDto } from "../ipc/types";
import { formatDuration } from "../format";
import { t, tn } from "../i18n";
import { errorMessage } from "../labels";
import { languageName, sourceLabel } from "./Transcripts";

/** Ready transcripts first, then the newest; failed, stale and the rest are not shown. */
export function usableTranscripts(list: TranscriptDto[]): TranscriptDto[] {
  return list.filter((x) => x.status === "ready" || x.status === "partial").sort((a, b) => Number(b.status === "ready") - Number(a.status === "ready") || b.updatedAt - a.updatedAt);
}

/** Index of the segment playing at `ms`: the last one that has started; -1 before the first. */
export function currentIndex(segs: TranscriptDetail["segments"], ms: number): number {
  let lo = 0;
  let hi = segs.length - 1;
  let found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (segs[mid]!.startMs <= ms) {
      found = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  return found;
}

/**
 * The transcript beside the player (STATE.md F6.5; PLAY-06, DATA-11). The
 * line being said is marked and followed; a timecode seeks the video to
 * that segment's start. Lines can be picked for a Markdown note that the
 * backend builds from the stored segments, shown before it is saved.
 */
export function TranscriptPanel({
  client,
  itemId,
  nowMs,
  onSeek,
  onAvailable,
  notify,
}: {
  client: TranscriptClient;
  itemId: string;
  nowMs: number;
  onSeek: (ms: number) => void;
  /** Called once it is known whether there is a transcript to show. */
  onAvailable?: (yes: boolean) => void;
  notify: (text: string, tone?: "info" | "error") => void;
}) {
  const [list, setList] = useState<TranscriptDto[] | null>(null);
  const [chosen, setChosen] = useState<string | null>(null);
  const [detail, setDetail] = useState<TranscriptDetail | null>(null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [follow, setFollow] = useState(true);
  const [preview, setPreview] = useState<NotePreview | null>(null);
  const [busy, setBusy] = useState(false);
  const lines = useRef<HTMLOListElement>(null);
  const available = useRef(onAvailable);
  available.current = onAvailable;

  useEffect(() => {
    let live = true;
    client.listTranscripts(itemId).then(
      (v) => {
        if (!live) return;
        const usable = usableTranscripts(v.transcripts);
        setList(usable);
        setChosen(usable[0]?.id ?? null);
        available.current?.(usable.length > 0);
      },
      () => live && available.current?.(false),
    );
    return () => {
      live = false;
    };
  }, [client, itemId]);

  useEffect(() => {
    if (!chosen) return;
    let live = true;
    setDetail(null);
    client.getTranscript(chosen).then(
      (d) => live && setDetail(d),
      (e) => live && notify(errorMessage(errorCode(e)), "error"),
    );
    return () => {
      live = false;
    };
  }, [client, chosen, notify]);

  const segs = useMemo(() => detail?.segments ?? [], [detail]);
  const current = currentIndex(segs, nowMs);
  useEffect(() => {
    if (!follow || current < 0) return;
    const li = lines.current?.children[current] as HTMLElement | undefined;
    li?.scrollIntoView?.({ block: "nearest" });
  }, [current, follow]);

  if (!list || list.length === 0) return null;
  const tr = list.find((x) => x.id === chosen) ?? list[0]!;
  const toggle = (id: string) =>
    setPicked((p) => {
      const n = new Set(p);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  const ids = segs.filter((s) => picked.has(s.id)).map((s) => s.id);
  const openPreview = () => {
    setBusy(true);
    client
      .previewNote(itemId, ids)
      .then(setPreview, (e) => notify(errorMessage(errorCode(e)), "error"))
      .finally(() => setBusy(false));
  };
  const save = () => {
    setBusy(true);
    client
      .exportNote(itemId, ids)
      .then(
        (r) => {
          if (!r.saved) return;
          notify(t("note.saved", { file: r.fileName ?? "" }));
          setPreview(null);
        },
        (e) => notify(errorMessage(errorCode(e)), "error"),
      )
      .finally(() => setBusy(false));
  };

  return (
    <aside className="player-transcript" aria-label={t("transcripts.title")} data-testid="player-transcript">
      <div className="player-transcript-head">
        {list.length > 1 ? (
          <select value={tr.id} onChange={(e) => setChosen(e.target.value)} aria-label={t("panel.choose")} data-testid="panel-transcript-choice">
            {list.map((x) => (
              <option key={x.id} value={x.id}>
                {languageName(x.language)} · {sourceLabel(x)}
              </option>
            ))}
          </select>
        ) : (
          <span className="small">
            <strong>{languageName(tr.language)}</strong> <span className="muted">· {sourceLabel(tr)}</span>
          </span>
        )}
        <label className="small panel-follow">
          <input type="checkbox" checked={follow} onChange={(e) => setFollow(e.target.checked)} data-testid="panel-follow" /> {t("panel.follow")}
        </label>
      </div>
      {tr.status === "partial" && <p className="muted small">{t("transcripts.partial", { covered: formatDuration(tr.coveredMs) })}</p>}
      {preview ? (
        <div className="note-preview" data-testid="note-preview">
          <p className="small muted">{t("note.preview_hint", { file: preview.fileName })}</p>
          <pre className="note-markdown" data-testid="note-markdown">
            {preview.markdown}
          </pre>
          <div className="modal-actions">
            <button type="button" className="button" onClick={() => setPreview(null)} data-testid="note-back">
              {t("note.back")}
            </button>
            <button type="button" className="button button-primary" onClick={save} disabled={busy} data-testid="note-save">
              {t("note.save")}
            </button>
          </div>
        </div>
      ) : (
        <>
          {!detail && <p role="status">{t("panel.loading")}</p>}
          <ol ref={lines} className="player-lines" aria-label={t("transcripts.lines_label", { language: languageName(tr.language) })} data-testid="panel-lines">
            {segs.map((s, i) => {
              const tc = formatDuration(s.startMs);
              return (
                <li key={s.id} className={i === current ? "current" : undefined} aria-current={i === current ? "true" : undefined} data-start-ms={s.startMs}>
                  <input type="checkbox" checked={picked.has(s.id)} onChange={() => toggle(s.id)} aria-label={t("panel.pick", { time: tc })} />
                  <button type="button" className="timecode link-button" onClick={() => onSeek(s.startMs)} aria-label={t("search.moment_label", { time: tc, text: s.text })}>
                    {tc}
                  </button>
                  <span className="transcript-text">{s.text}</span>
                </li>
              );
            })}
          </ol>
          <div className="player-transcript-foot">
            <span className="small muted" data-testid="note-count">
              {tn("panel.picked", ids.length)}
            </span>
            <button type="button" className="button" onClick={openPreview} disabled={busy || !detail} data-testid="note-export">
              {t("note.export")}
            </button>
          </div>
        </>
      )}
    </aside>
  );
}
