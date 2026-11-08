import { useCallback, useEffect, useRef, useState } from "react";
import { errorCode } from "../ipc/errors";
import type { BackendClient, PlaybackInfo, PlaybackSource } from "../ipc/types";
import { formatDuration } from "../format";
import { t } from "../i18n";
import { errorMessage } from "../labels";
import { Modal } from "./Modal";

/** How often the position is saved while playing (PLAY-02). */
export const SAVE_EVERY_MS = 5_000;

type Problem = "partial" | "failed" | null;

/**
 * The player view (STATE.md F2.4; PLAY-01…04; ADR-0004, ADR-0016).
 * Plays the item's file through the media protocol, offers to resume where
 * the user stopped, and decides "plays / sound only / fails" from what the
 * element actually did, not from `canPlayType`.
 */
export function Player({ client, itemId, onClose, notify }: { client: BackendClient; itemId: string; onClose: () => void; notify: (text: string, tone?: "info" | "error") => void }) {
  const [info, setInfo] = useState<PlaybackInfo | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [problem, setProblem] = useState<Problem>(null);
  const [started, setStarted] = useState(false);
  const video = useRef<HTMLVideoElement>(null);
  const dialog = useRef<HTMLDivElement>(null);
  const lastSaved = useRef(0);
  const source: PlaybackSource | null = info?.sources.find((s) => s.available) ?? info?.sources[0] ?? null;
  const resume = info?.state?.resumable && info.state.assetId === source?.assetId ? info.state : null;

  const load = useCallback(() => {
    client
      .getPlayback(itemId)
      .then((i) => {
        setInfo(i);
        setProblem(null);
      })
      .catch((e) => setLoadError(errorMessage(errorCode(e))));
  }, [client, itemId]);
  useEffect(load, [load]);

  const save = useCallback(() => {
    const v = video.current;
    if (!v || !source || !source.available || !started) return;
    lastSaved.current = Date.now();
    const duration = Number.isFinite(v.duration) && v.duration > 0 ? Math.round(v.duration * 1000) : source.durationMs;
    client.savePlaybackPosition(itemId, source.assetId, Math.round(v.currentTime * 1000), duration ?? null).catch(() => {});
  }, [client, itemId, source, started]);

  // The last position is saved when the player closes.
  const saveRef = useRef(save);
  saveRef.current = save;
  useEffect(() => () => saveRef.current(), []);

  useEffect(() => {
    dialog.current?.querySelector<HTMLElement>("[data-autofocus]")?.focus();
  }, [info]);

  const start = (fromMs: number) => {
    const v = video.current;
    if (!v) return;
    setStarted(true);
    const go = () => {
      v.currentTime = fromMs / 1000;
      void v.play().catch(() => {});
      v.focus();
    };
    if (v.readyState >= 1) go();
    else v.addEventListener("loadedmetadata", go, { once: true });
  };

  const close = () => {
    save();
    onClose();
  };

  const onKey = (e: React.KeyboardEvent) => {
    const v = video.current;
    if (!v || !source?.available) return;
    const onButton = (e.target as HTMLElement).closest("button, a, input");
    const seek = (d: number) => {
      v.currentTime = Math.max(0, Math.min(Number.isFinite(v.duration) ? v.duration : Infinity, v.currentTime + d));
      setStarted(true);
    };
    switch (e.key) {
      case " ":
      case "k":
        if (onButton && e.key === " ") return;
        e.preventDefault();
        setStarted(true);
        if (v.paused) void v.play().catch(() => {});
        else v.pause();
        break;
      case "ArrowLeft":
        e.preventDefault();
        seek(-5);
        break;
      case "ArrowRight":
        e.preventDefault();
        seek(5);
        break;
      case "j":
        seek(-10);
        break;
      case "l":
        seek(10);
        break;
      case "Home":
        e.preventDefault();
        seek(-v.currentTime);
        break;
      case "m":
        v.muted = !v.muted;
        break;
      case "f":
        if (document.fullscreenElement) void document.exitFullscreen().catch(() => {});
        else void v.requestFullscreen?.().catch(() => {});
        break;
    }
  };

  const openExternal = () => {
    if (!source) return;
    client
      .openExternally(source.assetId)
      .then(() => notify(t("player.opened_external")))
      .catch((e) => notify(errorMessage(errorCode(e)), "error"));
  };
  const openSource = () =>
    client
      .openSource(itemId)
      .then(() => notify(t("player.opened_source")))
      .catch((e) => notify(errorMessage(errorCode(e)), "error"));
  const relink = () => {
    if (!source) return;
    client
      .relinkAsset(source.assetId)
      .then((d) => {
        if (d) {
          notify(t("player.relinked"));
          load();
        }
      })
      .catch((e) => notify(errorMessage(errorCode(e)), "error"));
  };

  return (
    <Modal title={info?.title ?? t("player.title")} onClose={close} testId="player" wide>
      <div ref={dialog} className="player" onKeyDown={onKey}>
        {loadError && (
          <p className="warning" role="alert">
            {loadError}
          </p>
        )}
        {info && !source && (
          <div className="player-empty" data-testid="player-no-file">
            <p>{t("player.no_file")}</p>
            {info.hasSourceUrl && (
              <button type="button" className="button button-primary" onClick={openSource} data-autofocus data-testid="player-open-source">
                {t("player.open_source")}
              </button>
            )}
          </div>
        )}
        {source && !source.available && (
          <div className="player-empty" data-testid="player-unavailable">
            <p>{t("player.unavailable")}</p>
            <button type="button" className="button" onClick={relink} data-autofocus data-testid="player-relink">
              {t("player.relink")}
            </button>
          </div>
        )}
        {source?.available && (
          <>
            {source.support === "unsupported" && !problem && (
              <p className="warning small" data-testid="player-codec-warning">
                {t("player.codec_warning")}
              </p>
            )}
            {problem && (
              <p className="warning" role="alert" data-testid="player-problem" data-problem={problem}>
                {t(problem === "partial" ? "player.partial" : "player.failed")}
              </p>
            )}
            <video
              ref={video}
              className="player-video"
              src={source.url}
              controls
              preload="metadata"
              tabIndex={0}
              aria-label={info?.title}
              data-autofocus={resume ? undefined : true}
              data-testid="player-video"
              onPlay={() => setStarted(true)}
              onPause={save}
              onEnded={save}
              onTimeUpdate={() => Date.now() - lastSaved.current >= SAVE_EVERY_MS && save()}
              onLoadedData={(e) => {
                // ADR-0004: HEVC opens and plays sound, with no picture.
                if (source.mediaKind === "video" && e.currentTarget.videoWidth === 0) setProblem("partial");
              }}
              onError={() => setProblem("failed")}
            />
            {resume && !started && (
              <div className="player-resume" data-testid="player-resume">
                <button type="button" className="button button-primary" onClick={() => start(resume.positionMs)} data-autofocus data-testid="player-resume-button">
                  {t("player.resume", { time: formatDuration(resume.positionMs) })}
                </button>
                <button type="button" className="button" onClick={() => start(0)} data-testid="player-start-over">
                  {t("player.start_over")}
                </button>
              </div>
            )}
            <div className="player-foot">
              <span className="muted small">{t("player.keys")}</span>
              {(source.support !== "supported" || problem) && (
                <button type="button" className="button" onClick={openExternal} data-testid="player-open-external">
                  {t("player.open_external")}
                </button>
              )}
              {info?.hasSourceUrl && (
                <button type="button" className="link-button" onClick={openSource} data-testid="player-open-source">
                  {t("player.open_source")}
                </button>
              )}
            </div>
          </>
        )}
        <div className="modal-actions">
          <button type="button" className="button" onClick={close} data-testid="player-close">
            {t("common.close")}
          </button>
        </div>
      </div>
    </Modal>
  );
}
