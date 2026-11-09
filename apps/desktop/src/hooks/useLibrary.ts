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
  const dataRef = useRef(data);
  dataRef.current = data;

  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    const gen = ++generation.current;
    // A reload of the same query (a thumbnail or title arrived) keeps the
    // pages already loaded, so the grid does not shrink back to the first
    // page and jump while the user looks at it.
    const same = lastKey.current === key;
    lastKey.current = key;
    const cur = dataRef.current;
    const want = same && cur.status === "ready" ? Math.max(PAGE_SIZE, cur.items.length) : PAGE_SIZE;
    if (!same) setData((d) => (d.status === "ready" ? d : { status: "loading" }));
    const load = async () => {
      // Pages of PAGE_SIZE (the backend caps one page) up to `want` items.
      const first = await client.listItems({ ...query, offset: 0, limit: PAGE_SIZE });
      const items = [...first.items];
      while (items.length < Math.min(want, first.total) && gen === generation.current) {
        const next = await client.listItems({ ...query, offset: items.length, limit: PAGE_SIZE });
        if (next.items.length === 0) break;
        items.push(...next.items);
      }
      return { ...first, items };
    };
    load()
      .then((p) => {
        if (gen !== generation.current) return;
        setData((d) =>
          d.status === "ready" && d.total === p.total && d.notice === p.notice && sameItems(d.items, p.items)
            ? d
            : { status: "ready", items: p.items, total: p.total, notice: p.notice, loadingMore: false },
        );
      })
      .catch((e: unknown) => gen === generation.current && setData({ status: "error", code: errorCode(e) }));
    // `key` stands for `query`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client, key, reloadKey]);

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

/** Same items in the same order with the same content: nothing to redraw. */
function sameItems(a: LibraryItemDto[], b: LibraryItemDto[]): boolean {
  return a.length === b.length && a.every((x, i) => JSON.stringify(x) === JSON.stringify(b[i]));
}
