import { describe, expect, it } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { createFixtureClient, IpcError } from "./ipc/client";
import { fixtureRecords, manyRecords } from "./ipc/fixture";
import type { BackendClient } from "./ipc/types";

// STATE.md F1.7: the Library screen against the fixture backend.

const cards = () => screen.findAllByTestId("library-card");
const titles = async () => (await cards()).map((c) => within(c).getByRole("heading").textContent);

describe("Library screen", () => {
  it("shows every item with its local state, never derived from kind (INV-02)", async () => {
    render(<App client={createFixtureClient()} />);
    const all = await cards();
    expect(all).toHaveLength(7);
    const state = (id: string) => all.find((c) => c.dataset.id?.endsWith(id))!;
    expect(within(state("0001")).getByTestId("local-state")).toHaveTextContent("Downloaded");
    expect(within(state("0002")).getByTestId("local-state")).toHaveTextContent("Bookmark only");
    expect(within(state("0004")).getByTestId("local-state")).toHaveTextContent("File missing");
    expect(within(state("0007")).getByTestId("local-state")).toHaveTextContent("Drive disconnected");
    expect(screen.getByTestId("result-count")).toHaveTextContent("7 items");
  });

  it("shows unknown duration as a dash and a placeholder for a missing thumbnail", async () => {
    render(<App client={createFixtureClient()} />);
    const card = (await cards()).find((c) => c.textContent?.includes("Thread with embedded video"))!;
    expect(within(card).getByTestId("duration")).toHaveTextContent("—");
    expect(card.querySelector(".thumb")).toHaveAttribute("data-thumb", "missing");
  });

  it("first use: explains the empty library and offers import", async () => {
    render(<App client={createFixtureClient([])} />);
    const empty = await screen.findByTestId("library-empty");
    expect(within(empty).getByRole("button", { name: "Import files…" })).toBeEnabled();
    expect(within(empty).getByRole("button", { name: "Import a folder…" })).toBeEnabled();
  });

  it("loading shows a status", () => {
    const c = { ...createFixtureClient(), listItems: () => new Promise<never>(() => {}) } as BackendClient;
    render(<App client={c} />);
    expect(screen.getByTestId("library-loading")).toBeInTheDocument();
    expect(within(screen.getByTestId("library-loading")).getByRole("status")).toHaveTextContent("Loading");
  });

  it("an error can be retried", async () => {
    const base = createFixtureClient();
    let calls = 0;
    const c = { ...base, listItems: (q: Parameters<BackendClient["listItems"]>[0]) => (++calls === 1 ? Promise.reject(new IpcError("INTERNAL_ERROR", "x")) : base.listItems(q)) } as BackendClient;
    render(<App client={c} />);
    const err = await screen.findByTestId("library-error");
    expect(err).toHaveTextContent("INTERNAL_ERROR");
    await userEvent.click(within(err).getByRole("button", { name: "Try again" }));
    expect(await cards()).toHaveLength(7);
  });

  it("search narrows the list and highlights matches as text (SEA-03)", async () => {
    const records = fixtureRecords();
    records[0]!.notes = '<img src=x onerror="alert(1)"> ekmek notu';
    render(<App client={createFixtureClient(records)} />);
    await cards();
    await userEvent.type(screen.getByTestId("search-input"), "ekmek");
    await waitFor(async () => expect(await titles()).toHaveLength(2));
    const snippet = screen.getByTestId("snippet");
    expect(snippet).toHaveAttribute("data-field", "notes");
    expect(snippet.textContent).toContain('<img src=x onerror="alert(1)">');
    expect(snippet.querySelector("img")).toBeNull();
    expect(snippet.querySelector("mark")).toHaveTextContent("ekmek");
  });

  it("Turkish search folds case and dotless i", async () => {
    render(<App client={createFixtureClient()} />);
    await cards();
    await userEvent.type(screen.getByTestId("search-input"), "KISA");
    await waitFor(async () => expect(await titles()).toEqual(["Kısa tarif: ev yapımı ekmek"]));
  });

  it("no results offers to clear the search", async () => {
    render(<App client={createFixtureClient()} />);
    await cards();
    await userEvent.type(screen.getByTestId("search-input"), "zzzz");
    const none = await screen.findByTestId("library-no-match");
    expect(none).toHaveTextContent("No results for “zzzz”");
    await userEvent.click(within(none).getByRole("button", { name: "Clear search" }));
    expect(await cards()).toHaveLength(7);
  });

  it("kind chips and the platform list filter; clearing restores everything", async () => {
    render(<App client={createFixtureClient()} />);
    await cards();
    await userEvent.click(screen.getByTestId("chip-kind-local_video"));
    await waitFor(async () => expect(await cards()).toHaveLength(2));
    expect(screen.getByTestId("chip-kind-local_video")).toHaveAttribute("aria-pressed", "true");
    await userEvent.click(screen.getByTestId("clear-filters"));
    await waitFor(async () => expect(await cards()).toHaveLength(7));

    await userEvent.click(screen.getByTestId("nav-platform-youtube"));
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("YouTube");
    await waitFor(async () => expect(await cards()).toHaveLength(1));
    expect(screen.getByTestId("chip-platform-youtube")).toBeInTheDocument();
  });

  it("an empty filter is not the first-use state", async () => {
    render(<App client={createFixtureClient()} />);
    await cards();
    await userEvent.click(screen.getByTestId("nav-platform-facebook"));
    expect(await screen.findByTestId("library-no-match")).toHaveTextContent("No items match these filters");
    expect(screen.queryByTestId("library-empty")).toBeNull();
  });

  it("Favorites and Recently Added views", async () => {
    render(<App client={createFixtureClient()} />);
    await cards();
    await userEvent.click(screen.getByTestId("nav-favorites"));
    await waitFor(async () => expect(await cards()).toHaveLength(2));
    await userEvent.click(screen.getByTestId("nav-recent"));
    // Added within 30 days of the fixture "now".
    await waitFor(async () => expect(await cards()).toHaveLength(5));
    await userEvent.click(screen.getByTestId("nav-archive"));
    expect(await screen.findByTestId("view-empty")).toHaveTextContent("Nothing in Archive yet");
  });

  it("sorts by title", async () => {
    render(<App client={createFixtureClient()} />);
    await cards();
    await userEvent.selectOptions(screen.getByTestId("sort-select"), "title");
    await waitFor(async () => expect((await titles())[0]).toBe("An extremely long title that keeps going to make sure that the card layout never breaks or overflows horizontally even on narrow windows"));
  });

  it("keyboard: arrows move and select, Shift extends, Enter opens details, Esc closes and returns focus (§9.4)", async () => {
    render(<App client={createFixtureClient()} />);
    const all = await cards();
    expect(all[0]).toHaveAttribute("tabindex", "0");
    expect(all[1]).toHaveAttribute("tabindex", "-1");
    act(() => all[0]!.focus());
    await userEvent.keyboard("{ArrowRight}");
    await waitFor(() => expect(document.activeElement).toBe(screen.getAllByTestId("library-card")[1]));
    expect(screen.getAllByTestId("library-card")[1]).toHaveAttribute("aria-selected", "true");
    await userEvent.keyboard("{Shift>}{ArrowRight}{/Shift}");
    expect(screen.getByTestId("result-count")).toHaveTextContent("2 selected");
    await userEvent.keyboard("{ArrowLeft}");
    await userEvent.keyboard("{Enter}");
    const inspector = await screen.findByTestId("inspector");
    expect(await within(inspector).findByTestId("inspector-title")).toHaveTextContent("Kısa tarif: ev yapımı ekmek");
    act(() => inspector.focus());
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByTestId("inspector")).toBeNull();
    expect(document.activeElement).toBe(screen.getAllByTestId("library-card")[1]);
  });

  it("Ctrl+click selects several; Ctrl+A selects all loaded", async () => {
    render(<App client={createFixtureClient()} />);
    const all = await cards();
    const user = userEvent.setup();
    await user.click(all[0]!);
    await user.keyboard("{Control>}");
    await user.click(all[2]!);
    await user.keyboard("{/Control}");
    expect(screen.getByTestId("result-count")).toHaveTextContent("2 selected");
    await userEvent.keyboard("{Control>}a{/Control}");
    expect(screen.getByTestId("result-count")).toHaveTextContent("7 selected");
    await userEvent.click(screen.getByTestId("clear-selection"));
    expect(screen.getByTestId("result-count")).toHaveTextContent("7 items");
  });

  it("inspector shows media of a local item without its path", async () => {
    render(<App client={createFixtureClient()} />);
    const card = (await cards()).find((c) => c.dataset.id?.endsWith("0007"))!;
    fireEvent.doubleClick(card);
    const media = await screen.findByTestId("inspector-media");
    expect(media).toHaveTextContent("konser.mp4");
    expect(media).toHaveTextContent("Not connected");
    expect(media).toHaveTextContent("Not checked while the drive is away");
  });

  it("Ctrl+K focuses search", async () => {
    render(<App client={createFixtureClient()} />);
    await cards();
    await userEvent.keyboard("{Control>}k{/Control}");
    expect(document.activeElement).toBe(screen.getByTestId("search-input"));
  });

  it("import reports what happened", async () => {
    render(<App client={createFixtureClient()} />);
    await cards();
    await userEvent.click(screen.getByTestId("import-button"));
    await userEvent.click(screen.getByTestId("import-files"));
    expect(screen.getByTestId("import-button")).toHaveTextContent("Importing…");
    const toast = await screen.findByTestId("toast");
    expect(toast).toHaveTextContent("Imported 2 files.");
    expect(toast).toHaveTextContent("1 file was not imported: it is a playlist, manifest or other unsupported input.");
    expect(screen.getByTestId("import-button")).toHaveTextContent("Import");
  });

  it("an import without media tools says so", async () => {
    const base = createFixtureClient();
    const c = { ...base, importMedia: () => Promise.reject(new IpcError("TOOL_MISSING", "x")) } as BackendClient;
    render(<App client={c} />);
    await cards();
    await userEvent.click(screen.getByTestId("import-button"));
    await userEvent.click(screen.getByTestId("import-folder"));
    expect(await screen.findByTestId("toast")).toHaveTextContent("media tools (ffprobe)");
  });

  it("a drop imported by the backend reloads the list", async () => {
    const base = createFixtureClient();
    let lists = 0;
    const c = { ...base, listItems: (q: Parameters<BackendClient["listItems"]>[0]) => (lists++, base.listItems(q)) } as BackendClient;
    render(<App client={c} />);
    await cards();
    const before = lists;
    act(() => base.emitImportStatus({ phase: "started" }));
    expect(screen.getByTestId("import-button")).toHaveTextContent("Importing…");
    act(() => base.emitImportStatus({ phase: "finished", summary: { picked: true, imported: 1, alreadyImported: 0, duplicates: 0, rejected: 0, skipped: 0, ignored: 0, canceled: false, firstError: null } }));
    expect(await screen.findByTestId("toast")).toHaveTextContent("Imported 1 file.");
    await waitFor(() => expect(lists).toBeGreaterThan(before));
  });

  it("a large library renders only what is on screen and pages in more", async () => {
    render(<App client={createFixtureClient(manyRecords(2000))} />);
    const shown = await cards();
    expect(screen.getByTestId("result-count")).toHaveTextContent("2000 items");
    expect(screen.getByTestId("library-results")).toHaveAttribute("data-total", "2000");
    expect(shown.length).toBeGreaterThan(8);
    expect(shown.length).toBeLessThan(120);
    expect(shown[0]).toHaveAttribute("aria-setsize", "2000");
  });

  it("offline shows a banner and the library keeps working", async () => {
    render(<App client={createFixtureClient()} />);
    await cards();
    act(() => {
      Object.defineProperty(navigator, "onLine", { configurable: true, value: false });
      window.dispatchEvent(new Event("offline"));
    });
    expect(screen.getByTestId("offline-banner")).toHaveTextContent("You are offline");
    expect(await cards()).toHaveLength(7);
    act(() => {
      Object.defineProperty(navigator, "onLine", { configurable: true, value: true });
      window.dispatchEvent(new Event("online"));
    });
    expect(screen.queryByTestId("offline-banner")).toBeNull();
  });
});
