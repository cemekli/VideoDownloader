import { describe, expect, it } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { createFixtureClient } from "./ipc/client";
import { currentIndex, usableTranscripts } from "./components/TranscriptPanel";
import type { TranscriptDto } from "./ipc/types";

// STATE.md F6.5 (PLAY-06, DATA-11, AC-22): the transcript beside the player
// marks the line being said, a timecode seeks there, and picked lines go
// into a Markdown note that is shown before it is saved. The `transcripts`
// fixture gives item 0001 (it has a file) the Turkish captions fixture.

async function openPlayer() {
  const user = userEvent.setup();
  render(<App client={createFixtureClient(undefined, "transcripts")} />);
  const card = (await screen.findAllByTestId("library-card")).find((c) => c.dataset.id?.endsWith("0001"))!;
  fireEvent.doubleClick(card);
  const inspector = await screen.findByTestId("inspector");
  await user.click(await within(inspector).findByTestId("inspector-play"));
  const player = await screen.findByTestId("player");
  const panel = await within(player).findByTestId("player-transcript");
  await waitFor(() => expect(within(panel).getByTestId("panel-lines").children.length).toBe(8));
  return { user, player, panel, video: within(player).getByTestId("player-video") as HTMLVideoElement };
}

describe("Transcript beside the player", () => {
  it("finds the line being said", () => {
    const segs = [0, 3000, 9000].map((s, i) => ({ id: String(i), startMs: s, endMs: s + 2000, text: "x" }));
    expect(currentIndex(segs, -1)).toBe(-1);
    expect(currentIndex(segs, 0)).toBe(0);
    expect(currentIndex(segs, 5000)).toBe(1);
    expect(currentIndex(segs, 99_000)).toBe(2);
    const t = (id: string, status: TranscriptDto["status"], updatedAt: number) => ({ id, status, updatedAt }) as TranscriptDto;
    expect(usableTranscripts([t("a", "partial", 9), t("b", "ready", 1), t("c", "failed", 10), t("d", "ready", 5)]).map((x) => x.id)).toEqual(["d", "b", "a"]);
  });

  it("marks the line being said and seeks to a line's start when its time is clicked", async () => {
    const { user, panel, video } = await openPlayer();
    const lines = within(panel).getByTestId("panel-lines");
    video.currentTime = 4;
    fireEvent.timeUpdate(video);
    await waitFor(() => expect(lines.querySelector('[aria-current="true"]')).toHaveAttribute("data-start-ms", "3040"));
    await user.click(within(lines).getByRole("button", { name: /Play from 0:12/ }));
    expect(video.currentTime).toBeCloseTo(12.76, 2);
    fireEvent.timeUpdate(video);
    await waitFor(() => expect(lines.querySelector('[aria-current="true"]')).toHaveAttribute("data-start-ms", "12760"));
  });

  it("picked lines become a Markdown note, in time order, shown before saving", async () => {
    const { user, panel } = await openPlayer();
    await user.click(within(panel).getByRole("checkbox", { name: "Add 0:12 to the note" }));
    await user.click(within(panel).getByRole("checkbox", { name: "Add 0:03 to the note" }));
    expect(within(panel).getByTestId("note-count")).toHaveTextContent("2 lines picked for the note");
    await user.click(within(panel).getByTestId("note-export"));
    const md = (await within(panel).findByTestId("note-markdown")).textContent!;
    expect(md).toContain("# PostgreSQL replication deep dive");
    expect(md).toContain("Source: <https://www.youtube.com/watch?v=aaaaaaaaaaa>");
    expect(md.indexOf("0:03")).toBeLessThan(md.indexOf("0:12"));
    expect(md).toContain("Kadıköy");
    await user.click(within(panel).getByTestId("note-save"));
    const toasts = await screen.findAllByTestId("toast");
    expect(toasts.at(-1)).toHaveTextContent(/Saved the note as PostgreSQL replication deep dive - note\.md/);
    expect(within(panel).queryByTestId("note-preview")).toBeNull();
  });

  it("no transcript: the player stays as it was", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    const card = (await screen.findAllByTestId("library-card")).find((c) => c.dataset.id?.endsWith("0001"))!;
    fireEvent.doubleClick(card);
    const inspector = await screen.findByTestId("inspector");
    await user.click(await within(inspector).findByTestId("inspector-play"));
    const player = await screen.findByTestId("player");
    await within(player).findByTestId("player-video");
    expect(within(player).queryByTestId("player-transcript")).toBeNull();
    expect(player.className).not.toContain("modal-player");
  });
});
