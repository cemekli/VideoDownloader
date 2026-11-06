import { useEffect, useState } from "react";
import type { AppStatus, StatusClient } from "../ipc/types";
import { CONTRACT_VERSION } from "../ipc/client";

type State = { kind: "checking" } | { kind: "ok"; status: AppStatus } | { kind: "mismatch"; status: AppStatus } | { kind: "down" };

/**
 * Small top-bar badge: is the backend reachable, and is this fixture data?
 * A contract version mismatch is shown instead of silently misreading DTOs.
 */
export function BackendStatus({ client }: { client: StatusClient }) {
  const [state, setState] = useState<State>({ kind: "checking" });

  useEffect(() => {
    let live = true;
    client
      .getStatus()
      .then((status) => live && setState({ kind: status.contractVersion === CONTRACT_VERSION ? "ok" : "mismatch", status }))
      .catch(() => live && setState({ kind: "down" }));
    return () => {
      live = false;
    };
  }, [client]);

  const label =
    state.kind === "checking"
      ? "Connecting…"
      : state.kind === "down"
        ? "Backend unavailable"
        : state.kind === "mismatch"
          ? `Version mismatch (backend v${state.status.contractVersion}, UI v${CONTRACT_VERSION})`
          : client.kind === "fixture"
            ? "Fixture data"
            : `Desktop v${state.status.appVersion}`;

  return (
    <span className="backend-status" data-testid="backend-status" data-state={state.kind} data-backend={client.kind} role="status" aria-live="polite">
      {label}
    </span>
  );
}
