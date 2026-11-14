import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AiProcessingView } from "./components/AiProcessing";
import { createFixtureClient } from "./ipc/client";

// STATE.md F7.1, SPEC §16.3, AC-20, AC-24 against the fixture backend:
// search by meaning is off until the embedding model is downloaded through
// the consent dialog; the index is built, a rebuild can be canceled and the
// current index stays. The rules themselves are tested in Rust
// (crates/vv-persistence/tests/vectors.rs, crates/vv-engine/tests/embed.rs).

function setup() {
  const client = createFixtureClient();
  render(<AiProcessingView client={client} notify={vi.fn()} onSettings={vi.fn()} />);
  return client;
}

describe("Search by meaning", () => {
  it("needs the model, downloaded only after consent, then builds the index", async () => {
    const user = userEvent.setup();
    const client = setup();
    const block = await screen.findByTestId("search-index");
    expect(block).toHaveTextContent("Ordinary search works without it");
    expect(within(block).getByTestId("search-model")).toHaveAttribute("data-status", "not_installed");
    expect(within(block).queryByTestId("search-index-build")).toBeNull();
    expect(block).toHaveTextContent("Download the model first.");

    // AC-20: cancel in the dialog downloads nothing.
    await user.click(within(block).getByTestId("search-model-download"));
    const dialog = await screen.findByTestId("model-consent");
    expect(within(dialog).getByTestId("consent-source")).toHaveTextContent("huggingface.co/ggml-org/bge-m3-Q8_0-GGUF");
    expect(within(dialog).getByTestId("consent-license")).toHaveTextContent("MIT");
    expect(dialog).toHaveTextContent("Memory while indexing");
    await user.click(within(dialog).getByTestId("consent-cancel"));
    expect((await client.getSearchIndex()).models[0]!.status).toBe("not_installed");

    await user.click(within(block).getByTestId("search-model-download"));
    await user.click(within(await screen.findByTestId("model-consent")).getByTestId("consent-agree"));
    await waitFor(() => expect(within(block).getByTestId("search-model")).toHaveAttribute("data-status", "ready"), { timeout: 5000 });

    await user.click(await within(block).findByTestId("search-index-build"));
    await waitFor(() => expect(within(block).getByTestId("search-index-state")).toHaveAttribute("data-active", "yes"), { timeout: 5000 });
    expect(within(block).getByTestId("search-index-active")).toHaveTextContent("Index of 12 of 12 transcript passages, made with BGE-M3.");
    expect(within(block).getByTestId("search-index-rebuild")).toBeInTheDocument();
  });

  it("cancels a rebuild and keeps the current index (AC-24)", async () => {
    const user = userEvent.setup();
    const client = createFixtureClient();
    await client.downloadModel("embed-bge-m3", true);
    await waitFor(async () => expect((await client.getSearchIndex()).models[0]!.status).toBe("ready"), { timeout: 5000 });
    await client.buildSearchIndex(false);
    await waitFor(async () => expect((await client.getSearchIndex()).active).not.toBeNull(), { timeout: 5000 });
    render(<AiProcessingView client={client} notify={vi.fn()} onSettings={vi.fn()} />);
    const block = await screen.findByTestId("search-index");
    await user.click(await within(block).findByTestId("search-index-rebuild"));
    await waitFor(() => expect(within(block).getByTestId("search-index-state")).toHaveAttribute("data-building", "yes"));
    expect(within(block).getByTestId("search-index-progress")).toHaveTextContent("Searches use the current index until it is complete.");
    await user.click(within(block).getByTestId("search-index-cancel"));
    await waitFor(() => expect(within(block).getByTestId("search-index-state")).toHaveAttribute("data-building", "no"));
    expect(within(block).getByTestId("search-index-state")).toHaveAttribute("data-active", "yes");
    expect(block).toHaveTextContent("The last build was canceled; the current index was kept.");
  });
});
