import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AiProcessingView } from "./components/AiProcessing";
import { createFixtureClient } from "./ipc/client";

// STATE.md F7.1, SPEC §16.3, AC-24, AI-C07 against the fixture backend:
// search by meaning is off until the user gives an embedding API; it is
// checked with one request, an address other than this PC is asked about
// first, the index is built, a rebuild can be canceled and the current
// index stays. The rules themselves are tested in Rust
// (crates/vv-persistence/tests/vectors.rs, crates/vv-engine/tests/embed.rs).

function setup(client = createFixtureClient()) {
  render(<AiProcessingView client={client} notify={vi.fn()} onSettings={vi.fn()} />);
  return client;
}

describe("Search by meaning", () => {
  it("needs an API, checks it, then builds the index", async () => {
    const user = userEvent.setup();
    const client = setup();
    const block = await screen.findByTestId("search-index");
    expect(block).toHaveTextContent("Ordinary search works without it");
    expect(within(block).getByTestId("search-api")).toHaveAttribute("data-state", "none");
    expect(within(block).queryByTestId("search-index-build")).toBeNull();
    expect(block).toHaveTextContent("Set an embedding API first.");
    // Ollama on this PC is the default.
    expect(within(block).getByTestId("search-api-url")).toHaveValue("http://localhost:11434/v1");
    expect(within(block).getByTestId("search-api-model")).toHaveValue("bge-m3");

    // Nothing answers: the form stays with a reason.
    const url = within(block).getByTestId("search-api-url");
    await user.clear(url);
    await user.type(url, "http://localhost:9/unreachable");
    await user.click(within(block).getByTestId("search-api-save"));
    expect(await within(block).findByTestId("search-api-error")).toHaveTextContent("Nothing answered at that address");
    expect((await client.getSearchIndex()).api).toBeNull();

    await user.clear(url);
    await user.type(url, "http://localhost:11434/v1");
    await user.click(within(block).getByTestId("search-api-save"));
    expect(await within(block).findByTestId("search-api-summary")).toHaveTextContent("bge-m3 at http://localhost:11434/v1 (1,024 numbers per passage).");
    expect(within(block).getByTestId("search-api-where")).toHaveTextContent("does not leave it");

    await user.click(await within(block).findByTestId("search-index-build"));
    await waitFor(() => expect(within(block).getByTestId("search-index-state")).toHaveAttribute("data-active", "yes"), { timeout: 5000 });
    expect(within(block).getByTestId("search-index-active")).toHaveTextContent("Index of 12 of 12 transcript passages, made with bge-m3.");
    expect(within(block).getByTestId("search-index-rebuild")).toBeInTheDocument();
  });

  it("asks before using an API on another computer (AI-C07)", async () => {
    const user = userEvent.setup();
    const client = setup();
    const block = await screen.findByTestId("search-index");
    const url = within(block).getByTestId("search-api-url");
    await user.clear(url);
    await user.type(url, "https://embed.example.com/v1");
    await user.type(within(block).getByTestId("search-api-key"), "sk-test");
    await user.click(within(block).getByTestId("search-api-save"));

    // Cancel sends nothing and saves nothing.
    let dialog = await screen.findByTestId("confirm-dialog");
    expect(within(dialog).getByTestId("search-remote-host")).toHaveTextContent("https://embed.example.com/v1 is not this PC");
    await user.click(within(dialog).getByText("Cancel"));
    expect((await client.getSearchIndex()).api).toBeNull();

    await user.click(within(block).getByTestId("search-api-save"));
    dialog = await screen.findByTestId("confirm-dialog");
    await user.click(within(dialog).getByTestId("confirm-submit"));
    expect(await within(block).findByTestId("search-api-where")).toHaveTextContent("Another computer");
    expect(within(block).getByTestId("search-api-where")).toHaveTextContent("credential store");
    const api = (await client.getSearchIndex()).api!;
    expect(api.local).toBe(false);
    expect(api.hasKey).toBe(true);
    // The key never comes back to the screen.
    expect(JSON.stringify(api)).not.toContain("sk-test");

    // Forget: search by meaning is off again.
    await user.click(within(block).getByTestId("search-api-clear"));
    await waitFor(() => expect(within(block).getByTestId("search-api")).toHaveAttribute("data-state", "none"));
  });

  it("cancels a rebuild and keeps the current index (AC-24)", async () => {
    const user = userEvent.setup();
    const client = createFixtureClient();
    await client.setEmbeddingApi("http://localhost:11434/v1", "bge-m3", null, false);
    await client.buildSearchIndex(false);
    await waitFor(async () => expect((await client.getSearchIndex()).active).not.toBeNull(), { timeout: 5000 });
    setup(client);
    const block = await screen.findByTestId("search-index");
    await user.click(await within(block).findByTestId("search-index-rebuild"));
    await waitFor(() => expect(within(block).getByTestId("search-index-state")).toHaveAttribute("data-building", "yes"));
    expect(within(block).getByTestId("search-index-progress")).toHaveTextContent("Searches use the current index until it is complete.");
    await user.click(within(block).getByTestId("search-index-cancel"));
    await waitFor(() => expect(within(block).getByTestId("search-index-state")).toHaveAttribute("data-building", "no"));
    expect(within(block).getByTestId("search-index-state")).toHaveAttribute("data-active", "yes");
    expect(block).toHaveTextContent("The last build was canceled; the current index was kept.");
  });

  it("says when the index was made with another API", async () => {
    const user = userEvent.setup();
    const client = createFixtureClient();
    await client.setEmbeddingApi("http://localhost:11434/v1", "bge-m3", null, false);
    await client.buildSearchIndex(false);
    await waitFor(async () => expect((await client.getSearchIndex()).active).not.toBeNull(), { timeout: 5000 });
    await client.setEmbeddingApi("http://localhost:11434/v1", "nomic-embed-text", null, false);
    setup(client);
    const block = await screen.findByTestId("search-index");
    expect(await within(block).findByTestId("search-index-other")).toHaveTextContent("made with bge-m3");
    expect(within(block).getByTestId("search-index-build")).toHaveTextContent("Build index");
    await user.click(within(block).getByTestId("search-index-build"));
    await waitFor(() => expect(within(block).getByTestId("search-index-state")).toHaveAttribute("data-building", "yes"));
  });
});
