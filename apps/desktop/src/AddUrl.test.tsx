import { describe, expect, it } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { createFixtureClient } from "./ipc/client";
import { fixtureRecords } from "./ipc/fixture";
import type { BackendClient } from "./ipc/types";
import { looksLikeUrl, withScheme } from "./url";

// STATE.md F1.8: Add URL and the Ctrl+K / Ctrl+V capture paths against the
// fixture backend. The real validation is tested in Rust (capture_commands).

const cards = () => screen.findAllByTestId("library-card");
const dialog = () => screen.findByTestId("add-url-dialog");

async function openDialog(user: ReturnType<typeof userEvent.setup>) {
  await cards();
  await user.click(screen.getByTestId("add-url-button"));
  return dialog();
}

describe("Add URL", () => {
  it("paste and Enter saves a bookmark and shows it first, selected (Journey 1)", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    const d = await openDialog(user);
    const input = within(d).getByTestId("add-url-input");
    expect(input).toHaveFocus();
    await user.click(input);
    await user.paste("https://youtu.be/newVideo01");
    expect(await within(d).findByTestId("add-url-title")).toHaveTextContent("YouTube · newVideo01");
    expect(within(d).getByTestId("add-url-platform")).toHaveTextContent("YouTube");
    await user.keyboard("{Enter}");

    await waitFor(() => expect(screen.queryByTestId("add-url-dialog")).toBeNull());
    expect(await screen.findByTestId("toast")).toHaveTextContent("Saved “YouTube · newVideo01” to your library.");
    await waitFor(() => expect(screen.getByTestId("result-count")).toHaveTextContent("8 items"));
    const first = (await cards())[0]!;
    expect(first).toHaveTextContent("YouTube · newVideo01");
    expect(within(first).getByTestId("local-state")).toHaveTextContent("Bookmark only");
    await waitFor(() => expect(first).toHaveAttribute("aria-selected", "true"));
  });

  it("Save is usable before the check answers (CAP-03)", async () => {
    const user = userEvent.setup();
    const base = createFixtureClient();
    const c = { ...base, checkUrl: () => new Promise<never>(() => {}) } as BackendClient;
    render(<App client={c} />);
    const d = await openDialog(user);
    await user.paste("https://vk.com/video-5_6");
    expect(within(d).getByTestId("add-url-save")).toBeEnabled();
    await user.keyboard("{Enter}");
    expect(await screen.findByTestId("toast")).toHaveTextContent("Saved “VK · video-5_6”");
  });

  it("an invalid link is a fixable field error and nothing is saved", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    const d = await openDialog(user);
    await user.paste("https://user:secret@example.com/v");
    expect(await within(d).findByTestId("add-url-error")).toHaveTextContent("password or a temporary access key");
    expect(within(d).getByTestId("add-url-input")).toHaveAttribute("aria-invalid", "true");
    expect(within(d).getByTestId("add-url-save")).toBeDisabled();
    // The typed secret is not repeated anywhere in the dialog.
    expect(within(d).getByTestId("add-url-status").textContent).not.toContain("secret");

    await user.clear(within(d).getByTestId("add-url-input"));
    await user.paste("not a link");
    expect(await within(d).findByTestId("add-url-error")).toHaveTextContent("not a web link");
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("add-url-dialog")).toBeNull();
    expect(screen.getByTestId("add-url-button")).toHaveFocus();
    expect(screen.getByTestId("result-count")).toHaveTextContent("7 items");
  });

  it("a duplicate opens the existing item instead of saving (CAP-06)", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    const d = await openDialog(user);
    await user.paste("https://www.youtube.com/watch?v=aaaaaaaaaaa&si=tracking");
    expect(await within(d).findByTestId("add-url-existing")).toHaveTextContent("Already in your library");
    expect(within(d).queryByTestId("add-url-save")).toBeNull();
    await user.keyboard("{Enter}");
    expect(await screen.findByTestId("toast")).toHaveTextContent("already in your library");
    expect(screen.getByTestId("result-count")).toHaveTextContent("7 items");
    expect(await screen.findByTestId("inspector-url")).toHaveTextContent("https://www.youtube.com/watch?v=aaaaaaaaaaa");
  });

  it("a removed item is offered back; Restore brings it back with its notes (AC-45)", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    const d = await openDialog(user);
    await user.paste("https://youtu.be/removedVid1");
    expect(await within(d).findByTestId("add-url-removed")).toHaveTextContent("Recently removed");
    expect(within(d).getByTestId("add-url-restore")).toBeEnabled();
    await user.keyboard("{Enter}");
    expect(await screen.findByTestId("toast")).toHaveTextContent("Restored “Kaldırılmış bir video”.");
    await waitFor(() => expect(screen.getByTestId("result-count")).toHaveTextContent("8 items"));
    expect(await screen.findByTestId("inspector-notes")).toHaveTextContent("Bu not geri gelmeli");
  });

  it("Cancel on a removed item changes nothing", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    const d = await openDialog(user);
    await user.paste("https://youtu.be/removedVid1");
    await within(d).findByTestId("add-url-removed");
    await user.click(within(d).getByTestId("add-url-cancel"));
    expect(screen.queryByTestId("add-url-dialog")).toBeNull();
    expect(screen.getByTestId("result-count")).toHaveTextContent("7 items");
  });

  it("a playlist is saved as one bookmark with a warning", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    const d = await openDialog(user);
    await user.paste("https://www.youtube.com/playlist?list=PLx");
    expect(await within(d).findByTestId("add-url-warning")).toHaveTextContent("saved as one bookmark");
    expect(within(d).getByTestId("add-url-save")).toBeEnabled();
  });

  it("offline: says the bookmark is saved now and details wait (CAP-04)", async () => {
    const user = userEvent.setup();
    const online = Object.getOwnPropertyDescriptor(Navigator.prototype, "onLine");
    Object.defineProperty(navigator, "onLine", { configurable: true, get: () => false });
    try {
      render(<App client={createFixtureClient()} />);
      const d = await openDialog(user);
      await user.paste("https://x.com/a/status/1");
      expect(await within(d).findByTestId("add-url-preview")).toHaveTextContent("You are offline. The bookmark is saved now");
    } finally {
      delete (navigator as unknown as Record<string, unknown>).onLine;
      if (online) Object.defineProperty(Navigator.prototype, "onLine", online);
    }
  });

  it("Ctrl+K, paste a link, Enter: Add URL opens with it (§9.4)", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    await cards();
    await user.keyboard("{Control>}k{/Control}");
    expect(screen.getByTestId("search-input")).toHaveFocus();
    await user.paste("www.tiktok.com/@a/video/123");
    expect(screen.getByTestId("url-suggestion")).toHaveTextContent("Save this link as a bookmark");
    // The link is not searched for.
    expect(screen.queryByTestId("library-no-match")).toBeNull();
    await user.keyboard("{Enter}");
    const d = await dialog();
    expect(within(d).getByTestId("add-url-input")).toHaveValue("https://www.tiktok.com/@a/video/123");
    expect(await within(d).findByTestId("add-url-platform")).toHaveTextContent("TikTok");
    await user.keyboard("{Enter}");
    expect(await screen.findByTestId("toast")).toHaveTextContent("Saved “TikTok · 123”");
  });

  it("Ctrl+V outside a text field opens Add URL; inside a field it pastes normally (CAP-01)", async () => {
    render(<App client={createFixtureClient()} />);
    const [first] = await cards();
    first!.focus();
    fireEvent.paste(first!, { clipboardData: { getData: () => "https://vk.com/video-9_9" } });
    const d = await dialog();
    expect(within(d).getByTestId("add-url-input")).toHaveValue("https://vk.com/video-9_9");
    fireEvent.keyDown(d, { key: "Escape" });
    expect(screen.queryByTestId("add-url-dialog")).toBeNull();

    fireEvent.paste(screen.getByTestId("search-input"), { clipboardData: { getData: () => "https://vk.com/video-9_9" } });
    expect(screen.queryByTestId("add-url-dialog")).toBeNull();
    fireEvent.paste(first!, { clipboardData: { getData: () => "just words" } });
    expect(screen.queryByTestId("add-url-dialog")).toBeNull();
  });

  it("first use offers Add URL", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient([])} />);
    await user.click(await screen.findByTestId("empty-add-url"));
    expect(await dialog()).toBeInTheDocument();
  });

  it("removed fixture items stay out of lists and the inspector", async () => {
    expect(fixtureRecords().filter((r) => r.removed)).toHaveLength(1);
    render(<App client={createFixtureClient()} />);
    expect(await cards()).toHaveLength(7);
  });
});

describe("link detection", () => {
  it("offers Add URL only for text that is clearly a link", () => {
    for (const t of ["https://a.b/c", "http://x.y", "  www.youtube.com/watch?v=1 ", "HTTPS://A.B"]) expect(looksLikeUrl(t), t).toBe(true);
    for (const t of ["", "deniz kenarı", "https://a.b c", "ftp://a.b", "youtube.com"]) expect(looksLikeUrl(t), t).toBe(false);
    expect(withScheme(" www.a.b/x ")).toBe("https://www.a.b/x");
    expect(withScheme("https://a.b")).toBe("https://a.b");
  });
});
