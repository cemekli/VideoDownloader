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
      modelVersion: null,
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
    client.getTranscript = async () => ({ transcript: t, segments: [{ id: "s1", startMs: 0, endMs: 1, text: "<img src=x onerror=alert(1)>" }], truncated: false });
    client.listTranscripts = async () => ({ itemId: YOUTUBE, transcripts: [t], captionsJob: null, canFetchCaptions: true, transcribeJob: null, transcribeModel: null, transcribeElapsedMs: null, local: { ready: false, reason: "no-media", models: [], languages: ["auto", "tr", "en"], memory: null, diskNeedBytes: null, freeBytes: null } });
    const user = userEvent.setup();
    const { container } = render(<Transcripts client={client} itemId={YOUTUBE} />);
    await user.click(await screen.findByTestId("transcript-show"));
    expect(await screen.findByTestId("transcript-lines")).toHaveTextContent("<img src=x onerror=alert(1)>");
    expect(container.querySelector("img")).toBeNull();
  });

  it("a partial transcript says how far it goes (F6.3, AC-23)", async () => {
    const t: TranscriptDto = {
      id: "p",
      itemId: LOCAL,
      language: "tr",
      provenance: "generated_local",
      provider: "whisper_cpp",
      captionKind: null,
      modelId: "whisper-medium",
      modelVersion: "5359861c739e955e79d9a303bcbc70fb988958b1",
      status: "partial",
      coveredMs: 600_000,
      segmentCount: 120,
      createdAt: 0,
      updatedAt: 0,
    };
    const client = createFixtureClient();
    client.listTranscripts = async () => ({ itemId: LOCAL, transcripts: [t], captionsJob: null, canFetchCaptions: false, transcribeJob: null, transcribeModel: null, transcribeElapsedMs: null, local: { ready: false, reason: "no-model", models: [], languages: ["auto", "tr", "en"], memory: null, diskNeedBytes: null, freeBytes: null } });
    render(<Transcripts client={client} itemId={LOCAL} />);
    const row = await screen.findByTestId("transcript-row");
    expect(row).toHaveTextContent("120 lines · partial, up to 10:00");
    expect(row).toHaveTextContent("Transcribed on this PC (Medium)");
    expect(row).toHaveTextContent("Model: Medium (whisper.cpp, revision 5359861)");
  });

  it("transcribes a local file on this PC and shows the model as the source (F6.2, AI-A02)", async () => {
    const user = userEvent.setup();
    const client = createFixtureClient();
    render(<Transcripts client={client} itemId={LOCAL} />);
    const section = await screen.findByTestId("inspector-transcripts");
    expect(section).toHaveTextContent("transcribe the file on this PC");
    await user.selectOptions(within(section).getByTestId("transcribe-language"), "tr");
    await user.click(within(section).getByTestId("transcribe-start"));
    expect(await screen.findByTestId("transcribe-running")).toBeInTheDocument();
    expect(await screen.findByText(/Transcribing on this PC with Base… 50%/, {}, { timeout: 4000 })).toBeInTheDocument();
    expect(screen.getByTestId("transcribe-timing")).toHaveTextContent(/elapsed, about \d+:\d\d left/);
    const row = await screen.findByTestId("transcript-row", {}, { timeout: 4000 });
    expect(row).toHaveTextContent("Turkish");
    expect(row).toHaveTextContent("Transcribed on this PC (Base)");
    expect(row).toHaveTextContent("complete");
    expect(screen.queryByTestId("transcribe-running")).toBeNull();
    expect(await screen.findByTestId("transcribe-last-run")).toHaveTextContent(/Last run: Base, took \d+:\d\d\./);
  });

  it("a transcription can be cancelled", async () => {
    const user = userEvent.setup();
    render(<Transcripts client={createFixtureClient()} itemId={LOCAL} />);
    await user.click(await screen.findByTestId("transcribe-start"));
    await user.click(await screen.findByTestId("transcribe-cancel"));
    await waitFor(() => expect(screen.queryByTestId("transcribe-running")).toBeNull());
    expect(screen.queryByTestId("transcript-row")).toBeNull();
    expect(screen.getByTestId("transcribe-start")).toBeEnabled();
  });

  it("explains what is missing for local transcription", async () => {
    const client = createFixtureClient();
    // A bookmark: nothing to transcribe, no button.
    render(<Transcripts client={client} itemId={INSTAGRAM} />);
    await screen.findByTestId("inspector-transcripts");
    expect(screen.queryByTestId("transcribe-start")).toBeNull();
    await expect(client.transcribeLocally(INSTAGRAM, "whisper-base", "auto")).rejects.toMatchObject({ code: "FILE_MISSING" });
    await expect(client.transcribeLocally(LOCAL, "whisper-huge", "auto")).rejects.toMatchObject({ code: "MODEL_MISSING" });

    const base = await client.listTranscripts(LOCAL);
    for (const [reason, text] of [
      ["no-model", "download a model in AI Processing"],
      ["no-tool", "local transcriber is missing"],
    ] as const) {
      const c = createFixtureClient();
      c.listTranscripts = async () => ({ ...base, local: { ...base.local, ready: false, reason, models: reason === "no-model" ? [] : base.local.models } });
      const { unmount } = render(<Transcripts client={c} itemId={LOCAL} />);
      expect(await screen.findByTestId("transcribe-hint")).toHaveTextContent(text);
      expect(screen.queryByTestId("transcribe-start")).toBeNull();
      unmount();
    }
  });
});

describe("low memory (F6.6, AI-A05)", () => {
  it("does not pick a model the PC cannot hold, and runs it only after 'use it anyway'", async () => {
    const user = userEvent.setup();
    // A 4 GB PC with Base and Medium installed; Medium needs about 2.7 GB plus the system.
    const client = createFixtureClient(undefined, "low-memory");
    render(<Transcripts client={client} itemId={LOCAL} />);
    const section = await screen.findByTestId("inspector-transcripts");
    const select = within(section).getByTestId<HTMLSelectElement>("transcribe-model");
    // Default: the largest model that fits, not the largest installed.
    expect(select.value).toBe("whisper-base");
    expect(within(section).queryByTestId("transcribe-memory")).toBeNull();
    expect(within(section).getByRole("option", { name: "Medium (needs more memory)" })).toBeInTheDocument();

    await user.selectOptions(select, "whisper-medium");
    const warn = within(section).getByTestId("transcribe-memory");
    expect(warn).toHaveAttribute("data-fit", "too_big");
    expect(warn).toHaveTextContent(/Medium needs about 2\.\d GB of memory and this PC has 4(\.0)? GB in all/);
    expect(within(section).getByTestId("transcribe-start")).toBeDisabled();
    // The backend refuses it without the user's choice, too.
    await expect(client.transcribeLocally(LOCAL, "whisper-medium", "auto")).rejects.toMatchObject({ code: "RESOURCE_LIMIT" });

    await user.click(within(section).getByTestId("transcribe-anyway"));
    await user.click(within(section).getByTestId("transcribe-start"));
    expect(await screen.findByText(/with Medium/, {}, { timeout: 4000 })).toBeInTheDocument();
  });

  it("blocks the start when there is no room for the audio", async () => {
    const client = createFixtureClient();
    const base = await client.listTranscripts(LOCAL);
    client.listTranscripts = async () => ({ ...base, local: { ...base.local, diskNeedBytes: 500_000_000, freeBytes: 100_000_000 } });
    render(<Transcripts client={client} itemId={LOCAL} />);
    expect(await screen.findByTestId("transcribe-disk")).toHaveTextContent("Not enough free space for the audio");
    expect(screen.getByTestId("transcribe-start")).toBeDisabled();
  });
});

describe("captions job state", () => {
  it("names waiting, retrying and blocked instead of one spinner text", async () => {
    const { captionsJobText } = await import("./components/Transcripts");
    const base = (await createFixtureClient().fetchCaptions(YOUTUBE)).captionsJob!;
    expect(captionsJobText({ ...base, status: "queued" })).toBe("Waiting to start…");
    expect(captionsJobText({ ...base, status: "running", attempts: 2, maxAttempts: 4 })).toContain("attempt 2 of 4");
    expect(captionsJobText({ ...base, status: "retry_wait", errorCode: "TIMEOUT", nextRetryAt: 1_794_000_000_000 })).toContain("too long");
    expect(captionsJobText({ ...base, status: "blocked", blockedReason: "offline" })).not.toContain("Getting");
  });
});
