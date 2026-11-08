import { useEffect, useRef, useState } from "react";
import { errorCode } from "../ipc/errors";
import type { BackendClient, Settings, SettingsPatch, StorageOverview } from "../ipc/types";
import { t } from "../i18n";
import { errorMessage } from "../labels";
import { LegalNotice, StorageLine, ThemeChoice } from "./SettingsDialog";

const STEPS = ["welcome", "legal", "storage", "updates", "theme"] as const;
type Step = (typeof STEPS)[number];

/**
 * First run (SPEC §9.7; STATE.md F2.8): five short steps, every one
 * skippable. Skipping keeps the documented defaults (dark theme, manual
 * update checks, default media folder). Nothing here asks for an account,
 * a donation, a model download or a sign-in.
 */
export function Onboarding({ client, settings, onSettings, onDone, notify }: { client: BackendClient; settings: Settings; onSettings: (p: SettingsPatch) => Promise<void>; onDone: (s: Settings) => void; notify: (text: string, tone?: "info" | "error") => void }) {
  const [step, setStep] = useState<Step>("welcome");
  const [storage, setStorage] = useState<StorageOverview | null>(null);
  const [acked, setAcked] = useState(settings.legalAckAt !== null);
  const heading = useRef<HTMLHeadingElement>(null);
  const i = STEPS.indexOf(step);

  useEffect(() => {
    client.getStorageOverview().then(setStorage, () => {});
  }, [client]);
  // Each step announces itself: focus moves to its heading.
  useEffect(() => heading.current?.focus(), [step]);

  const finish = async () => {
    try {
      onDone(await client.finishOnboarding());
    } catch (e) {
      notify(errorMessage(errorCode(e)), "error");
    }
  };
  const next = () => (i === STEPS.length - 1 ? void finish() : setStep(STEPS[i + 1]!));

  return (
    <div className="onboarding" role="dialog" aria-modal="true" aria-label={t("onboarding.label")} data-testid="onboarding" data-step={step}>
      <div className="onboarding-card">
        <p className="muted small" aria-live="polite">
          {t("onboarding.step", { n: i + 1, total: STEPS.length })}
        </p>
        <h1 ref={heading} tabIndex={-1} data-testid="onboarding-title">
          {t(`onboarding.${step}.title`)}
        </h1>
        {step === "welcome" && <p>{t("onboarding.welcome.body")}</p>}
        {step === "legal" && (
          <>
            <LegalNotice />
            {acked ? (
              <p className="muted small" role="status" data-testid="legal-acked">
                {t("onboarding.legal.done")}
              </p>
            ) : (
              <button
                type="button"
                className="button button-primary"
                onClick={async () => {
                  try {
                    await client.acknowledgeLegal();
                    setAcked(true);
                    setStep("storage");
                  } catch (e) {
                    notify(errorMessage(errorCode(e)), "error");
                  }
                }}
                data-testid="legal-ack"
              >
                {t("onboarding.legal.ack")}
              </button>
            )}
          </>
        )}
        {step === "storage" && (
          <>
            <p>{t("onboarding.storage.body")}</p>
            <StorageLine
              storage={storage}
              onChoose={() =>
                client.chooseMediaFolder().then(
                  (s) => s && setStorage(s),
                  (e) => notify(errorMessage(errorCode(e)), "error"),
                )
              }
            />
          </>
        )}
        {step === "updates" && (
          <>
            <p>{t("onboarding.updates.body")}</p>
            <fieldset className="choice-group">
              <legend>{t("settings.updates")}</legend>
              {(["manual", "automatic"] as const).map((u) => (
                <label key={u} className="choice">
                  <input type="radio" name="onboarding-updates" checked={settings.updateCheck === u} onChange={() => void onSettings({ updateCheck: u })} data-testid={`onboarding-updates-${u}`} />
                  {t(`settings.updates.${u}`)}
                </label>
              ))}
            </fieldset>
            <p className="muted">{t("onboarding.updates.offline")}</p>
          </>
        )}
        {step === "theme" && (
          <>
            <p>{t("onboarding.theme.body")}</p>
            <ThemeChoice name="onboarding-theme" value={settings.theme} onChange={(theme) => void onSettings({ theme })} />
          </>
        )}
        <div className="onboarding-actions">
          <button type="button" className="link-button" onClick={() => void finish()} data-testid="onboarding-skip">
            {t("common.skip")}
          </button>
          <span className="spacer" />
          {i > 0 && (
            <button type="button" className="button" onClick={() => setStep(STEPS[i - 1]!)} data-testid="onboarding-back">
              {t("common.back")}
            </button>
          )}
          {!(step === "legal" && !acked) && (
            <button type="button" className="button button-primary" onClick={next} data-testid="onboarding-next">
              {i === STEPS.length - 1 ? t("onboarding.finish") : t("common.next")}
            </button>
          )}
          {step === "legal" && !acked && (
            <button type="button" className="button" onClick={next} data-testid="onboarding-next">
              {t("common.next")}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
