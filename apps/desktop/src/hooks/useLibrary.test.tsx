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

// F7.2 (ADR-0036): the word result comes first; when meaning is
// `available` the same query is asked with `meaning: true` and the fused
// result replaces it. Later pages keep the flag; a failing API leaves the
// word result with "failed".
describe("useLibrary search by meaning", () => {
  const records = manyRecords(150);
  const query: Omit<ListQuery, "offset" | "limit"> = { text: "video", view: "all", platforms: [], kinds: [], sort: "relevance", descending: false };
  const make = (second: "used" | "throw") => {
    const listItems = vi.fn(async (q: ListQuery) => {
      const page = queryRecords(records, { ...q, text: "" }, FIXTURE_NOW);
      if (!q.meaning) return { ...page, meaning: "available" as const };
      if (second === "throw") throw new Error("api down");
      return { ...page, items: [...page.items].reverse(), meaning: "used" as const };
    });
    return { listItems, client: { listItems } as unknown as LibraryClient };
  };

  it("shows words first, then the result with meaning; more pages keep asking for it", async () => {
    const { listItems, client } = make("used");
    const { result } = renderHook(() => useLibrary(client, query, 0));
    await waitFor(() => expect(result.current.data.status === "ready" && result.current.data.meaning).toBe("used"));
    expect(listItems.mock.calls[0]![0].meaning).toBeUndefined();
    expect(listItems.mock.calls[1]![0].meaning).toBe(true);
    listItems.mockClear();
    act(() => result.current.loadMore());
    await waitFor(() => expect(result.current.data.status === "ready" && result.current.data.items.length).toBe(150));
    expect(listItems.mock.calls[0]![0]).toMatchObject({ meaning: true, offset: PAGE_SIZE });
  });

  it("keeps the word result when the API fails", async () => {
    const { client } = make("throw");
    const { result } = renderHook(() => useLibrary(client, query, 0));
    await waitFor(() => expect(result.current.data.status === "ready" && result.current.data.meaning).toBe("failed"));
    expect(result.current.data.status === "ready" && result.current.data.items.length).toBe(PAGE_SIZE);
  });
});
