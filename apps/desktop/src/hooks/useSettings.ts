import { useCallback, useEffect, useState } from "react";
import { setLocale } from "../i18n";
import type { BackendClient, Settings, SettingsPatch } from "../ipc/types";
import { applyTheme } from "../theme";

/**
 * App settings from the backend (F2.7–F2.8). The theme and locale apply as
 * soon as they load or change; "system" follows the OS while the app runs.
 */
export function useSettings(client: BackendClient): { settings: Settings | null; update: (p: SettingsPatch) => Promise<void>; replace: (s: Settings) => void } {
  const [settings, setSettings] = useState<Settings | null>(null);
  useEffect(() => {
    let live = true;
    client.getSettings().then(
      (s) => live && setSettings(s),
      () => {},
    );
    return () => {
      live = false;
    };
  }, [client]);
  useEffect(() => {
    if (!settings) return;
    setLocale(settings.locale);
    applyTheme(settings.theme);
    if (settings.theme !== "system" || typeof window.matchMedia !== "function") return;
    const m = window.matchMedia("(prefers-color-scheme: light)");
    const on = () => applyTheme("system");
    m.addEventListener("change", on);
    return () => m.removeEventListener("change", on);
  }, [settings]);
  const update = useCallback(
    async (patch: SettingsPatch) => {
      setSettings(await client.updateSettings(patch));
    },
    [client],
  );
  return { settings, update, replace: setSettings };
}
