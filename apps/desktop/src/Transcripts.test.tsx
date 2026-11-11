import { describe, expect, it } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Transcripts, languageName, sourceLabel } from "./components/Transcripts";
import { createFixtureClient } from "./ipc/client";
import type { TranscriptDto } from "./ipc/types";

// STATE.md F6.2, SPEC AI-A01 against the fixture backend: the site's own
// captions become a transcript that says its language and source; lines are
// plain text with timecodes. The job itself is tested in Rust
// (crates/vv-engine/tests/captions.rs).

const YOUTUBE = "00000000-0000-4000-8000-000000000001";
const INSTAGRAM = "00000000-0000-4000-8000-000000000002";
const LOCAL = "00000000-0000-4000-8000-000000000005";

describe("Details → Transcript", () => {
  it("gets the site captions, shows language and source, and lists the lines", async () => {
    const user = userEvent.setup();
    const client = createFixtureClient();
    render(<Transcripts client={client} itemId={YOUTUBE} />);
    const section = await screen.findByTestId("inspector-transcripts");
    expect(section).toHaveTextContent("No transcript yet");
    await user.click(within(section).getByTestId("captions-fetch"));
    expect(await screen.findByTestId("captions-running")).toBeInTheDocument();
    const row = await screen.findByTestId("transcript-row", {}, { timeout: 4000 });
    expect(row).toHaveTextContent("Turkish");
    expect(row).toHaveTextContent("Site captions (uploader)");
    expect(row).toHaveTextContent("3 lines");
    await user.click(within(row).getByTestId("transcript-show"));
    const lines = await screen.findByTestId("transcript-lines");
    const items = within(lines).getAllByRole("listitem");
    expect(items).toHaveLength(3);
    expect(items[1]).toHaveTextContent("0:03");
    expect(items[1]).toHaveTextContent("Bugün sabah yürüyüşü yapıyoruz.");
    expect(screen.getByTestId("captions-fetch")).toHaveTextContent("Check site captions again");

    await user.click(within(row).getByTestId("transcript-delete"));
    await waitFor(() => expect(screen.queryByTestId("transcript-row")).toBeNull());
  });

  it("says when the site has no captions", async () => {
    const user = userEvent.setup();
    render(<Transcripts client={createFixtureClient()} itemId={INSTAGRAM} />);
    await user.click(await screen.findByTestId("captions-fetch"));
    expect(await screen.findByTestId("captions-none", {}, { timeout: 4000 })).toHaveTextContent("no captions");
  });

  it("offers nothing to fetch for a local file", async () => {
    const client = createFixtureClient();
    render(<Transcripts client={client} itemId={LOCAL} />);
    await screen.findByTestId("inspector-transcripts");
    expect(screen.queryByTestId("captions-fetch")).toBeNull();
    await expect(client.fetchCaptions(LOCAL)).rejects.toMatchObject({ code: "INPUT_REJECTED" });
  });

  it("caption text with markup stays text", async () => {
    const t: TranscriptDto = {
      id: "t",
      itemId: "i",
      language: "en-orig",
      provenance: "source_captions",
      provider: "source_captions",
      captionKind: "automatic",
      modelId: null,
      status: "ready",
      coveredMs: 1,
      segmentCount: 1,
      createdAt: 0,
      updatedAt: 0,
    };
    expect(languageName("en-orig")).toBe("English");
    expect(languageName("xx-not-a-tag-!!")).toBe("xx-not-a-tag-!!");
    expect(sourceLabel(t)).toBe("Site captions (automatic)");
    const client = createFixtureClient();
    client.getTranscript = async () => ({ transcript: t, segments: [{ startMs: 0, endMs: 1, text: "<img src=x onerror=alert(1)>" }], truncated: false });
    client.listTranscripts = async () => ({ itemId: YOUTUBE, transcripts: [t], captionsJob: null, canFetchCaptions: true });
    const user = userEvent.setup();
    const { container } = render(<Transcripts client={client} itemId={YOUTUBE} />);
    await user.click(await screen.findByTestId("transcript-show"));
    expect(await screen.findByTestId("transcript-lines")).toHaveTextContent("<img src=x onerror=alert(1)>");
    expect(container.querySelector("img")).toBeNull();
  });
});
