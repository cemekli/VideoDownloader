import { describe, expect, it } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { createFixtureClient } from "./ipc/client";

// STATE.md F5.3–F5.5 against the fixture backend: the Sessions screen
// (SPEC §9.3), connecting with a cookie file, Check now (§12.1 evidence),
// consent, Clear, Reconnect and Remove. The rules themselves are tested in
// Rust (crates/vv-engine/tests/engine.rs `f5_*`).

async function openSessions(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByTestId("nav-sessions"));
  return screen.findByTestId("sessions");
}

describe("Sessions screen", () => {
  it("lists every site; a connected one shows its state, check time and what it may do", async () => {
    const user = userEvent.setup();
    const client = createFixtureClient(undefined, "login-required");
    await client.connectSession("reddit", "firefox", "default-release", ["download"]);
    render(<App client={client} />);
    const screenEl = await openSessions(user);
    const row = await within(screenEl).findByTestId("session");
    expect(row).toHaveAttribute("data-provider", "reddit");
    expect(row).toHaveTextContent("Firefox (default-release)");
    expect(within(row).getByTestId("session-state")).toHaveTextContent("Not checked");
    expect(within(row).getByTestId("session-verified")).toHaveTextContent("Not checked yet");
    expect(within(row).getByTestId("session-allow-download")).toBeChecked();
    expect(within(row).getByTestId("session-allow-verify")).not.toBeChecked();
    // "Check now" stays visible but waits until checking is allowed.
    expect(within(row).getByTestId("session-verify")).toBeDisabled();
    expect(within(row).getByTestId("session-verify-hint")).toHaveTextContent("Checking the sign-in");
    await user.click(within(row).getByTestId("session-allow-verify"));
    await waitFor(() => expect(within(screenEl).getByTestId("session-verify")).toBeEnabled());
    expect(within(screenEl).queryByTestId("session-verify-hint")).toBeNull();
    // The other sites can be connected or their sign-in page opened.
    const sites = within(screenEl).getAllByTestId("session-site");
    expect(sites.map((s) => s.dataset.provider)).not.toContain("reddit");
    expect(sites.map((s) => s.dataset.provider)).toContain("vk");
  });

  it("Check now proves the session with a saved video that needs a login (§12.1)", async () => {
    const user = userEvent.setup();
    const client = createFixtureClient(undefined, "login-required");
    await client.connectSession("instagram", "firefox", "default-release", ["metadata", "download", "verify"]);
    render(<App client={client} />);
    const row = await within(await openSessions(user)).findByTestId("session");
    await user.click(within(row).getByTestId("session-verify"));
    await screen.findByText(/Signed in to Instagram: the check passed/);
    await waitFor(() => expect(screen.getByTestId("session")).toHaveAttribute("data-state", "session_available"));
    expect(within(screen.getByTestId("session")).getByTestId("session-verified")).toHaveTextContent("checked");
  });

  it("Check now says what is wrong: still signed out, or nothing to check with yet", async () => {
    const user = userEvent.setup();
    const client = createFixtureClient(undefined, "login-required");
    await client.connectSession("vk", "firefox", "work", ["verify"]);
    await client.connectSession("reddit", "firefox", "default-release", ["verify"]);
    render(<App client={client} />);
    const s = await openSessions(user);
    const rows = await within(s).findAllByTestId("session");
    const vk = rows.find((r) => r.dataset.provider === "vk")!;
    await user.click(within(vk).getByTestId("session-verify"));
    await screen.findByText(/VK still asks to sign in/);
    expect(vk).toHaveAttribute("data-state", "needs_verification");
    const reddit = rows.find((r) => r.dataset.provider === "reddit")!;
    await user.click(within(reddit).getByTestId("session-verify"));
    await screen.findByText(/save a Reddit video that needs you to sign in/);
  });

  it("connects a site with a cookie file only after consent; the file's path is never shown", async () => {
    const user = userEvent.setup();
    const client = createFixtureClient(undefined, "login-required");
    render(<App client={client} />);
    const s = await openSessions(user);
    const vk = within(s).getAllByTestId("session-site").find((r) => r.dataset.provider === "vk")!;
    await user.click(within(vk).getByTestId("session-site-connect"));
    const dialog = await screen.findByTestId("session-dialog");
    expect(within(dialog).getByTestId("session-provider")).toHaveValue("vk");
    await user.click(within(dialog).getByTestId("session-method-file"));
    expect(within(dialog).getByTestId("session-file-help")).toHaveTextContent("keeps only VK's cookies");
    expect(within(dialog).getByTestId("session-connect-submit")).toBeDisabled();
    await user.click(within(dialog).getByTestId("session-consent"));
    await user.click(within(dialog).getByTestId("session-connect-submit"));
    await waitFor(() => expect(screen.queryByTestId("session-dialog")).toBeNull());
    await screen.findByText(/You can delete the file now/);
    const row = await within(s).findByTestId("session");
    expect(row).toHaveAttribute("data-method", "cookie_file");
    expect(row).toHaveTextContent("Cookie file");
  });

  it("consent can be narrowed; Clear withdraws it; Reconnect brings a Firefox session back; Remove deletes it", async () => {
    const user = userEvent.setup();
    const client = createFixtureClient(undefined, "login-required");
    await client.connectSession("reddit", "firefox", "default-release", ["metadata", "download", "verify"]);
    render(<App client={client} />);
    const s = await openSessions(user);
    let row = await within(s).findByTestId("session");
    await user.click(within(row).getByTestId("session-allow-metadata"));
    await waitFor(async () => expect((await client.listSessions()).sessions[0]!.consent).toEqual(["download", "verify"]));
    row = within(s).getByTestId("session");
    await user.click(within(row).getByTestId("session-clear"));
    await waitFor(() => expect(within(s).getByTestId("session")).toHaveAttribute("data-state", "not_connected"));
    row = within(s).getByTestId("session");
    expect(row).toHaveTextContent("won't use this Firefox profile");
    expect(within(row).queryByTestId("session-verify")).toBeNull();
    await user.click(within(row).getByTestId("session-reconnect"));
    await waitFor(() => expect(within(s).getByTestId("session")).toHaveAttribute("data-state", "needs_verification"));
    await user.click(within(within(s).getByTestId("session")).getByTestId("session-delete"));
    await within(s).findByTestId("sessions-empty");
  });

  it("Settings points to the Sessions screen", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient(undefined, "login-required")} />);
    await user.click(await screen.findByTestId("settings-button"));
    await user.click(await screen.findByTestId("settings-open-sessions"));
    await screen.findByTestId("sessions");
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
