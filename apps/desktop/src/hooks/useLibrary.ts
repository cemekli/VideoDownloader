import { useCallback, useEffect, useRef, useState } from "react";
import type { LibraryClient, LibraryItemDto, ListQuery } from "../ipc/types";
import { errorCode } from "../ipc/errors";

export const PAGE_SIZE = 100;

export type LibraryData =
  | { status: "loading" }
  | { status: "error"; code: string }
  | { status: "ready"; items: LibraryItemDto[]; total: number; notice: string | null; loadingMore: boolean };

/**
 * Pages of `list_items` for one query. A new query starts over; answers to
 * an older query are dropped. `loadMore` fetches the next page while the
 * grid scrolls (SPEC §9.2 large library).
 */
export function useLibrary(client: LibraryClient, query: Omit<ListQuery, "offset" | "limit">, reloadKey: number) {
  const [data, setData] = useState<LibraryData>({ status: "loading" });
  const generation = useRef(0);
  const key = JSON.stringify(query);

  useEffect(() => {
    const gen = ++generation.current;
    setData((d) => (d.status === "ready" ? d : { status: "loading" }));
    client
      .listItems({ ...query, offset: 0, limit: PAGE_SIZE })
      .then((p) => gen === generation.current && setData({ status: "ready", items: p.items, total: p.total, notice: p.notice, loadingMore: false }))
      .catch((e: unknown) => gen === generation.current && setData({ status: "error", code: errorCode(e) }));
    // `key` stands for `query`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client, key, reloadKey]);

  const dataRef = useRef(data);
  dataRef.current = data;
  const queryRef = useRef(query);
  queryRef.current = query;

  const loadMore = useCallback(() => {
    const d = dataRef.current;
    if (d.status !== "ready" || d.loadingMore || d.items.length >= d.total) return;
    const gen = generation.current;
    const offset = d.items.length;
    const next = { ...d, loadingMore: true };
    dataRef.current = next;
    setData(next);
    client
      .listItems({ ...queryRef.current, offset, limit: PAGE_SIZE })
      .then((p) =>
        setData((cur) =>
          gen !== generation.current || cur.status !== "ready" || cur.items.length !== offset
            ? cur
            : { ...cur, items: [...cur.items, ...p.items], total: p.total, loadingMore: false },
        ),
      )
      .catch(() => setData((cur) => (cur.status === "ready" ? { ...cur, loadingMore: false } : cur)));
  }, [client]);

  return { data, loadMore };
}
