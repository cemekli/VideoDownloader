import { describe, expect, it } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { createFixtureClient } from "./ipc/client";
import { fixtureRecords } from "./ipc/fixture";
import { opMessage, undoMessage } from "./labels";
import { tagKeyOf } from "./tags";

// STATE.md F2.1–F2.3 against the fixture backend. The per-field Undo rules
// and the database side are tested in Rust (tests/organize.rs, tests/ipc.rs).

const cards = () => screen.findAllByTestId("library-card");
const card = async (idEnd: string) => (await cards()).find((c) => c.dataset.id?.endsWith(idEnd))!;
const lastToast = async () => {
  await screen.findAllByTestId("toast");
  const all = screen.getAllByTestId("toast");
  return all[all.length - 1]!;
};
const count = () => screen.getByTestId("result-count");
const navTexts = (row: string, nav: string) => screen.queryAllByTestId(row).map((r) => within(r).getByTestId(nav).textContent);

async function selectAllCards(user: ReturnType<typeof userEvent.setup>) {
  await user.click((await cards())[0]!);
  await user.keyboard("{Control>}a{/Control}");
}

describe("Organize", () => {
  it("bulk-tags every selected item and Undo takes it back (AC-07)", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    await selectAllCards(user);
    expect(screen.getByTestId("bulk-bar")).toBeVisible();
    await user.click(screen.getByTestId("bulk-tag"));
    await user.type(screen.getByTestId("name-input"), "deniz");
    await user.click(screen.getByTestId("name-submit"));
    const t = await lastToast();
    expect(t).toHaveTextContent("Tagged 7 items with #deniz.");
    await waitFor(() => expect(navTexts("tag-row", "nav-tag")).toContain("#deniz7"));
    // Selection stays for the next action (AC-07).
    expect(count()).toHaveTextContent("7 selected");

    await user.click(within(t).getByTestId("toast-undo"));
    expect(await lastToast()).toHaveTextContent("Tagging undone.");
    await waitFor(() => expect(navTexts("tag-row", "nav-tag")).not.toContain("#deniz7"));
  });

  it("archives with the bar and Ctrl+Z undoes it", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    await user.click(await card("0002"));
    await user.click(screen.getByTestId("bulk-archive"));
    expect(await lastToast()).toHaveTextContent("Archived 1 item.");
    await waitFor(() => expect(count()).toHaveTextContent("6 items"));
    await user.keyboard("{Control>}z{/Control}");
    expect(await lastToast()).toHaveTextContent("Archive change undone.");
    await waitFor(() => expect(count()).toHaveTextContent("7 items"));
  });

  it("collections: create, add items, Inbox shrinks, delete keeps the items (AC-06, AC-48)", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    await cards();
    await user.click(screen.getByTestId("new-collection"));
    await user.type(screen.getByTestId("name-input"), "Gezi");
    await user.click(screen.getByTestId("name-submit"));
    const row = await screen.findByTestId("collection-row");
    expect(row).toHaveTextContent("Gezi");

    // A sub-collection from the row menu.
    await user.click(within(row).getByTestId("collection-menu"));
    await user.click(screen.getByTestId("menu-new-sub"));
    await user.type(screen.getByTestId("name-input"), "Deniz");
    await user.click(screen.getByTestId("name-submit"));
    await waitFor(() => expect(screen.getAllByTestId("collection-row")).toHaveLength(2));

    await user.click(await card("0001"));
    await user.click(screen.getByTestId("bulk-collect"));
    await user.selectOptions(screen.getByTestId("pick-select"), screen.getByRole("option", { name: "Gezi" }));
    await user.click(screen.getByTestId("pick-submit"));
    expect(await lastToast()).toHaveTextContent("Added 1 item to Gezi.");

    await user.click(screen.getByTestId("nav-inbox"));
    await waitFor(() => expect(count()).toHaveTextContent("6 items"));
    await user.click(screen.getAllByTestId("nav-collection")[0]!);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Gezi");
    await waitFor(() => expect(count()).toHaveTextContent("1 item"));

    // Delete with "move sub-collections up": the child stays, the item stays.
    await user.click(within(screen.getAllByTestId("collection-row")[0]!).getByTestId("collection-menu"));
    await user.click(screen.getByTestId("menu-delete"));
    expect(screen.getByTestId("choice-move_up")).toBeChecked();
    await user.click(screen.getByTestId("confirm-submit"));
    expect(await lastToast()).toHaveTextContent("Deleted Gezi. Its items are still in your library.");
    await waitFor(() => expect(navTexts("collection-row", "nav-collection")).toEqual(["Deniz"]));
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("All Items");
    await waitFor(() => expect(count()).toHaveTextContent("7 items"));
  });

  it("refuses a tag rename onto an existing name and offers merge; merge is undoable (AC-39, AC-46)", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    await cards();
    const rowOf = async (name: string) => (await screen.findAllByTestId("tag-row")).find((r) => r.textContent?.startsWith(`#${name}`))!;
    await user.click(within(await rowOf("talk")).getByTestId("tag-menu"));
    await user.click(screen.getByTestId("menu-rename"));
    const input = screen.getByTestId("name-input");
    await user.clear(input);
    await user.type(input, "DATABASE");
    expect(screen.getByTestId("name-error")).toHaveTextContent("Use Merge into…");
    expect(screen.getByTestId("name-submit")).toBeDisabled();
    await user.clear(input);
    await user.type(input, "konuşma");
    await user.click(screen.getByTestId("name-submit"));
    expect(await lastToast()).toHaveTextContent("Renamed #talk to #konuşma.");

    await user.click(within(await rowOf("konuşma")).getByTestId("tag-menu"));
    await user.click(screen.getByTestId("menu-merge"));
    await user.selectOptions(screen.getByTestId("pick-select"), screen.getByRole("option", { name: "#database" }));
    await user.click(screen.getByTestId("pick-submit"));
    expect(await lastToast()).toHaveTextContent("Merged #konuşma.");
    await waitFor(() => expect(screen.getAllByTestId("tag-row").some((r) => r.textContent?.startsWith("#konuşma"))).toBe(false));
    await user.keyboard("{Control>}z{/Control}");
    expect(await lastToast()).toHaveTextContent("Tag merge undone.");
    expect(await rowOf("konuşma")).toBeTruthy();
  });

  it("remove, restore, then delete for good keeps downloaded media as orphaned; re-import brings it back (AC-53)", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    await user.click(await card("0001"));
    await user.click(screen.getByTestId("bulk-remove"));
    expect(await lastToast()).toHaveTextContent("Moved 1 item to Recently removed.");
    await waitFor(() => expect(count()).toHaveTextContent("6 items"));

    await user.click(screen.getByTestId("nav-removed"));
    await waitFor(() => expect(count()).toHaveTextContent("2 items"));
    await user.click(await card("0001"));
    await user.click(screen.getByTestId("bulk-restore"));
    expect(await lastToast()).toHaveTextContent("Restored 1 item.");
    await waitFor(() => expect(count()).toHaveTextContent("1 item"));

    await user.click(screen.getByTestId("nav-all"));
    await user.click(await card("0001"));
    await user.keyboard("{Delete}");
    expect(await lastToast()).toHaveTextContent("Moved 1 item to Recently removed.");
    await user.click(screen.getByTestId("nav-removed"));
    await user.click(await card("0001"));
    await user.click(screen.getByTestId("bulk-delete"));
    const dialog = screen.getByTestId("confirm-dialog");
    expect(within(dialog).getByTestId("confirm-checkbox")).not.toBeChecked();
    await user.click(within(dialog).getByTestId("confirm-submit"));
    expect(await lastToast()).toHaveTextContent("Deleted 1 item. 1 downloaded file was kept in Orphaned media.");

    await user.click(await screen.findByTestId("open-orphans"));
    const orphan = await screen.findByTestId("orphan");
    expect(orphan).toHaveTextContent("postgres-replication.mp4");
    await user.click(within(orphan).getByTestId("orphan-reimport"));
    expect(await lastToast()).toHaveTextContent("Re-imported “PostgreSQL replication deep dive”.");
    expect(await screen.findByTestId("orphans-empty")).toBeVisible();
  });

  it("merges a duplicate after a preview, and Undo brings both back (ORG-11)", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    await cards();
    await user.click(screen.getByTestId("nav-duplicates"));
    const dup = await screen.findByTestId("duplicate");
    await user.click(within(dup).getAllByTestId("dup-keep")[0]!);
    const preview = await screen.findByTestId("merge-preview");
    expect(preview).toHaveTextContent("PostgreSQL replication deep dive is kept.");
    expect(preview).toHaveTextContent("Thread with embedded video moves to Recently removed");
    await user.click(screen.getByTestId("merge-confirm"));
    expect(await lastToast()).toHaveTextContent("Merged into “PostgreSQL replication deep dive”.");
    expect(await screen.findByTestId("duplicates-empty")).toBeVisible();
    await user.click(within(await lastToast()).getByTestId("toast-undo"));
    expect(await lastToast()).toHaveTextContent("Merge undone.");
    expect(await screen.findByTestId("duplicate")).toBeVisible();
  });

  it("a smart collection applies its saved rules (ORG-07)", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    await cards();
    await user.click(screen.getByTestId("new-smart"));
    await user.type(screen.getByTestId("smart-name"), "Yerel");
    expect(screen.getByTestId("smart-save")).toBeDisabled();
    await user.click(screen.getByRole("checkbox", { name: "Local files" }));
    await user.click(screen.getByTestId("smart-save"));
    await waitFor(() => expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Yerel"));
    await waitFor(() => expect(count()).toHaveTextContent("2 items"));
  });

  it("edits notes and tags from the details panel; a stale revision is refused (ORG-03)", async () => {
    const user = userEvent.setup();
    const records = fixtureRecords();
    const client = createFixtureClient(records);
    render(<App client={client} />);
    await user.dblClick(await card("0002"));
    const notes = await screen.findByTestId("inspector-notes-input");
    await user.clear(notes);
    await user.type(notes, "Hafta sonu dene");
    await user.click(screen.getByTestId("inspector-notes-save"));
    expect(await screen.findByText("Saved.")).toBeVisible();

    // Someone else changed it: the next save is refused.
    records.find((r) => r.id.endsWith("0002"))!.revision += 1;
    await user.type(notes, "!");
    await user.click(screen.getByTestId("inspector-notes-save"));
    expect(await screen.findByText(/changed somewhere else/)).toBeVisible();

    await user.click(screen.getByTestId("inspector-add-tag"));
    await user.type(screen.getByTestId("name-input"), "ekmek");
    await user.click(screen.getByTestId("name-submit"));
    await waitFor(() => expect(within(screen.getByTestId("inspector-tags")).getByText("#ekmek")).toBeVisible());
    await user.click(within(screen.getByTestId("inspector-tags")).getAllByTestId("inspector-untag")[0]!);
    expect(await lastToast()).toHaveTextContent("Removed #yemek from 1 item.");

    // Search finds the saved note right away (SEA-05).
    await user.type(screen.getByTestId("search-input"), "hafta");
    await waitFor(() => expect(count()).toHaveTextContent("1 item"));
  });
});

describe("Organize messages", () => {
  it("counts changed, unchanged and failed honestly (ORG-05)", () => {
    expect(
      opMessage("Archived {n}.", {
        changed: 18,
        unchanged: 1,
        failed: 1,
        undo: null,
      }),
    ).toBe("Archived 18 items. 1 item was already like that. 1 item was not found or already removed.");
    expect(
      opMessage("Archived {n}.", {
        changed: 0,
        unchanged: 2,
        failed: 0,
        undo: null,
      }),
    ).toBe("Nothing changed. 2 items were already like that.");
    expect(undoMessage({ kind: "set_archived", applied: 1, skipped: 2 })).toBe("Archive change undone. 2 items were left as they are because they were changed since.");
  });

  it("tag names fold like the backend (İ/I/ı → i)", () => {
    expect(tagKeyOf("  İstanbul  ")).toBe(tagKeyOf("ISTANBUL"));
    expect(tagKeyOf("ıstanbul")).toBe("istanbul");
    expect(tagKeyOf("Deniz   Kenarı")).toBe("deniz kenari");
  });
});
