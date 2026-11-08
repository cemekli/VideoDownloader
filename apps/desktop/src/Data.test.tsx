import { describe, expect, it } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { createFixtureClient } from "./ipc/client";

// STATE.md F2.4–F2.8 against the fixture backend: player, settings, theme,
// Offline Mode, backups and the first run. Playback itself (frames, seeking)
// needs a real browser and is in e2e/player.spec.ts; the database side is in
// Rust (tests/playback_settings.rs, tests/portable.rs).

const cards = () => screen.findAllByTestId("library-card");
const openInspector = async (idEnd: string) => {
  const card = (await cards()).find((c) => c.dataset.id?.endsWith(idEnd))!;
  fireEvent.doubleClick(card);
  return screen.findByTestId("inspector");
};
const lastToast = async () => {
  await screen.findAllByTestId("toast");
  const all = screen.getAllByTestId("toast");
  return all[all.length - 1]!;
};

describe("Player", () => {
  it("offers to resume a saved position or start over (PLAY-02)", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    const inspector = await openInspector("0005");
    await user.click(await within(inspector).findByTestId("inspector-play"));
    const player = await screen.findByTestId("player");
    expect(within(player).getByRole("heading")).toHaveTextContent("Holiday clip");
    const resume = await within(player).findByTestId("player-resume-button");
    expect(resume).toHaveTextContent("Resume from 0:02");
    expect(resume).toHaveFocus();
    expect(within(player).getByTestId("player-start-over")).toBeVisible();
    // H.264 plays here: no warning, no "open in another player" offer.
    expect(within(player).queryByTestId("player-codec-warning")).toBeNull();
    expect(within(player).queryByTestId("player-open-external")).toBeNull();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByTestId("player")).toBeNull());
  });

  it("warns before playing HEVC and offers another player (AC-32, PLAY-04)", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    const inspector = await openInspector("0001");
    await user.click(await within(inspector).findByTestId("inspector-play"));
    const player = await screen.findByTestId("player");
    expect(await within(player).findByTestId("player-codec-warning")).toHaveTextContent("HEVC");
    await user.click(within(player).getByTestId("player-open-external"));
    expect(await lastToast()).toHaveTextContent("Opened in your default player.");
  });

  it("a file on a disconnected drive can be located again (DATA-03)", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    const inspector = await openInspector("0007");
    await user.click(await within(inspector).findByTestId("inspector-play"));
    const player = await screen.findByTestId("player");
    expect(await within(player).findByTestId("player-unavailable")).toBeVisible();
    await user.click(within(player).getByTestId("player-relink"));
    expect(await lastToast()).toHaveTextContent("File found again.");
    expect(await within(player).findByTestId("player-video")).toBeInTheDocument();
  });

  it("a bookmark without a file offers its source page instead (PLAY-03)", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    const inspector = await openInspector("0002");
    expect(within(inspector).queryByTestId("inspector-play")).toBeNull();
    await user.click(await within(inspector).findByTestId("inspector-open-source"));
    expect(await lastToast()).toHaveTextContent("Opened the source page in your browser.");
  });
});

describe("Settings", () => {
  it("the theme changes at once and is remembered (F2.8)", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    await cards();
    await user.click(screen.getByTestId("settings-button"));
    const settings = await screen.findByTestId("settings");
    await user.click(within(settings).getByTestId("theme-light"));
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("light"));
    expect(localStorage.getItem("vv.theme")).toBe("light");
    await user.click(within(settings).getByTestId("theme-dark"));
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("dark"));
  });

  it("Offline Mode shows a banner, blocks the source page and turns off from the banner (AC-17)", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    await cards();
    await user.keyboard("{Control>},{/Control}");
    const settings = await screen.findByTestId("settings");
    await user.click(within(settings).getByTestId("settings-offline"));
    expect(await screen.findByTestId("offline-mode-banner")).toBeVisible();
    await user.click(within(settings).getByTestId("settings-close"));

    const inspector = await openInspector("0002");
    await user.click(await within(inspector).findByTestId("inspector-open-source"));
    expect(await lastToast()).toHaveTextContent(/Offline Mode/);

    await user.click(screen.getByTestId("offline-mode-off"));
    await waitFor(() => expect(screen.queryByTestId("offline-mode-banner")).toBeNull());
  });

  it("backs up on request and lists the backup", async () => {
    const user = userEvent.setup();
    render(<App client={createFixtureClient()} />);
    await cards();
    await user.click(screen.getByTestId("settings-button"));
    const settings = await screen.findByTestId("settings");
    await user.click(within(settings).getByTestId("backup-now"));
    const rows = await within(settings).findAllByTestId("backup-row");
    expect(rows).toHaveLength(1);
    expect(rows[0]).toHaveAttribute("data-kind", "manual");
  });
});

describe("First run", () => {
  it("walks through five steps by keyboard and keeps the choices (F2.8)", async () => {
    const user = userEvent.setup();
    const client = createFixtureClient(undefined, "first-run");
    render(<App client={client} />);
    const ob = await screen.findByTestId("onboarding");
    const title = () => within(ob).getByTestId("onboarding-title");
    await waitFor(() => expect(title()).toHaveFocus());
    expect(title()).toHaveTextContent("Welcome to VideoVault");

    await user.click(within(ob).getByTestId("onboarding-next"));
    expect(ob).toHaveAttribute("data-step", "legal");
    await user.click(within(ob).getByTestId("legal-ack"));
    expect(ob).toHaveAttribute("data-step", "storage");
    await waitFor(() => expect(title()).toHaveFocus());
    expect(within(ob).getByTestId("storage-line")).toHaveTextContent("Default folder (media)");
    await user.click(within(ob).getByTestId("choose-media-folder"));
    await waitFor(() => expect(within(ob).getByTestId("storage-line")).toHaveTextContent("Videos"));
    await user.click(within(ob).getByTestId("onboarding-next"));
    expect(ob).toHaveAttribute("data-step", "updates");
    await user.click(within(ob).getByTestId("onboarding-next"));
    expect(ob).toHaveAttribute("data-step", "theme");
    await user.click(within(ob).getByTestId("theme-light"));
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("light"));
    await user.click(within(ob).getByTestId("onboarding-next"));

    await waitFor(() => expect(screen.queryByTestId("onboarding")).toBeNull());
    const s = await client.getSettings();
    expect(s.onboardingCompletedAt).not.toBeNull();
    expect(s.legalAckAt).not.toBeNull();
    expect(s.theme).toBe("light");
  });

  it("Skip keeps the defaults and never comes back", async () => {
    const user = userEvent.setup();
    const client = createFixtureClient(undefined, "first-run");
    const { unmount } = render(<App client={client} />);
    await user.click(await screen.findByTestId("onboarding-skip"));
    await waitFor(() => expect(screen.queryByTestId("onboarding")).toBeNull());
    const s = await client.getSettings();
    expect(s.theme).toBe("dark");
    expect(s.updateCheck).toBe("manual");
    unmount();
    render(<App client={client} />);
    await cards();
    expect(screen.queryByTestId("onboarding")).toBeNull();
  });
});
