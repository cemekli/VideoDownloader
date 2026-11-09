import { useEffect, useState } from "react";
import { errorCode } from "../ipc/errors";
import type { BackendClient, BackupDto, BackupPreview, PendingImport, Settings, SettingsPatch, StorageOverview, StorageUsage, Theme, ToolStatus } from "../ipc/types";
import { formatBytes } from "../format";
import { formatDateTime, t, tn, type MessageKey } from "../i18n";
import { errorMessage } from "../labels";
import { Modal } from "./Modal";

type Notify = (text: string, tone?: "info" | "error") => void;

export const THEMES: Theme[] = ["dark", "light", "system"];

/** Theme radio group, shared by Settings and the first run. */
export function ThemeChoice({ value, onChange, name }: { value: Theme; onChange: (t: Theme) => void; name: string }) {
  return (
    <fieldset className="choice-group">
      <legend>{t("settings.theme")}</legend>
      {THEMES.map((th) => (
        <label key={th} className="choice">
          <input type="radio" name={name} value={th} checked={value === th} onChange={() => onChange(th)} data-testid={`theme-${th}`} />
          {t(`settings.theme.${th}` as MessageKey)}
        </label>
      ))}
    </fieldset>
  );
}

/** The media folder line: name, free space, cloud warning (§9.7 step 3). */
const GB = 1024 * 1024 * 1024;
const QUOTAS = [null, 10, 25, 50, 100, 250, 500] as const;

/** DATA-07: what downloads use, the optional limit and moving the folder (DATA-08). */
export function StorageUsageBlock({ usage, busy, onQuota, onMove }: { usage: StorageUsage | null; busy: boolean; onQuota: (bytes: number | null) => void; onMove: () => void }) {
  if (!usage) return null;
  const current = usage.quotaBytes === null ? "" : String(Math.round(usage.quotaBytes / GB));
  const options = QUOTAS.map((q) => (q === null ? "" : String(q)));
  if (!options.includes(current)) options.push(current);
  return (
    <div className="storage-usage" data-testid="storage-usage">
      <p className="muted small">{t("storage.usage", { media: formatBytes(usage.managedMediaBytes), derived: formatBytes(usage.derivedBytes), staging: formatBytes(usage.stagingBytes) })}</p>
      {usage.lowSpace && (
        <p className="warning small" role="alert" data-testid="storage-low">
          {t("storage.low")}
        </p>
      )}
      {usage.quotaExceeded && (
        <p className="warning small" role="alert" data-testid="storage-quota-exceeded">
          {t("storage.quota.exceeded")}
        </p>
      )}
      <label className="field">
        <span>{t("storage.quota")}</span>
        <select value={current} disabled={busy} onChange={(e) => onQuota(e.target.value === "" ? null : Number(e.target.value) * GB)} data-testid="storage-quota">
          {options.map((o) => (
            <option key={o} value={o}>
              {o === "" ? t("storage.quota.none") : t("storage.quota.gb", { size: Number(o) })}
            </option>
          ))}
        </select>
      </label>
      <div className="actions">
        <button type="button" className="button" disabled={busy} onClick={onMove} aria-describedby="move-help" data-testid="storage-move">
          {t("storage.move")}
        </button>
      </div>
      <p id="move-help" className="muted small">
        {t("storage.move.help")}
      </p>
    </div>
  );
}

/** MT-11: each managed tool's pinned version and check result. No paths. */
export function ToolHealthBlock({ tools }: { tools: ToolStatus | null }) {
  if (!tools) return null;
  return (
    <div data-testid="tool-health">
      <h4>{t("diag.tools")}</h4>
      <p className={tools.ready ? "muted small" : "warning small"} data-testid="tool-health-summary">
        {tools.checking ? t("diag.checking") : tools.ready ? t("diag.ready") : t("diag.not_ready", { code: tools.code ?? "TOOL_MISSING" })}
      </p>
      <ul className="tool-list">
        {tools.tools.map((x) => (
          <li key={x.id} data-testid="tool" data-state={x.state}>
            <span>
              {x.id} <span className="muted small">{t("diag.version", { version: x.version })}</span>
            </span>
            <span className={x.state === "ok" ? "" : "warning"}>{t(`diag.state.${x.state}` as MessageKey)}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

export function StorageLine({ storage, onChoose }: { storage: StorageOverview | null; onChoose: () => void }) {
  return (
    <div className="storage-line" data-testid="storage-line">
      <div>
        <strong>{storage ? (storage.isDefault ? `${t("settings.storage.default")} (${storage.label})` : storage.label) : "…"}</strong>
        {storage?.freeBytes != null && <span className="muted"> · {t("settings.storage.free", { size: formatBytes(storage.freeBytes) })}</span>}
      </div>
      <button type="button" className="button" onClick={onChoose} data-testid="choose-media-folder">
        {t("common.choose_folder")}
      </button>
      {storage?.cloudSynced && (
        <p className="warning small" role="alert" data-testid="storage-cloud-warning">
          {t("settings.storage.cloud")}
        </p>
      )}
    </div>
  );
}

/** The legal notice (§9.7 step 2), also reachable from Settings → About. */
export function LegalNotice() {
  return <p data-testid="legal-text">{t("onboarding.legal.body")}</p>;
}

/**
 * Settings (STATE.md F2.5–F2.8): appearance, Offline Mode, updates, media
 * folder, export/import, backups and About.
 */
export function SettingsDialog({
  client,
  settings,
  onSettings,
  onClose,
  notify,
  onLibraryChanged,
}: {
  client: BackendClient;
  settings: Settings;
  onSettings: (patch: SettingsPatch) => Promise<void>;
  onClose: () => void;
  notify: Notify;
  onLibraryChanged: () => void;
}) {
  const [storage, setStorage] = useState<StorageOverview | null>(null);
  const [usage, setUsage] = useState<StorageUsage | null>(null);
  const [tools, setTools] = useState<ToolStatus | null>(null);
  const [backups, setBackups] = useState<BackupDto[] | null>(null);
  const [paths, setPaths] = useState(false);
  const [busy, setBusy] = useState(false);
  const [importing, setImporting] = useState<PendingImport | null>(null);
  const [restoring, setRestoring] = useState<BackupPreview | null>(null);
  const [version, setVersion] = useState("");
  const [legal, setLegal] = useState(false);
  const fail = (e: unknown) => notify(errorMessage(errorCode(e)), "error");

  useEffect(() => {
    client.getStorageOverview().then(setStorage, fail);
    client.getStorageUsage().then(setUsage, fail);
    client.getToolHealth().then(setTools, () => {});
    client.listBackups().then(setBackups, fail);
    client.getStatus().then((s) => setVersion(s.appVersion), () => {});
  }, [client]);

  const run = async (f: () => Promise<void>) => {
    setBusy(true);
    try {
      await f();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };

  if (importing) return <ImportLibraryDialog client={client} pending={importing} onClose={() => setImporting(null)} notify={notify} onDone={onLibraryChanged} />;
  if (restoring)
    return (
      <RestoreDialog
        client={client}
        preview={restoring}
        onClose={() => setRestoring(null)}
        notify={notify}
        onDone={() => {
          setRestoring(null);
          onLibraryChanged();
          client.listBackups().then(setBackups, fail);
        }}
      />
    );

  return (
    <Modal title={t("settings.title")} onClose={onClose} testId="settings" wide>
      <div className="settings">
        <section aria-labelledby="set-appearance">
          <h3 id="set-appearance">{t("settings.appearance")}</h3>
          <ThemeChoice name="settings-theme" value={settings.theme} onChange={(theme) => void onSettings({ theme })} />
          <label className="field">
            <span>{t("settings.language")}</span>
            <select value={settings.locale} onChange={(e) => void onSettings({ locale: e.target.value })} data-testid="settings-locale">
              <option value="en">{t("settings.language.en")}</option>
            </select>
          </label>
        </section>

        <section aria-labelledby="set-network">
          <h3 id="set-network">{t("settings.network")}</h3>
          <label className="choice">
            <input type="checkbox" checked={settings.offlineMode} onChange={(e) => void onSettings({ offlineMode: e.target.checked })} aria-describedby="offline-help" data-testid="settings-offline" />
            {t("settings.offline")}
          </label>
          <p id="offline-help" className="muted small">
            {t("settings.offline.help")}
          </p>
          <fieldset className="choice-group">
            <legend>{t("settings.updates")}</legend>
            {(["manual", "automatic"] as const).map((u) => (
              <label key={u} className="choice">
                <input type="radio" name="settings-updates" checked={settings.updateCheck === u} onChange={() => void onSettings({ updateCheck: u })} data-testid={`updates-${u}`} />
                {t(`settings.updates.${u}`)}
              </label>
            ))}
          </fieldset>
          <p className="muted small">{t("settings.updates.help")}</p>
        </section>

        <section aria-labelledby="set-storage">
          <h3 id="set-storage">{t("settings.storage")}</h3>
          <StorageLine storage={storage} onChoose={() => void run(async () => setStorage((await client.chooseMediaFolder()) ?? storage))} />
          <StorageUsageBlock
            usage={usage}
            busy={busy}
            onQuota={(bytes) => void run(async () => setUsage(await client.setStorageQuota(bytes)))}
            onMove={() =>
              void run(async () => {
                if (await client.moveMediaFolder()) notify(t("storage.move.started"));
              })
            }
          />
        </section>

        <section aria-labelledby="set-diag">
          <h3 id="set-diag">{t("diag.title")}</h3>
          <ToolHealthBlock tools={tools} />
        </section>

        <section aria-labelledby="set-data">
          <h3 id="set-data">{t("settings.data")}</h3>
          <p className="muted small">{t("settings.export.contents")}</p>
          <label className="choice">
            <input type="checkbox" checked={paths} onChange={(e) => setPaths(e.target.checked)} aria-describedby="paths-help" data-testid="export-paths" />
            {t("settings.export.paths")}
          </label>
          <p id="paths-help" className="muted small">
            {t("settings.export.paths.help")}
          </p>
          <div className="actions">
            <button
              type="button"
              className="button"
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  const r = await client.exportLibrary(paths);
                  if (r.saved) notify(t("settings.exported", { items: r.items, file: r.fileName ?? "" }));
                })
              }
              data-testid="export-library"
            >
              {t("settings.export")}
            </button>
            <button
              type="button"
              className="button"
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  const p = await client.previewLibraryImport();
                  if (p) setImporting(p);
                })
              }
              data-testid="import-library"
            >
              {t("settings.import")}
            </button>
          </div>

          <h4>{t("settings.backups")}</h4>
          <div className="actions">
            <button
              type="button"
              className="button"
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  const b = await client.backupLibrary();
                  notify(t("settings.backed_up", { size: formatBytes(b.sizeBytes) }));
                  setBackups(await client.listBackups());
                })
              }
              data-testid="backup-now"
            >
              {t("settings.backup")}
            </button>
            <button
              type="button"
              className="button"
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  const p = await client.inspectBackup(null);
                  if (p) setRestoring(p);
                })
              }
              data-testid="restore-from-file"
            >
              {t("settings.backup.from_file")}
            </button>
          </div>
          {backups && backups.length === 0 && <p className="muted small">{t("settings.backups.none")}</p>}
          {backups && backups.length > 0 && (
            <ul className="backup-list" data-testid="backup-list">
              {backups.map((b) => (
                <li key={b.name} data-testid="backup-row" data-kind={b.kind}>
                  <span>
                    {t(`settings.backup.kind.${b.kind}` as MessageKey)} · {formatDateTime(b.createdAt)} · {formatBytes(b.sizeBytes)}
                  </span>
                  <button type="button" className="link-button" disabled={busy} onClick={() => void run(async () => setRestoring(await client.inspectBackup(b.name)))} aria-label={`${t("settings.backup.restore")} ${formatDateTime(b.createdAt)}`} data-testid="backup-restore">
                    {t("settings.backup.restore")}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>

        <section aria-labelledby="set-about">
          <h3 id="set-about">{t("settings.about")}</h3>
          <p>{t("settings.about.version", { version })}</p>
          <p className="muted small">{t("settings.about.local")}</p>
          <button type="button" className="link-button" aria-expanded={legal} onClick={() => setLegal((v) => !v)} data-testid="show-legal">
            {t("settings.about.legal")}
          </button>
          {legal && <LegalNotice />}
        </section>
      </div>
      <div className="modal-actions">
        <button type="button" className="button button-primary" onClick={onClose} data-testid="settings-close">
          {t("common.done")}
        </button>
      </div>
    </Modal>
  );
}

/** §15.2 steps 3–6: what the file holds, where its media is, conflict policy. */
export function ImportLibraryDialog({ client, pending, onClose, notify, onDone }: { client: BackendClient; pending: PendingImport; onClose: () => void; notify: Notify; onDone: () => void }) {
  const p = pending.preview;
  const [mapped, setMapped] = useState<Record<string, string>>({});
  const [policy, setPolicy] = useState<"keep_existing" | "replace_user_fields">("keep_existing");
  const [original, setOriginal] = useState(false);
  const [busy, setBusy] = useState(false);
  const cancel = () => {
    void client.cancelPending(pending.token).catch(() => {});
    onClose();
  };
  const row = (key: MessageKey, n: number, id: string) => (
    <tr data-testid={id}>
      <th scope="row">{t(key)}</th>
      <td>{n}</td>
    </tr>
  );
  return (
    <Modal title={t("import_lib.title")} onClose={cancel} testId="import-library-dialog" wide>
      <p className="muted">{t("import_lib.summary", { file: pending.fileName, date: p.exportedAt.slice(0, 10) })}</p>
      <table className="summary-table">
        <tbody>
          {row("import_lib.new", p.itemsNew, "import-new")}
          {row("import_lib.matched", p.itemsMatched, "import-matched")}
          {p.itemsSkipped > 0 && row("import_lib.skipped", p.itemsSkipped, "import-skipped")}
          {p.removed > 0 && row("import_lib.removed", p.removed, "import-removed")}
          {row("import_lib.collections", p.collections, "import-collections")}
          {row("import_lib.tags", p.tags, "import-tags")}
          {row("import_lib.media", p.mediaReferences, "import-media")}
          {p.unknownFields > 0 && row("import_lib.unknown", p.unknownFields, "import-unknown")}
        </tbody>
      </table>
      {p.roots.length > 0 && (
        <>
          <h3>{t("import_lib.roots")}</h3>
          <p className="muted small">{t("import_lib.roots.help")}</p>
          <ul className="root-map">
            {p.roots.map((r) => (
              <li key={r.id} data-testid="import-root">
                <span>
                  <strong>{r.label}</strong> · {tn("import_lib.root.files", r.references)} · {mapped[r.id] ? t("import_lib.root.mapped", { folder: mapped[r.id]! }) : t("import_lib.root.unmapped")}
                </span>
                <button
                  type="button"
                  className="button"
                  disabled={busy}
                  aria-label={`${t("common.choose_folder")} ${r.label}`}
                  onClick={async () => {
                    try {
                      const name = await client.mapImportRoot(pending.token, r.id);
                      if (name) setMapped((m) => ({ ...m, [r.id]: name }));
                    } catch (e) {
                      notify(errorMessage(errorCode(e)), "error");
                    }
                  }}
                  data-testid="import-root-choose"
                >
                  {t("common.choose_folder")}
                </button>
              </li>
            ))}
          </ul>
        </>
      )}
      <fieldset className="choice-group">
        <legend>{t("import_lib.policy")}</legend>
        {(["keep_existing", "replace_user_fields"] as const).map((v) => (
          <label key={v} className="choice">
            <input type="radio" name="import-policy" checked={policy === v} onChange={() => setPolicy(v)} data-testid={`import-policy-${v}`} />
            {t(`import_lib.policy.${v}`)}
          </label>
        ))}
      </fieldset>
      {p.absolutePaths > 0 && (
        <>
          <label className="choice">
            <input type="checkbox" checked={original} onChange={(e) => setOriginal(e.target.checked)} aria-describedby="original-help" data-testid="import-original" />
            {t("import_lib.original", { count: p.absolutePaths })}
          </label>
          <p id="original-help" className="muted small">
            {t("import_lib.original.help")}
          </p>
        </>
      )}
      <div className="modal-actions">
        <button type="button" className="button" onClick={cancel}>
          {t("common.cancel")}
        </button>
        <button
          type="button"
          className="button button-primary"
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            try {
              const r = await client.applyLibraryImport({ token: pending.token, policy, useOriginalLocations: original });
              notify(t("import_lib.done", { added: r.itemsAdded, matched: r.itemsMatched, linked: r.mediaLinked, missing: r.mediaMissing }));
              onDone();
              onClose();
            } catch (e) {
              notify(errorMessage(errorCode(e)), "error");
              setBusy(false);
            }
          }}
          data-testid="import-apply"
        >
          {t("import_lib.apply")}
        </button>
      </div>
    </Modal>
  );
}

/** §15.3 / AC-50, AC-57: what the backup holds before it replaces the library. */
export function RestoreDialog({ client, preview, onClose, notify, onDone }: { client: BackendClient; preview: BackupPreview; onClose: () => void; notify: Notify; onDone: () => void }) {
  const [busy, setBusy] = useState(false);
  const cancel = () => {
    if (preview.token) void client.cancelPending(preview.token).catch(() => {});
    onClose();
  };
  return (
    <Modal title={t("restore.title")} onClose={cancel} testId="restore-dialog">
      <p>{t("restore.intro")}</p>
      <ul className="plain-list">
        <li data-testid="restore-items">
          {t("restore.items")}: {preview.items}
        </li>
        {preview.removed > 0 && <li data-testid="restore-removed">{t("restore.removed", { count: preview.removed })}</li>}
        <li className="muted small">{t("restore.media", { count: preview.mediaFiles })}</li>
        <li className="muted small">{t("restore.schema", { version: preview.schemaVersion })}</li>
      </ul>
      <div className="modal-actions">
        <button type="button" className="button" onClick={cancel}>
          {t("common.cancel")}
        </button>
        <button
          type="button"
          className="button button-primary"
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            try {
              const r = await client.restoreBackup(preview.token ? { token: preview.token } : { name: preview.name });
              notify(r.mediaMissing ? t("restore.done_missing", { items: r.items, missing: r.mediaMissing, backup: r.preRestoreBackup }) : t("restore.done", { items: r.items, backup: r.preRestoreBackup }));
              onDone();
            } catch (e) {
              notify(errorMessage(errorCode(e)), "error");
              setBusy(false);
            }
          }}
          data-testid="restore-confirm"
        >
          {t("restore.confirm")}
        </button>
      </div>
    </Modal>
  );
}
