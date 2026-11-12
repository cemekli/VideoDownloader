import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TranscriptionBlock } from "./components/Transcription";
import { createFixtureClient } from "./ipc/client";

// STATE.md F6.1, SPEC §17.1, AC-20 against the fixture backend: no model and
// no download until the user agrees; the consent dialog shows size, free
// space, memory, source and licence. The rules themselves are tested in Rust
// (crates/vv-engine/tests/models.rs).

function setup() {
  const client = createFixtureClient();
  const notify = vi.fn();
  render(<TranscriptionBlock client={client} notify={notify} />);
  return { client, notify };
}

describe("Settings → Transcription", () => {
  it("starts with no model and downloads nothing when the dialog is cancelled", async () => {
    const user = userEvent.setup();
    const { client } = setup();
    const rows = await screen.findAllByTestId("model-row");
    expect(rows.map((r) => r.dataset.status)).toEqual(["not_installed", "not_installed"]);
    await user.click(within(rows[0]!).getByTestId("model-download"));
    const dlg = await screen.findByTestId("model-consent");
    expect(within(dlg).getByTestId("consent-size")).toHaveTextContent("MB");
    expect(within(dlg).getByTestId("consent-source")).toHaveTextContent("huggingface.co/ggerganov/whisper.cpp");
    expect(within(dlg).getByTestId("consent-license")).toHaveTextContent("MIT");
    expect(dlg).toHaveTextContent("not a measurement");
    await user.click(within(dlg).getByTestId("consent-cancel"));
    expect(screen.queryByTestId("model-consent")).toBeNull();
    const v = await client.listModels();
    expect(v.models.every((m) => m.status === "not_installed" && m.job === null)).toBe(true);
  });

  it("the backend refuses a download without consent", async () => {
    const client = createFixtureClient();
    await expect(client.downloadModel("whisper-tiny", false)).rejects.toMatchObject({ code: "INPUT_REJECTED" });
    expect((await client.listModels()).models[0]!.status).toBe("not_installed");
  });

  it("downloads after Download, shows progress and ends ready; Remove forgets it", async () => {
    const user = userEvent.setup();
    setup();
    const row = (await screen.findAllByTestId("model-row"))[0]!;
    await user.click(within(row).getByTestId("model-download"));
    await user.click(within(await screen.findByTestId("model-consent")).getByTestId("consent-agree"));
    await waitFor(() => expect(screen.getAllByTestId("model-row")[0]).toHaveAttribute("data-status", "pending"));
    expect(within(screen.getAllByTestId("model-row")[0]!).getByTestId("job-progress")).toBeInTheDocument();
    await waitFor(() => expect(screen.getAllByTestId("model-row")[0]).toHaveAttribute("data-status", "ready"), { timeout: 4000 });
    const ready = screen.getAllByTestId("model-row")[0]!;
    expect(within(ready).getByTestId("model-status")).toHaveTextContent("Ready");
    await user.click(within(ready).getByTestId("model-remove"));
    await waitFor(() => expect(screen.getAllByTestId("model-row")[0]).toHaveAttribute("data-status", "not_installed"));
  });

  it("warns and blocks the download when the disk is too small", async () => {
    const user = userEvent.setup();
    const client = createFixtureClient();
    const real = client.listModels.bind(client);
    client.listModels = async () => ({ ...(await real()), freeBytes: 1024 });
    render(<TranscriptionBlock client={client} notify={vi.fn()} />);
    const row = (await screen.findAllByTestId("model-row"))[0]!;
    await user.click(within(row).getByTestId("model-download"));
    const dlg = await screen.findByTestId("model-consent");
    expect(within(dlg).getByTestId("consent-no-space")).toBeInTheDocument();
    expect(within(dlg).getByTestId("consent-agree")).toBeDisabled();
  });
});
