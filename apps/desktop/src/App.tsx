import { useMemo } from "react";
import { Library } from "./components/Library";
import { createClient } from "./ipc/client";
import type { LibraryClient } from "./ipc/types";

export function App({ client }: { client?: LibraryClient }) {
  const c = useMemo(() => client ?? createClient(), [client]);
  return (
    <div className="app">
      <header className="topbar">
        <span className="brand">VideoVault</span>
      </header>
      <main>
        <h1>Library</h1>
        <Library client={c} />
      </main>
    </div>
  );
}
