import { describe, expect, it } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { createFixtureClient } from "./ipc/client";
import { fixtureRecords } from "./ipc/fixture";
import { SuggestionReview } from "./components/SuggestionReview";

// Suggested tags review (F7.3 follow-up, ADR-0038) against the fixture
// backend: suggestions grouped by value, one tag for many videos at once,
// one undoable "add tag"; rejecting many asks first. The rules are tested
// in Rust (crates/vv-persistence/tests/proposal_groups.rs,
// apps/desktop/src-tauri/tests/ipc.rs).

/** A fixture client whose live items all have suggestions (from their title words). */
async function suggested() {
  const recs = fixtureRecords();
  const client = createFixtureClient(recs);
  await client.setSuggestApi("http://localhost:11434/v1", "qwen3.5:4b", null, false);
  for (const r of recs.filter((x) => !x.removed)) await client.suggestTags(r.id, false);
  return { client, recs };
}

// "Grup toplantısı kaydı" and "Konser kaydı (harici disk)" share "kaydı".
const shared = (recs: ReturnType<typeof fixtureRecords>) => recs.filter((r) => !r.removed && /kaydı/.test(r.title ?? ""));

async function openReview() {
  const user = userEvent.setup();
  await user.click(await screen.findByTestId("nav-suggestions"));
  const review = await screen.findByTestId("suggestion-review");
  // Looked up afresh: the list re-renders after each decision.
  const group = (value: string) => screen.queryAllByTestId("review-group").find((g) => g.dataset.value === value);
  return { user, review, group };
}

describe("Suggested tags review", () => {
  it("adds one tag to every video of a group at once, and Undo takes it off", async () => {
    const { client, recs } = await suggested();
    const both = shared(recs);
    expect(both).toHaveLength(2);
    render(<App client={client} />);
    // The sidebar counts what is waiting.
    const pending = (await client.getTagSuggest()).pending;
    await waitFor(() => expect(screen.getByTestId("nav-suggestions")).toHaveTextContent(String(pending)));
    const { user, review, group } = await openReview();
    expect(within(review).getByTestId("review-summary")).toHaveTextContent(`${pending} suggestions`);
    // The value suggested for the most videos comes first.
    const first = within(review).getAllByTestId("review-group")[0]!;
    expect(first.dataset.value).toBe("kaydı");
    expect(first).toHaveTextContent("2 videos");

    // The group lists its videos.
    await user.click(within(first).getByTestId("review-toggle"));
    expect(within(first).getAllByTestId("review-item")).toHaveLength(2);
    // A video's title opens its details beside the review, also when the
    // library's loaded page does not hold it.
    const title = within(within(first).getAllByTestId("review-item")[0]!).getAllByRole("button")[0]!;
    const name = title.textContent!;
    await user.click(title);
    const inspector = await screen.findByTestId("inspector");
    await waitFor(() => expect(inspector).toHaveTextContent(name));
    expect(screen.getByTestId("suggestion-review")).toBeInTheDocument();

    await user.click(within(first).getByTestId("review-accept"));
    await waitFor(() => expect(group("kaydı")).toBeUndefined());
    const toast = (await screen.findAllByTestId("toast")).at(-1)!;
    expect(toast).toHaveTextContent("Added #kaydı to 2 videos.");
    for (const r of both) expect((await client.getItem(r.id)).tagRefs.map((t) => t.name)).toContain("kaydı");

    await user.click(within(toast).getByTestId("toast-undo"));
    await waitFor(async () => {
      for (const r of both) expect((await client.getItem(r.id)).tagRefs.map((t) => t.name)).not.toContain("kaydı");
    });
  });

  it("adds a group under a new name, and rejecting many asks first and tags nothing", async () => {
    const { client, recs } = await suggested();
    render(<App client={client} />);
    const { user, review, group } = await openReview();

    // Rename before adding.
    const one = within(review).getAllByTestId("review-group").find((g) => g.dataset.value === "tarif")!;
    await user.click(within(one).getByTestId("review-name"));
    const input = within(one).getByTestId("review-edit-input");
    await user.clear(input);
    await user.type(input, "Ekmek tarifi{Enter}");
    await waitFor(() => expect(group("tarif")).toBeUndefined());
    const bread = recs.find((r) => r.title?.includes("ekmek"))!;
    expect((await client.getItem(bread.id)).tagRefs.map((t) => t.name)).toContain("Ekmek tarifi");

    // Reject for two videos: a confirmation first.
    await waitFor(() => expect(group("kaydı")).toBeDefined());
    const kaydi = group("kaydı")!;
    await user.click(within(kaydi).getByTestId("review-reject"));
    const dialog = await screen.findByTestId("confirm-dialog");
    expect(dialog).toHaveTextContent("these 2 videos");
    await user.click(within(dialog).getByTestId("confirm-submit"));
    await waitFor(() => expect(group("kaydı")).toBeUndefined());
    for (const r of shared(recs)) expect((await client.getItem(r.id)).tagRefs.map((t) => t.name)).not.toContain("kaydı");
    // Not suggested again for those videos.
    for (const r of shared(recs)) {
      const again = await client.suggestTags(r.id, true);
      expect(again.proposals.map((p) => p.value)).not.toContain("kaydı");
    }
  });

  it("finds a tag by name", async () => {
    const { client } = await suggested();
    render(<App client={client} />);
    const { user, review } = await openReview();
    await user.type(within(review).getByTestId("review-filter"), "KONS");
    expect(within(review).getAllByTestId("review-group").map((g) => g.dataset.value)).toEqual(["konser"]);
  });

  it("is not in the sidebar while nothing waits, and explains itself when empty", async () => {
    const client = createFixtureClient();
    const { unmount } = render(<App client={client} />);
    await screen.findByTestId("nav-ai");
    expect(screen.queryByTestId("nav-suggestions")).toBeNull();
    unmount();
    render(<SuggestionReview client={client} reloadKey={0} onDecided={() => undefined} onOpenItem={() => undefined} />);
    expect(await screen.findByTestId("review-empty")).toHaveTextContent("Suggest for all videos");
  });
});
