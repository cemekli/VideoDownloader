import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import { Library } from "./Library";
import { createFixtureClient } from "../ipc/client";
import type { LibraryClient } from "../ipc/types";

describe("Library", () => {
  it("renders every fixture item and distinguishes bookmark-only from downloaded (INV-02)", async () => {
    render(<Library client={createFixtureClient()} />);
    const cards = await screen.findAllByTestId("library-card");
    expect(cards).toHaveLength(6);

    const bookmark = cards.find((c) => c.dataset.localState === "bookmark_only")!;
    const ready = cards.find((c) => c.dataset.localState === "local_ready")!;
    expect(within(bookmark).getByTestId("local-state")).toHaveTextContent("Bookmark only");
    expect(within(ready).getByTestId("local-state")).toHaveTextContent("Downloaded");
  });

  it("shows an empty state", async () => {
    render(<Library client={createFixtureClient([])} />);
    expect(await screen.findByTestId("library-empty")).toBeInTheDocument();
  });

  it("shows an error when the backend call fails", async () => {
    const failing: LibraryClient = { listItems: () => Promise.reject(new Error("backend down")) };
    render(<Library client={failing} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("backend down");
  });

  it("shows a loading status before data arrives", () => {
    const never: LibraryClient = { listItems: () => new Promise(() => {}) };
    render(<Library client={never} />);
    expect(screen.getByRole("status")).toHaveTextContent("Loading");
  });
});
