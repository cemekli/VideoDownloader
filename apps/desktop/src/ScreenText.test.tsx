import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { ScreenTextBlock } from "./components/ScreenText";
import { MomentLine } from "./components/ItemCard";
import { createFixtureClient } from "./ipc/client";
import { fixtureRecords } from "./ipc/fixture";

// Text on screen (STATE.md F7.4, AC-41, ADR-0039) against the fixture
// backend: AI Processing reads all videos, a search then finds words shown
// on screen, labelled "on screen" apart from speech, and the line plays
// from the time it appeared. Frames, OCR and storage are tested in Rust
// (crates/vv-engine/tests/screen_text.rs, crates/vv-persistence/tests/
// screen_text.rs, apps/desktop/src-tauri/tests/ipc.rs).

const query = (text: string) => ({ text, view: "all" as const, platforms: [], kinds: [], sort: "relevance" as const, descending: false, offset: 0, limit: 50 });

describe("Text on screen", () => {
  it("reads all videos from AI Processing, then a search finds the text with its time", async () => {
    const user = userEvent.setup();
    const client = createFixtureClient();
    const videos = fixtureRecords().filter((r) => !r.removed && r.fileName !== null);
    render(<ScreenTextBlock client={client} notify={vi.fn()} />);
    const block = await screen.findByTestId("screen-text");
    expect(within(block).getByTestId("screen-text-ocr")).toHaveTextContent("Reads Turkish with Windows' own OCR.");
    expect(within(block).getByTestId("screen-text-summary")).toHaveTextContent(`0 of ${videos.length} videos read.`);
    await user.click(within(block).getByTestId("screen-text-read"));
    await waitFor(() => expect(within(block).getByTestId("screen-text-summary")).toHaveTextContent(`${videos.length} of ${videos.length} videos read.`), { timeout: 10_000 });
    expect(within(block).getByTestId("screen-text-read")).toBeDisabled();

    const word = videos[0]!.title.split(" ")[0]!;
    const page = await client.listItems(query(word));
    const hit = page.items.find((i) => i.id === videos[0]!.id)!;
    expect(hit.transcript).toBeNull();
    expect(hit.onScreen?.provenance).toBe("ocr");
    expect(hit.onScreen?.moments[0]?.startMs).toBe(4000);
    const m = await client.getMoment(hit.onScreen!.moments[0]!.segmentId);
    expect(m).toMatchObject({ itemId: videos[0]!.id, startMs: 4000, transcriptId: "" });

    render(<MomentLine item={{ ...hit, snippet: null }} />);
    const line = screen.getByTestId("snippet");
    expect(line).toHaveAttribute("data-field", "on_screen");
    expect(line).toHaveTextContent("▣ on screen 0:04");
  });

  it("the details list on-screen lines under their own heading", async () => {
    const user = userEvent.setup();
    const client = createFixtureClient();
    const video = fixtureRecords().find((r) => !r.removed && r.fileName !== null)!;
    await client.readScreenText(video.id);
    render(<App client={client} />);
    const search = await screen.findByRole("searchbox");
    await user.type(search, video.title.split(" ")[0]!);
    // The card shows where it was read, with the time, marked as on screen.
    const line = await waitFor(() => {
      const card = screen.getAllByTestId("library-card").find((c) => c.dataset.id === video.id)!;
      const l = within(card).getByTestId("snippet");
      expect(l).toHaveAttribute("data-field", "on_screen");
      return l;
    });
    expect(line).toHaveTextContent("on screen 0:04");
    await user.dblClick(line);
    const section = await screen.findByTestId("inspector-on-screen");
    expect(section).toHaveTextContent("On screen");
    expect(within(section).getAllByTestId("moment")[0]).toHaveAttribute("data-start-ms", "4000");
    expect(screen.queryByTestId("inspector-moments")).toBeNull();
    // It plays from the time the text appeared.
    await user.click(within(section).getAllByTestId("moment")[0]!.querySelector("button")!);
    const player = await screen.findByTestId("player");
    expect(await within(player).findByTestId("player-video")).toHaveAttribute("data-start-ms", "4000");
  });

  it("without an OCR language it says how to add one and does not read", async () => {
    const client = createFixtureClient(undefined, "no-ocr");
    render(<ScreenTextBlock client={client} notify={vi.fn()} />);
    expect(await screen.findByTestId("screen-text-unavailable")).toHaveTextContent("Optical character recognition");
    expect(screen.getByTestId("screen-text-read")).toBeDisabled();
    await expect(client.readScreenText(null)).rejects.toMatchObject({ code: "MODEL_MISSING" });
  });
});
