import { useCallback, useEffect, useState } from "react";
import { errorCode, IpcError } from "../ipc/errors";
import type { Platform, SessionClient, SessionDto, SessionOperation, SessionOverview, VerifyOutcome } from "../ipc/types";
import { formatDateTime, t, type MessageKey } from "../i18n";
import { errorMessage } from "../labels";
import { Modal } from "./Modal";

type Notify = (text: string, tone?: "info" | "error") => void;
type ConnectMethod = "firefox" | "file" | "app";

const OPS: SessionOperation[] = ["metadata", "download", "verify"];
const platformName = (p: string) => t(`platform.${p}` as MessageKey);
const opName = (o: SessionOperation) => t(`sessions.op.${o}` as MessageKey);

/** A refused cookie file says why (`session-file-*`); everything else is the §19 message. */
function connectError(e: unknown): string {
  const reason = e instanceof IpcError ? e.domain?.safeDetails?.reason : undefined;
  if (
    reason === "session-file-no-cookies" ||
    reason === "session-file-unreadable" ||
    reason === "session-file-too-large" ||
    reason === "session-app-not-signed-in" ||
    reason === "session-app-closed"
  )
    return t(`sessions.${reason}` as MessageKey);
  return errorMessage(errorCode(e));
}

/**
 * Connect a site (STATE.md F5.2, F5.3, F5.7; SPEC §20.3, §21.1, §21.3):
 * with a Firefox profile, a cookie file the user exported, or by signing in
 * inside the app's own private sign-in window. The user sees what is read,
 * for which site and for what, and gives consent explicitly. Nothing is
 * connected until they press Connect.
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
  const [method, setMethod] = useState<ConnectMethod>(profiles.length > 0 || !overview.cookieFileSupported ? "firefox" : "file");
  const [profile, setProfile] = useState(profiles.find((p) => p.isDefault)?.name ?? profiles[0]?.name ?? "");
  const [ops, setOps] = useState<SessionOperation[]>(OPS);
  const [agreed, setAgreed] = useState(false);
  const [busy, setBusy] = useState(false);
  // The in-app sign-in window is open and waits for the user (F5.7).
  const [waiting, setWaiting] = useState(false);
  const site = platformName(provider);

  // A sign-in window left open when this form goes away is closed.
  useEffect(() => {
    if (!waiting) return;
    return () => void client.cancelAppSignIn().catch(() => undefined);
  }, [waiting, client]);

  const finishApp = async () => {
    setBusy(true);
    try {
      const s = await client.finishAppSignIn(provider, ops);
      setWaiting(false);
      notify(t("sessions.connected_app", { site: platformName(s.provider) }));
      onConnected(s);
    } catch (e) {
      if (e instanceof IpcError && e.domain?.safeDetails?.reason === "session-app-closed") setWaiting(false);
      notify(connectError(e), "error");
    } finally {
      setBusy(false);
    }
  };

  const cancelApp = async () => {
    setWaiting(false);
    await client.cancelAppSignIn().catch(() => undefined);
  };

  const connect = async () => {
    setBusy(true);
    try {
      if (method === "app") {
        await client.startAppSignIn(provider);
        setWaiting(true);
        return;
      }
      const s = method === "firefox" ? await client.connectSession(provider, "firefox", profile, ops) : await client.connectSessionFile(provider, ops);
      if (!s) return; // The file dialog was closed.
      notify(t(method === "file" ? "sessions.connected_file" : "sessions.connected", { site: platformName(s.provider) }));
      onConnected(s);
    } catch (e) {
      notify(connectError(e), "error");
    } finally {
      setBusy(false);
    }
  };

  const ready = method === "firefox" ? profiles.length > 0 && !!profile : method === "file" ? overview.cookieFileSupported : overview.appSignInSupported;
  if (waiting)
    return (
      <div className="session-connect" data-testid="session-app-waiting">
        <p data-testid="session-app-waiting-text">{t("sessions.app.waiting", { site })}</p>
        <div className="actions">
          <button type="button" className="button button-primary" disabled={busy} onClick={() => void finishApp()} data-testid="session-app-done">
            {t("sessions.app.done")}
          </button>
          <button type="button" className="button" disabled={busy} onClick={() => void cancelApp()} data-testid="session-app-cancel">
            {t("sessions.app.cancel")}
          </button>
        </div>
      </div>
    );
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
      <fieldset className="choice-group">
        <legend>{t("sessions.method")}</legend>
        <label className="choice">
          <input type="radio" name="session-method" checked={method === "firefox"} onChange={() => setMethod("firefox")} data-testid="session-method-firefox" />
          {t("sessions.method.firefox")}
        </label>
        <label className="choice">
          <input type="radio" name="session-method" checked={method === "file"} onChange={() => setMethod("file")} disabled={!overview.cookieFileSupported} data-testid="session-method-file" />
          {t("sessions.method.file")}
        </label>
        <label className="choice">
          <input type="radio" name="session-method" checked={method === "app"} onChange={() => setMethod("app")} disabled={!overview.appSignInSupported} data-testid="session-method-app" />
          {t("sessions.method.app")}
        </label>
      </fieldset>
      {method === "firefox" ? (
        profiles.length === 0 ? (
          <p className="muted small" data-testid="session-no-firefox">
            {t("sessions.no_firefox")}
          </p>
        ) : (
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
        )
      ) : method === "file" ? (
        <p className="muted small" data-testid="session-file-help">
          {overview.cookieFileSupported ? t("sessions.file.help", { site }) : t("sessions.file.unsupported")}
        </p>
      ) : (
        <p className="muted small" data-testid="session-app-help">
          {overview.appSignInSupported ? t("sessions.app.help", { site }) : t("sessions.app.unsupported")}
        </p>
      )}
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
            {opName(op)}
          </label>
        ))}
      </fieldset>
      <p className="muted small" id="session-consent-help">
        {method === "firefox" ? t("sessions.consent.body", { site, profile }) : method === "file" ? t("sessions.consent.file", { site }) : t("sessions.consent.app", { site })}
      </p>
      <label className="choice">
        <input type="checkbox" checked={agreed} onChange={(e) => setAgreed(e.target.checked)} aria-describedby="session-consent-help" data-testid="session-consent" />
        {t("sessions.consent.agree")}
      </label>
      <div className="actions">
        <button type="submit" className="button button-primary" disabled={busy || !agreed || ops.length === 0 || !ready} data-testid="session-connect-submit">
          {method === "file" ? t("sessions.connect_file") : method === "app" ? t("sessions.app.open") : t("sessions.connect")}
        </button>
      </div>
    </form>
  );
}

const verifyTone = (o: VerifyOutcome): "info" | "error" => (o === "verified" || o === "no_test_item" || o === "public_item" ? "info" : "error");

function methodText(s: SessionDto): string {
  if (s.method === "cookie_file") return t("sessions.method.file_short");
  if (s.method === "app_profile") return t("sessions.method.app_short");
  return `Firefox (${s.browserProfile ?? "?"})`;
}

function SessionRow({ s, client, busy, act, notify }: { s: SessionDto; client: SessionClient; busy: boolean; act: (f: () => Promise<unknown>) => Promise<void>; notify: Notify }) {
  const cleared = s.state === "not_connected";
  const verify = () =>
    act(async () => {
      const r = await client.verifySession(s.id);
      const text = t(`sessions.verify.${r.outcome}` as MessageKey, { site: platformName(s.provider) });
      // A failed check names its §19 reason, so "network" and "the page couldn't be read" are told apart.
      notify(r.outcome === "failed" && r.code ? `${text} ${t("sessions.verify.reason", { reason: errorMessage(r.code) })}` : text, verifyTone(r.outcome));
    });
  const toggle = (op: SessionOperation, on: boolean) => act(() => client.setSessionConsent(s.id, on ? OPS.filter((o) => o === op || s.consent.includes(o)) : s.consent.filter((o) => o !== op)));
  return (
    <li className="session job" data-testid="session" data-state={s.state} data-provider={s.provider} data-method={s.method}>
      <div className="job-head">
        <strong>
          {platformName(s.provider)} <span className="muted">· {methodText(s)}</span>
        </strong>
        <span className={`session-state session-state-${s.state}`} data-testid="session-state">
          {t(`sessions.state.${s.state}` as MessageKey)}
        </span>
      </div>
      <p className="muted small" data-testid="session-verified">
        {s.lastVerifiedAt ? t("sessions.verified_at", { time: formatDateTime(s.lastVerifiedAt) }) : t("sessions.never_verified")}
      </p>
      {!cleared && (
        <fieldset className="choice-group" disabled={busy}>
          <legend>{t("sessions.use_for")}</legend>
          {OPS.map((op) => (
            <label key={op} className="choice">
              <input type="checkbox" checked={s.consent.includes(op)} onChange={(e) => void toggle(op, e.target.checked)} data-testid={`session-allow-${op}`} />
              {opName(op)}
            </label>
          ))}
        </fieldset>
      )}
      {!cleared && !s.consent.includes("verify") && (
        <p className="muted small" id={`verify-hint-${s.id}`} data-testid="session-verify-hint">
          {t("sessions.verify_needs_consent")}
        </p>
      )}
      {cleared && <p className="muted small">{t(s.method === "cookie_file" ? "sessions.cleared_file" : s.method === "app_profile" ? "sessions.cleared_app" : "sessions.cleared")}</p>}
      <div className="actions">
        {!cleared && (
          <button
            type="button"
            className="button button-primary"
            disabled={busy || !s.consent.includes("verify")}
            aria-describedby={s.consent.includes("verify") ? undefined : `verify-hint-${s.id}`}
            onClick={() => void verify()}
            data-testid="session-verify"
          >
            {t("sessions.verify")}
          </button>
        )}
        {cleared && s.method === "browser_cookies" && (
          <button type="button" className="button button-primary" disabled={busy} onClick={() => void act(() => client.reconnectSession(s.id, OPS))} data-testid="session-reconnect">
            {t("sessions.reconnect")}
          </button>
        )}
        <button type="button" className="button" disabled={busy} onClick={() => void act(() => client.openSignIn(s.provider))} data-testid="session-sign-in">
          {t("sessions.open_sign_in", { site: platformName(s.provider) })}
        </button>
        {!cleared && (
          <button type="button" className="button" disabled={busy} onClick={() => void act(() => client.clearSession(s.id))} data-testid="session-clear">
            {t("sessions.clear")}
          </button>
        )}
        <button type="button" className="button button-quiet" disabled={busy} onClick={() => void act(() => client.deleteSession(s.id))} data-testid="session-delete">
          {t("sessions.delete")}
        </button>
      </div>
    </li>
  );
}

/**
 * The Sessions screen (STATE.md F5.5, SPEC §9.3): per site, the session's
 * state, when it was last checked, what it may be used for, and Check now,
 * Reconnect, Clear and Remove; sites without a session can be connected or
 * their sign-in page opened in the user's browser.
 */
export function SessionsView({ client, notify }: { client: SessionClient; notify: Notify }) {
  const [overview, setOverview] = useState<SessionOverview | null>(null);
  const [connecting, setConnecting] = useState<Platform | null>(null);
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
    } catch (e) {
      fail(e);
    } finally {
      await load();
      setBusy(false);
    }
  };

  if (!overview) return null;
  const without = overview.providers.filter((p) => !overview.sessions.some((s) => s.provider === p));
  return (
    <div className="downloads sessions-view" data-testid="sessions">
      <p className="muted small">{t("sessions.help")}</p>
      {connecting && (
        <Modal title={t("sessions.connect_title", { site: platformName(connecting) })} onClose={() => setConnecting(null)} testId="session-dialog">
          <ConnectSession
            client={client}
            overview={overview}
            provider={connecting}
            notify={notify}
            onConnected={() => {
              setConnecting(null);
              void load();
            }}
          />
        </Modal>
      )}
      <section aria-labelledby="sessions-connected">
        <h2 id="sessions-connected" className="downloads-heading">
          {t("sessions.connected_sites")} <span className="muted">({overview.sessions.length})</span>
        </h2>
        {overview.sessions.length === 0 ? (
          <p className="muted small" data-testid="sessions-empty">
            {t("sessions.empty")}
          </p>
        ) : (
          <ul className="job-list sessions">
            {overview.sessions.map((s) => (
              <SessionRow key={s.id} s={s} client={client} busy={busy} act={act} notify={notify} />
            ))}
          </ul>
        )}
      </section>
      <section aria-labelledby="sessions-other">
        <h2 id="sessions-other" className="downloads-heading">
          {t("sessions.other_sites")}
        </h2>
        <ul className="job-list sessions">
          {without.map((p) => (
            <li key={p} className="session job" data-testid="session-site" data-provider={p}>
              <div className="job-head">
                <strong>{platformName(p)}</strong>
                <span className="session-state session-state-not_connected">{t("sessions.state.not_connected")}</span>
              </div>
              <div className="actions">
                <button type="button" className="button button-primary" onClick={() => setConnecting(p)} data-testid="session-site-connect">
                  {t("sessions.connect")}
                </button>
                <button type="button" className="button" disabled={busy} onClick={() => void act(() => client.openSignIn(p))} data-testid="session-site-sign-in">
                  {t("sessions.open_sign_in", { site: platformName(p) })}
                </button>
              </div>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}

/** Settings → Sessions: points to the Sessions screen. */
export function SessionsBlock({ onOpen }: { onOpen: () => void }) {
  return (
    <div data-testid="settings-sessions">
      <p className="muted small">{t("sessions.help")}</p>
      <button type="button" className="button" onClick={onOpen} data-testid="settings-open-sessions">
        {t("sessions.open_screen")}
      </button>
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
