import { useEffect, useState } from "react";
import { errorCode } from "../ipc/errors";
import type { DownloadClient, DownloadOptions, Preset, PresetId } from "../ipc/types";
import { formatBytes } from "../format";
import { errorMessage } from "../labels";
import { t } from "../i18n";
import { Modal } from "./Modal";
import { presetLabel } from "./DownloadsView";

function sizeText(p: Preset): string {
  if (p.sizeBytes !== null) return t("download.size", { size: formatBytes(p.sizeBytes) });
  if (p.sizeEstimateBytes !== null) return t("download.size_about", { size: formatBytes(p.sizeEstimateBytes) });
  return t("download.size_unknown");
}

function details(p: Preset, known: boolean): string {
  if (!known) return "";
  if (!p.available) return t("download.unavailable");
  const kind = [p.height ? `${p.height}p` : null, p.container?.toUpperCase(), [p.videoCodec, p.audioCodec].filter(Boolean).join(" + ")].filter(Boolean).join(" · ");
  return `${kind} · ${sizeText(p)}`;
}

/** Preset picker for one item (§9.5, DL-01/02). The UI sends only the preset name. */
export function DownloadDialog({
  client,
  itemId,
  title,
  onClose,
  onStarted,
}: {
  client: DownloadClient;
  itemId: string;
  title: string;
  onClose: () => void;
  onStarted: (message: string) => void;
}) {
  const [opts, setOpts] = useState<DownloadOptions | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [choice, setChoice] = useState<PresetId>("best");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let live = true;
    client.getDownloadOptions(itemId).then(
      (o) => {
        if (!live) return;
        setOpts(o);
        const first = o.presets.find((p) => p.available);
        if (first) setChoice(first.id);
      },
      (e) => live && setError(errorMessage(errorCode(e))),
    );
    return () => {
      live = false;
    };
  }, [client, itemId]);

  const start = async () => {
    setBusy(true);
    setError(null);
    try {
      const j = await client.startDownload(itemId, choice);
      onStarted(opts?.activeJobId === j.id ? t("download.already") : t("download.started"));
    } catch (e) {
      setError(errorMessage(errorCode(e)));
    } finally {
      setBusy(false);
    }
  };

  const blocker = opts ? (opts.live ? t("download.live") : opts.offline ? t("download.offline") : !opts.toolsReady ? t("download.tools") : opts.storage.quotaExceeded ? t("download.quota") : null) : null;
  const chosen = opts?.presets.find((p) => p.id === choice);
  return (
    <Modal title={t("download.title")} onClose={onClose} testId="download-dialog">
      <p className="muted">{title}</p>
      {!opts && !error && <p role="status">Loading…</p>}
      {opts && (
        <>
          {!opts.formatsKnown && <p className="muted small">{t("download.no_formats")}</p>}
          <fieldset className="choice-group">
            <legend>{t("download.title")}</legend>
            {opts.presets.map((p) => (
              <label key={p.id} className="choice preset">
                <input type="radio" name="download-preset" checked={choice === p.id} disabled={!p.available} onChange={() => setChoice(p.id)} data-testid={`preset-${p.id}`} />
                <span>
                  {presetLabel(p.id)}
                  <span className="muted small"> {details(p, opts.formatsKnown)}</span>
                </span>
              </label>
            ))}
          </fieldset>
          {chosen?.mayNotPlay && (
            <p className="warning small" data-testid="download-may-not-play">
              {t("download.may_not_play")}
            </p>
          )}
          {opts.storage.freeBytes !== null && <p className="muted small">{t("download.space", { free: formatBytes(opts.storage.freeBytes) })}</p>}
          {blocker && (
            <p className="warning small" role="alert" data-testid="download-blocked">
              {blocker}
            </p>
          )}
          <p className="muted small">{t("download.legal")}</p>
        </>
      )}
      {error && (
        <p className="job-error small" role="alert" data-testid="download-error">
          {error}
        </p>
      )}
      <div className="modal-actions">
        <button type="button" className="button" onClick={onClose}>
          Cancel
        </button>
        <button type="button" className="button button-primary" disabled={!opts || !!blocker || busy || !chosen?.available} onClick={() => void start()} data-testid="download-start">
          {t("download.start")}
        </button>
      </div>
    </Modal>
  );
}
