import type { TranscriptMatchesDto } from "../ipc/types";
import { formatDuration } from "../format";
import { t, tn } from "../i18n";
import { languageName } from "./Transcripts";

const SOURCE = {
  source_captions: "search.source.captions",
  generated_local: "search.source.local",
  imported: "transcripts.source.imported",
} as const;

/**
 * Where the search text was said in the item, grouped under it (STATE.md
 * F6.4, SEA-06). Each moment shows its real segment time; with a file on
 * this PC it plays from there (PLAY-06). A partial transcript says how far
 * it was searched (§16.2). Text is rendered as text (SEA-03).
 */
export function Moments({ matches, playable, onOpen }: { matches: TranscriptMatchesDto; playable: boolean; onOpen: (segmentId: string) => void }) {
  const more = matches.total - matches.moments.length;
  return (
    <section data-testid="inspector-moments">
      <h4>{t("search.said_title")}</h4>
      <p className="muted small">
        {languageName(matches.language)} · {t(SOURCE[matches.provenance])}
      </p>
      {matches.partial && (
        <p className="muted small" data-testid="moments-partial">
          {t("search.said_partial", { covered: formatDuration(matches.coveredMs) })}
        </p>
      )}
      <ol className="moments">
        {matches.moments.map((m) => {
          const text = m.snippet.parts.map((p) => p.text).join("");
          const body = (
            <>
              <span className="timecode">{formatDuration(m.startMs)}</span>{" "}
              <span className="moment-text">
                {m.snippet.cutStart && "…"}
                {m.snippet.parts.map((p, i) => (p.highlight ? <mark key={i}>{p.text}</mark> : <span key={i}>{p.text}</span>))}
                {m.snippet.cutEnd && "…"}
              </span>
            </>
          );
          return (
            <li key={m.segmentId} data-testid="moment" data-start-ms={m.startMs}>
              {playable ? (
                <button type="button" className="moment-button" onClick={() => onOpen(m.segmentId)} aria-label={t("search.moment_label", { time: formatDuration(m.startMs), text })}>
                  {body}
                </button>
              ) : (
                <span className="moment-button">{body}</span>
              )}
            </li>
          );
        })}
      </ol>
      {more > 0 && <p className="muted small">{tn("search.said_more", more)}</p>}
    </section>
  );
}
