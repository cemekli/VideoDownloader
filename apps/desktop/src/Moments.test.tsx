import { describe, expect, it } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { createFixtureClient } from "./ipc/client";
import { fixtureRecords } from "./ipc/fixture";

// STATE.md F6.4 (SEA-06, PLAY-06, AC-22, §16.2): words said in a video find the
// item; its moments are listed under it and play from the segment's start.
// The `transcripts` fixture gives the first YouTube item (it has a file)
// the self-authored Turkish captions (tests/fixtures/captions/).

const ITEM = "00000000-0000-4000-8000-000000000001";

async function search(text: string) {
  const client = createFixtureClient(undefined, "transcripts");
  render(<App client={client} />);
  await screen.findAllByTestId("library-card");
  await userEvent.type(screen.getByTestId("search-input"), text);
  return client;
}

describe("Search in transcripts", () => {
  it("finds an item by what was said and shows the moment's time on the card", async () => {
    await search("Kadıköy");
    await waitFor(() => expect(screen.getAllByTestId("library-card")).toHaveLength(1));
    const card = screen.getByTestId("library-card");
    expect(card).toHaveAttribute("data-id", ITEM);
    const line = within(card).getByTestId("snippet");
    expect(line).toHaveAttribute("data-field", "transcript");
    expect(line.querySelector(".timecode")).toHaveTextContent("0:03");
    expect(line.querySelector("mark")).toHaveTextContent("Kadıköy");
  });

  it("Turkish folding: ISIK finds ışık and IŞIK, as written", async () => {
    await search("ISIK");
    const card = (await screen.findAllByTestId("library-card")).find((c) => c.dataset.id === ITEM)!;
    await waitFor(() => expect(within(card).getByTestId("snippet")).toHaveAttribute("data-field", "transcript"));
    const marks = [...within(card).getByTestId("snippet").querySelectorAll("mark")].map((m) => m.textContent);
    expect(marks).toEqual(["IŞIK", "ışık"]);
  });

  it("lists the moments in the details and plays from the moment's segment", async () => {
    const user = userEvent.setup();
    await search("Iğdır");
    await waitFor(() => expect(screen.getAllByTestId("library-card")).toHaveLength(1));
    await user.dblClick(screen.getByTestId("library-card"));
    const moments = await screen.findByTestId("inspector-moments");
    expect(moments).toHaveTextContent("Said in the video");
    expect(moments).toHaveTextContent("Turkish · Site captions");
    const list = within(moments).getAllByTestId("moment");
    expect(list).toHaveLength(1);
    expect(list[0]).toHaveAttribute("data-start-ms", "12760");
    await user.click(within(list[0]!).getByRole("button", { name: /Play from 0:12/ }));
    const player = await screen.findByTestId("player");
    const video = await within(player).findByTestId("player-video");
    expect(video).toHaveAttribute("data-start-ms", "12760");
    // A moment starts the video there; no "Resume from" offer.
    expect(within(player).queryByTestId("player-resume")).toBeNull();
  });

  it("a moment whose transcript changed says so instead of seeking", async () => {
    const user = userEvent.setup();
    const client = await search("Kadıköy");
    await waitFor(() => expect(screen.getAllByTestId("library-card")).toHaveLength(1));
    await user.dblClick(screen.getByTestId("library-card"));
    const moments = await screen.findByTestId("inspector-moments");
    const tid = (await client.listTranscripts(ITEM)).transcripts[0]!.id;
    await client.deleteTranscript(tid);
    await user.click(within(moments).getAllByRole("button")[0]!);
    expect(await screen.findByText(/This transcript changed after the search/)).toBeInTheDocument();
    expect(screen.queryByTestId("player")).toBeNull();
  });

  it("an item without a file lists its moments as text, without play buttons", async () => {
    const records = fixtureRecords();
    records.find((r) => r.id === ITEM)!.localState = "bookmark_only";
    records.find((r) => r.id === ITEM)!.fileName = null;
    const client = createFixtureClient(records, "transcripts");
    const user = userEvent.setup();
    render(<App client={client} />);
    await screen.findAllByTestId("library-card");
    await user.type(screen.getByTestId("search-input"), "Kadıköy");
    await waitFor(() => expect(screen.getAllByTestId("library-card")).toHaveLength(1));
    await user.dblClick(screen.getByTestId("library-card"));
    const moments = await screen.findByTestId("inspector-moments");
    expect(within(moments).getAllByTestId("moment")).toHaveLength(1);
    expect(within(moments).queryByRole("button")).toBeNull();
  });
});
