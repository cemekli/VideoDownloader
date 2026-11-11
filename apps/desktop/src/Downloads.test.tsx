import { describe, expect, it } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { createFixtureClient } from "./ipc/client";

// STATE.md F3.8 against the fixture backend: the Download dialog, the
// Downloads screen (phases, real and indeterminate progress, retry, cancel,
// partial files) and the storage and tool sections of Settings. The engine
// itself is tested in Rust (crates/vv-engine/tests).

const SLOW = { timeout: 4000 };
const cards = () => screen.findAllByTestId("library-card");
const openInspector = async (idEnd: string) => {
  const card = (await cards()).find((c) => c.dataset.id?.endsWith(idEnd))!;
  fireEvent.doubleClick(card);
  return screen.findByTestId("inspector");
};
const job = () => screen.findByTestId("job");

async function startDownload(user: ReturnType<typeof userEvent.setup>, preset: string, idEnd = "0002") {
  await user.click(await screen.findByTestId("nav-all"));
  const inspector = await openInspector(idEnd);
  await user.click(await within(inspector).findByTestId("inspector-download"));
  const dialog = await screen.findByTestId("download-dialog");
  await user.click(await within(dialog).findByTestId(`preset-${preset}`));
  expect(within(dialog).getByTestId("preset-720")).toBeEnabled();
  await user.click(within(dialog).getByTestId("download-start"));
  await waitFor(() => expect(screen.queryByTestId("download-dialog")).toBeNull());
  await user.click(screen.getByTestId("nav-downloads"));
}

describe("Downloads", () => {
  it("a download that needs a login waits; connecting a Firefox session with consent lets it finish (F5.2)", async () => {
    const user = userEvent.setup();
    const client = createFixtureClient(undefined, "login-required");
    render(<App client={client} />);
    await startDownload(user, "720");
    await waitFor(() => expect(screen.getByTestId("job")).toHaveAttribute("data-status", "blocked"), SLOW);
    expect(screen.getByTestId("job-blocked")).toHaveTextContent("sign in");
    await user.click(screen.getByTestId("job-connect-session"));
    const dialog = await screen.findByTestId("session-dialog");
    expect(within(dialog).getByTestId("session-provider")).toHaveValue("instagram");
    expect(within(dialog).getByTestId("session-browser-profile")).toHaveValue("default-release");
    // Nothing is connected without explicit consent.
    expect(within(dialog).getByTestId("session-connect-submit")).toBeDisabled();
    await user.click(within(dialog).getByTestId("session-consent"));
    await user.click(within(dialog).getByTestId("session-connect-submit"));
    await waitFor(() => expect(screen.queryByTestId("session-dialog")).toBeNull());
    await waitFor(() => expect(screen.getByTestId("downloads-done")).toBeVisible(), SLOW);
    const overview = await client.listSessions();
    expect(overview.sessions).toHaveLength(1);
    expect(overview.sessions[0]).toMatchObject({ provider: "instagram", state: "session_available", consent: ["metadata", "download", "verify"] });
  });

  it("a download shows its phase and real progress, then lands in Completed and the item is Downloaded", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    await startDownload(user, "720");
    const bar = await screen.findByTestId("job-progress", undefined, SLOW);
    expect(await job()).toHaveTextContent("Up to 720p");
    await waitFor(() => expect(bar).toHaveAttribute("aria-valuenow", "50"), SLOW);
    expect(await job()).toHaveTextContent("Downloading video");
    await waitFor(() => expect(screen.getByTestId("downloads-done")).toBeVisible(), SLOW);
    expect(within(screen.getByTestId("downloads-done")).getByTestId("job-status")).toHaveTextContent("Done");
    await user.click(screen.getByTestId("nav-all"));
    const card = (await cards()).find((c) => c.dataset.id?.endsWith("0002"))!;
    await waitFor(() => expect(card).toHaveTextContent("Downloaded"), SLOW);
  });

  it("an estimated total is shown as about, with an approximate bar", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    await startDownload(user, "best");
    const bar = await screen.findByTestId("job-progress", undefined, SLOW);
    await waitFor(() => expect(bar).toHaveAttribute("aria-valuenow", "50"), SLOW);
    expect(bar).toHaveAttribute("aria-valuetext", "50 MB of about 100 MB");
    expect(bar).not.toHaveAttribute("data-indeterminate");
  });

  it("an unknown total shows an indeterminate bar, not a made-up percentage (AC-09)", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    await startDownload(user, "audio");
    const bar = await screen.findByTestId("job-progress", undefined, SLOW);
    expect(bar).toHaveAttribute("data-indeterminate", "true");
    expect(bar).not.toHaveAttribute("aria-valuenow");
  });

  it("a network failure keeps the partial file; Try again succeeds; Cancel and Delete partial file work (Journey 6)", async () => {
    const user = userEvent.setup();
    const client = createFixtureClient(undefined, "download-error");
    render(<App client={client} />);
    await startDownload(user, "720");
    const failed = await screen.findByTestId("downloads-failed", undefined, SLOW);
    expect(within(failed).getByTestId("job-error")).toHaveTextContent("Can't reach the internet");
    expect(within(failed).getByTestId("job-partial")).toHaveTextContent("50 MB");
    await user.click(within(failed).getByTestId("job-retry"));
    await waitFor(() => expect(screen.getByTestId("downloads-done")).toBeVisible(), SLOW);

    // A second download, cancelled, then its partial file deleted.
    await startDownload(user, "best", "0003");
    await waitFor(() => expect(client.downloads.jobs()[0]!.partialBytes).toBeGreaterThan(0), SLOW);
    const active = screen.getByTestId("downloads-active");
    await user.click(within(active).getByTestId("job-cancel"));
    const canceled = await screen.findByTestId("downloads-failed");
    await user.click(await within(canceled).findByTestId("job-remove-partial"));
    expect(await screen.findByText(/Deleted the partial file/)).toBeVisible();
  });

  it("shows the download where it was started: top bar indicator and the item's details", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    const inspector = await openInspector("0002");
    await user.click(await within(inspector).findByTestId("inspector-download"));
    await user.click(await screen.findByTestId("download-start"));
    const queue = await screen.findByTestId("queue-indicator", undefined, SLOW);
    expect(queue).toHaveTextContent("1 downloading");
    const status = await within(inspector).findByTestId("inspector-download-status", undefined, SLOW);
    expect(within(inspector).queryByTestId("inspector-download")).toBeNull();
    await waitFor(() => expect(status).toHaveAttribute("data-status", "running"), SLOW);
    await user.click(within(status).getByTestId("inspector-open-downloads"));
    expect(await screen.findByTestId("downloads")).toBeVisible();
    await waitFor(() => expect(screen.queryByTestId("queue-indicator")).toBeNull(), SLOW);
  });

  it("is empty with a hint before the first download", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    await user.click(await screen.findByTestId("nav-downloads"));
    expect(await screen.findByTestId("downloads-empty")).toHaveTextContent("No downloads yet");
  });

  it("Settings shows storage use, sets a limit and lists the tool check (DATA-07, MT-11)", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    await user.click(await screen.findByTestId("settings-button"));
    const settings = await screen.findByTestId("settings");
    const usage = await within(settings).findByTestId("storage-usage");
    expect(usage).toHaveTextContent("Downloaded media 3.0 GB");
    await user.selectOptions(within(usage).getByTestId("storage-quota"), "10");
    await waitFor(() => expect(within(usage).getByTestId("storage-quota")).toHaveValue("10"));
    expect(within(settings).queryByTestId("storage-quota-exceeded")).toBeNull();
    const tools = await within(settings).findByTestId("tool-health");
    expect(within(tools).getAllByTestId("tool")).toHaveLength(5);
    expect(tools).toHaveTextContent("All tools passed their check.");
  });
});
