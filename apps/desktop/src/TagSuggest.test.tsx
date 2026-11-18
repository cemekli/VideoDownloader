import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { AiProcessingView } from "./components/AiProcessing";
import { createFixtureClient } from "./ipc/client";
import { fixtureTags } from "./ipc/fixtureSuggest";

// STATE.md F7.3, SPEC AI-B01…B02, AI-C07 against the fixture backend: tag
// suggestions are off until the user gives a chat API; suggestions wait in
// the item's details and change nothing until one is accepted. The rules
// themselves are tested in Rust (crates/vv-engine/tests/suggest.rs,
// apps/desktop/src-tauri/tests/ipc.rs).

const cards = () => screen.findAllByTestId("library-card");
const card = async (idEnd: string) => (await cards()).find((c) => c.dataset.id?.endsWith(idEnd))!;

describe("Tag suggestions", () => {
  it("needs a chat API, asks before another computer, and suggests for all", async () => {
    const user = userEvent.setup();
    const client = createFixtureClient();
    render(<AiProcessingView client={client} notify={vi.fn()} onSettings={vi.fn()} />);
    const block = await screen.findByTestId("tag-suggest");
    expect(within(block).getByTestId("suggest-api")).toHaveAttribute("data-state", "none");
    expect(block).toHaveTextContent("Set a chat API first.");
    expect(within(block).queryByTestId("suggest-all")).toBeNull();
    // Ollama on this PC with a small model is the default.
    expect(within(block).getByTestId("suggest-api-url")).toHaveValue("http://localhost:11434/v1");
    expect(within(block).getByTestId("suggest-api-model")).toHaveValue("qwen3.5:4b");

    // Another computer: asked first; cancel saves nothing.
    const url = within(block).getByTestId("suggest-api-url");
    await user.clear(url);
    await user.type(url, "https://llm.example.com/v1");
    await user.click(within(block).getByTestId("suggest-api-save"));
    const dialog = await screen.findByTestId("confirm-dialog");
    expect(dialog).toHaveTextContent("https://llm.example.com/v1 is not this PC");
    await user.click(within(dialog).getByText("Cancel"));
    expect((await client.getTagSuggest()).api).toBeNull();

    await user.clear(url);
    await user.type(url, "http://localhost:11434/v1");
    await user.click(within(block).getByTestId("suggest-api-save"));
    expect(await within(block).findByTestId("suggest-api-summary")).toHaveTextContent("qwen3.5:4b at http://localhost:11434/v1.");
    expect(block).toHaveTextContent("item text does not leave it");

    await user.click(within(block).getByTestId("suggest-all"));
    await waitFor(() => expect(within(block).getByTestId("suggest-run")).toHaveAttribute("data-running", "no"), { timeout: 8000 });
    expect(within(block).getByTestId("suggest-run-status")).toHaveTextContent(/^Done: \d+ items asked/);
    expect(within(block).getByTestId("suggest-pending")).toHaveTextContent("suggestions are waiting");
  }, 15000);

  it("suggests for one item, accepts an edited one as a tag and rejects another", async () => {
    const user = userEvent.setup();
    const client = createFixtureClient();
    await client.setSuggestApi("http://localhost:11434/v1", "qwen3.5:4b", null, false);
    render(<App client={client} />);
    await user.dblClick(await card("0002"));
    const box = await screen.findByTestId("inspector-suggestions");
    expect(within(box).queryByTestId("suggestion")).toBeNull();
    await user.click(within(box).getByTestId("suggestion-ask"));
    const expected = fixtureTags("Kısa tarif: ev yapımı ekmek");
    await waitFor(() => expect(within(box).getAllByTestId("suggestion")).toHaveLength(expected.length));
    // Nothing was tagged yet (AI-B02).
    const tags = screen.getByTestId("inspector-tags");
    expect(tags).not.toHaveTextContent(`#${expected[0]}`);

    // Edit the first one, then add it.
    await user.click(within(box).getByRole("button", { name: `Edit suggestion ${expected[0]}` }));
    const input = within(box).getByTestId("suggestion-edit-input");
    await user.clear(input);
    await user.type(input, "Ekmek tarifi{Enter}");
    await waitFor(() => expect(within(screen.getByTestId("inspector-tags")).getByText("#Ekmek tarifi")).toBeVisible());
    expect(within(box).getAllByTestId("suggestion")).toHaveLength(expected.length - 1);

    // Reject the next one: gone, no tag.
    await user.click(within(box).getAllByTestId("suggestion-reject")[0]!);
    await waitFor(() => expect(within(box).queryAllByTestId("suggestion")).toHaveLength(expected.length - 2));
    expect(screen.getByTestId("inspector-tags")).not.toHaveTextContent(`#${expected[1]}`);

    // Asking again does not bring back what was decided.
    await user.click(within(box).getByTestId("suggestion-ask"));
    await waitFor(() => expect(within(box).queryAllByText(`#${expected[1]}`)).toHaveLength(0));
  });

  it("explains when no chat API is set", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    await user.dblClick(await card("0002"));
    const box = await screen.findByTestId("inspector-suggestions");
    await user.click(within(box).getByTestId("suggestion-ask"));
    expect(await within(box).findByTestId("suggestion-note")).toHaveTextContent("No chat API is set");
  });
});
