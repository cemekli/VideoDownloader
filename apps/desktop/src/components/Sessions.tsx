import { useCallback, useEffect, useState } from "react";
import { errorCode } from "../ipc/errors";
import type { Platform, SessionClient, SessionDto, SessionOperation, SessionOverview } from "../ipc/types";
import { formatDateTime, t, type MessageKey } from "../i18n";
import { errorMessage } from "../labels";
import { Modal } from "./Modal";

type Notify = (text: string, tone?: "info" | "error") => void;

const OPS: SessionOperation[] = ["metadata", "download"];
const platformName = (p: string) => t(`platform.${p}` as MessageKey);

/**
 * Connect a site to a Firefox profile (STATE.md F5.2, SPEC §20.3): the user
 * sees which browser profile is read, for which site and for what, and
 * gives consent explicitly. Nothing is connected until they press Connect.
 */
export function ConnectSession({
  client,
  overview,
  provider: initialProvider,
  notify,
  onConnected,
}: {
  client: SessionClient;
  overview: SessionOverview;
  provider?: Platform | null;
  notify: Notify;
  onConnected: (s: SessionDto) => void;
}) {
  const profiles = overview.browserProfiles;
  const [provider, setProvider] = useState<Platform>(initialProvider && overview.providers.includes(initialProvider) ? initialProvider : (overview.providers[0] ?? "reddit"));
  const [profile, setProfile] = useState(profiles.find((p) => p.isDefault)?.name ?? profiles[0]?.name ?? "");
  const [ops, setOps] = useState<SessionOperation[]>(OPS);
  const [agreed, setAgreed] = useState(false);
  const [busy, setBusy] = useState(false);

  if (profiles.length === 0)
    return (
      <p className="muted small" data-testid="session-no-firefox">
        {t("sessions.no_firefox")}
      </p>
    );

  const connect = async () => {
    setBusy(true);
    try {
      const s = await client.connectSession(provider, "firefox", profile, ops);
      notify(t("sessions.connected", { site: platformName(s.provider) }));
      onConnected(s);
    } catch (e) {
      notify(errorMessage(errorCode(e)), "error");
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      className="session-connect"
      data-testid="session-connect"
      onSubmit={(e) => {
        e.preventDefault();
        void connect();
      }}
    >
      <label className="field">
        <span>{t("sessions.site")}</span>
        <select value={provider} onChange={(e) => setProvider(e.target.value as Platform)} data-testid="session-provider">
          {overview.providers.map((p) => (
            <option key={p} value={p}>
              {platformName(p)}
            </option>
          ))}
        </select>
      </label>
      <label className="field">
        <span>{t("sessions.firefox_profile")}</span>
        <select value={profile} onChange={(e) => setProfile(e.target.value)} data-testid="session-browser-profile">
          {profiles.map((p) => (
            <option key={p.name} value={p.name}>
              {p.name}
              {p.isDefault ? ` (${t("sessions.default")})` : ""}
              {p.hasCookies ? "" : ` · ${t("sessions.unused")}`}
            </option>
          ))}
        </select>
      </label>
      <fieldset className="choice-group">
        <legend>{t("sessions.use_for")}</legend>
        {OPS.map((op) => (
          <label key={op} className="choice">
            <input
              type="checkbox"
              checked={ops.includes(op)}
              onChange={(e) => setOps((cur) => (e.target.checked ? [...cur, op] : cur.filter((o) => o !== op)))}
              data-testid={`session-op-${op}`}
            />
            {t(`sessions.op.${op}` as MessageKey)}
          </label>
        ))}
      </fieldset>
      <p className="muted small" id="session-consent-help">
        {t("sessions.consent.body", { site: platformName(provider), profile })}
      </p>
      <label className="choice">
        <input type="checkbox" checked={agreed} onChange={(e) => setAgreed(e.target.checked)} aria-describedby="session-consent-help" data-testid="session-consent" />
        {t("sessions.consent.agree")}
      </label>
      <div className="actions">
        <button type="submit" className="button button-primary" disabled={busy || !agreed || ops.length === 0 || !profile} data-testid="session-connect-submit">
          {t("sessions.connect")}
        </button>
      </div>
    </form>
  );
}

function SessionRow({ s, busy, onClear, onDelete }: { s: SessionDto; busy: boolean; onClear: () => void; onDelete: () => void }) {
  return (
    <li className="session" data-testid="session" data-state={s.state} data-provider={s.provider}>
      <div>
        <strong>{platformName(s.provider)}</strong>
        <span className="muted"> · Firefox ({s.browserProfile ?? "?"})</span>
      </div>
      <p className="small" data-testid="session-state">
        {t(`sessions.state.${s.state}` as MessageKey)}
        {s.state === "session_available" && s.lastVerifiedAt ? ` · ${t("sessions.verified_at", { time: formatDateTime(s.lastVerifiedAt) })}` : ""}
      </p>
      <p className="muted small">{s.consent.length ? t("sessions.allowed", { ops: s.consent.map((o) => t(`sessions.op.${o}` as MessageKey)).join(", ") }) : t("sessions.no_consent")}</p>
      <div className="actions">
        {s.state !== "not_connected" && (
          <button type="button" className="button" disabled={busy} onClick={onClear} data-testid="session-clear">
            {t("sessions.clear")}
          </button>
        )}
        <button type="button" className="button button-quiet" disabled={busy} onClick={onDelete} data-testid="session-delete">
          {t("sessions.delete")}
        </button>
      </div>
    </li>
  );
}

/** Settings → Sessions: the connected sites and connecting a new one. */
export function SessionsBlock({ client, notify }: { client: SessionClient; notify: Notify }) {
  const [overview, setOverview] = useState<SessionOverview | null>(null);
  const [adding, setAdding] = useState(false);
  const [busy, setBusy] = useState(false);
  const fail = (e: unknown) => notify(errorMessage(errorCode(e)), "error");
  const load = useCallback(() => client.listSessions().then(setOverview, fail), [client]);
  useEffect(() => {
    void load();
  }, [load]);

  const act = async (f: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await f();
      await load();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };

  if (!overview) return null;
  return (
    <div data-testid="sessions">
      <p className="muted small">{t("sessions.help")}</p>
      {overview.sessions.length === 0 ? (
        <p className="muted small" data-testid="sessions-empty">
          {t("sessions.empty")}
        </p>
      ) : (
        <ul className="sessions">
          {overview.sessions.map((s) => (
            <SessionRow key={s.id} s={s} busy={busy} onClear={() => void act(() => client.clearSession(s.id))} onDelete={() => void act(() => client.deleteSession(s.id))} />
          ))}
        </ul>
      )}
      {adding ? (
        <ConnectSession
          client={client}
          overview={overview}
          notify={notify}
          onConnected={() => {
            setAdding(false);
            void load();
          }}
        />
      ) : (
        <button type="button" className="button" onClick={() => setAdding(true)} data-testid="sessions-add">
          {t("sessions.add")}
        </button>
      )}
    </div>
  );
}

/** From a download waiting for a login: connect a session for its site. */
export function ConnectSessionDialog({ client, provider, notify, onClose }: { client: SessionClient; provider: Platform | null; notify: Notify; onClose: () => void }) {
  const [overview, setOverview] = useState<SessionOverview | null>(null);
  useEffect(() => {
    client.listSessions().then(setOverview, (e) => notify(errorMessage(errorCode(e)), "error"));
  }, [client]);
  return (
    <Modal title={t("sessions.connect_title", { site: provider ? platformName(provider) : "" })} onClose={onClose} testId="session-dialog">
      {overview && <ConnectSession client={client} overview={overview} provider={provider} notify={notify} onConnected={onClose} />}
    </Modal>
  );
}
