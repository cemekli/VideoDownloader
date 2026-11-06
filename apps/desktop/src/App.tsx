import { useMemo } from "react";
import { Library } from "./components/Library";
import { BackendStatus } from "./components/BackendStatus";
import { createClient } from "./ipc/client";
import type { BackendClient } from "./ipc/types";

export function App({ client }: { client?: BackendClient }) {
  const c = useMemo(() => client ?? createClient(), [client]);
  return (
    <div className="app">
      <header className="topbar">
        <span className="brand">VideoVault</span>
        <BackendStatus client={c} />
      </header>
      <main>
        <h1>Library</h1>
        <Library client={c} />
      </main>
    </div>
  );
}
