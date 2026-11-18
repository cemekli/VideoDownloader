import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import { MomentLine } from "./components/ItemCard";
import { Moments } from "./components/Moments";
import type { LibraryItemDto, TranscriptMatchesDto } from "./ipc/types";
import { createFixtureClient } from "./ipc/client";

// F7.2: an item found only by meaning shows its closest passage with the
// time, marked "≈ in meaning", and nothing highlighted (no word matched).
// The details list such passages under their own heading.

const matches: TranscriptMatchesDto = {
  transcriptId: "t1",
  language: "tr",
  provenance: "generated_local",
  partial: false,
  coveredMs: null,
  total: 2,
  moments: [
    { segmentId: "s1", startMs: 65_000, endMs: 70_000, snippet: { field: "meaning", parts: [{ text: "PostgreSQL replikasyonu nasıl kurulur", highlight: false }], cutStart: false, cutEnd: true } },
    { segmentId: "s2", startMs: 125_000, endMs: 130_000, snippet: { field: "meaning", parts: [{ text: "yedekleri başka sunucuya almak", highlight: false }], cutStart: false, cutEnd: false } },
  ],
};

async function item(): Promise<LibraryItemDto> {
  const page = await createFixtureClient().listItems({ text: "", view: "all", platforms: [], kinds: [], sort: "added", descending: true, offset: 0, limit: 1 });
  return { ...page.items[0]!, snippet: null, transcript: null, meaning: matches, foundBy: { words: null, meaning: 1 } };
}

describe("Results found by meaning", () => {
  it("card: the closest passage with its time, marked as meaning", async () => {
    render(<MomentLine item={await item()} />);
    const line = screen.getByTestId("snippet");
    expect(line).toHaveAttribute("data-field", "meaning");
    expect(line).toHaveTextContent("≈ in meaning 1:05 PostgreSQL replikasyonu nasıl kurulur… +1 more");
    expect(line.querySelector("mark")).toBeNull();
  });

  it("details: a 'Close in meaning' list that plays from each passage", async () => {
    const open = vi.fn();
    render(<Moments matches={matches} meaning playable onOpen={open} />);
    const section = screen.getByTestId("inspector-meaning");
    expect(section).toHaveTextContent("Close in meaning");
    const list = within(section).getAllByTestId("moment");
    expect(list).toHaveLength(2);
    list[1]!.querySelector("button")!.click();
    expect(open).toHaveBeenCalledWith("s2");
  });
});
