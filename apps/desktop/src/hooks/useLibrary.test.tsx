import { describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";
import { useLibrary, PAGE_SIZE } from "./useLibrary";
import type { LibraryClient, ListQuery } from "../ipc/types";
import { manyRecords, queryRecords, FIXTURE_NOW } from "../ipc/fixture";

// A thumbnail or title arriving reloads the list. The reload keeps the pages
// already loaded (the grid does not shrink and jump) and keeps the same data
// object when nothing changed (nothing redraws).
describe("useLibrary reloads", () => {
  const records = manyRecords(450);
  const listItems = vi.fn(async (q: ListQuery) => queryRecords(records, q, FIXTURE_NOW));
  const client = { listItems } as unknown as LibraryClient;
  const query: Omit<ListQuery, "offset" | "limit"> = { text: "", view: "all", platforms: [], kinds: [], sort: "added", descending: true };

  it("keeps every loaded page and the same data when nothing changed", async () => {
    const { result, rerender } = renderHook(({ k }) => useLibrary(client, query, k), { initialProps: { k: 0 } });
    await waitFor(() => expect(result.current.data.status).toBe("ready"));
    act(() => result.current.loadMore());
    await waitFor(() => expect(result.current.data.status === "ready" && result.current.data.items.length).toBe(2 * PAGE_SIZE));
    act(() => result.current.loadMore());
    await waitFor(() => expect(result.current.data.status === "ready" && result.current.data.items.length).toBe(3 * PAGE_SIZE));
    const before = result.current.data;
    listItems.mockClear();
    rerender({ k: 1 });
    await waitFor(() => expect(listItems).toHaveBeenCalledTimes(3));
    await new Promise((r) => setTimeout(r, 20));
    expect(result.current.data).toBe(before);
  });
});
