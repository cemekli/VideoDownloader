import { useEffect, useState } from "react";
import type { LibraryClient, LibraryItemDto, LocalState } from "../ipc/types";
import { formatDuration } from "../format";

const LOCAL_STATE_LABEL: Record<LocalState, string> = {
  bookmark_only: "Bookmark only",
  local_ready: "Downloaded",
  missing: "File missing",
};

type LoadState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; items: LibraryItemDto[] };

export function Library({ client }: { client: LibraryClient }) {
  const [state, setState] = useState<LoadState>({ status: "loading" });

  useEffect(() => {
    let alive = true;
    client
      .listItems()
      .then((items) => alive && setState({ status: "ready", items }))
      .catch((e: unknown) => alive && setState({ status: "error", message: e instanceof Error ? e.message : String(e) }));
    return () => {
      alive = false;
    };
  }, [client]);

  if (state.status === "loading") {
    return <p role="status">Loading library…</p>;
  }
  if (state.status === "error") {
    return (
      <p role="alert" data-testid="library-error">
        Could not load the library: {state.message}
      </p>
    );
  }
  if (state.items.length === 0) {
    return <p data-testid="library-empty">Your library is empty. Add a URL or import a video to get started.</p>;
  }
  return (
    <ul className="library-grid" aria-label="Library items">
      {state.items.map((item) => (
        <li key={item.id} className="card" data-testid="library-card" data-local-state={item.localState} tabIndex={0}>
          <h2 className="card-title" title={item.title}>
            {item.title}
          </h2>
          <div className="card-meta">
            <span className="platform">{item.platform}</span>
            <span className={`badge badge-${item.localState}`} data-testid="local-state">
              {LOCAL_STATE_LABEL[item.localState]}
            </span>
            <span className="duration" data-testid="duration">
              {formatDuration(item.durationMs)}
            </span>
          </div>
        </li>
      ))}
    </ul>
  );
}
