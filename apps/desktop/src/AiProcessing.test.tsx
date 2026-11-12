import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AiProcessingView } from "./components/AiProcessing";
import { createFixtureClient } from "./ipc/client";
import type { SettingsPatch } from "./ipc/types";

// STATE.md F6.6, SPEC §9.3 "AI Processing", AI-A05 against the fixture
// backend: this PC's memory and processor, the CPU limit, the models with
// whether each fits, and the transcription queue with cancel. The memory
// rule itself is tested in Rust (crates/vv-engine/src/resources.rs,
// crates/vv-engine/tests/low_memory.rs).

const LOCAL = "00000000-0000-4000-8000-000000000005";

function setup(scenario?: "low-memory") {
  const client = createFixtureClient(undefined, scenario);
  const onSettings = vi.fn(async (p: SettingsPatch) => void (await client.updateSettings(p)));
  render(<AiProcessingView client={client} notify={vi.fn()} onSettings={onSettings} />);
  return { client, onSettings };
}

describe("AI Processing", () => {
  it("shows this PC, an empty queue and the models", async () => {
    setup();
    const view = await screen.findByTestId("ai-processing");
    expect(within(view).getByTestId("ai-memory")).toHaveTextContent(/free of 16(\.0)? GB/);
    expect(within(view).getByTestId("ai-transcriber")).toHaveTextContent("Ready");
    expect(view).toHaveTextContent("Graphics cards are not used yet");
    expect(view).toHaveTextContent("No transcriptions yet");
    expect(within(view).getByTestId("ai-threads")).toHaveTextContent("Uses 7 of 8 threads, at low priority");
    const rows = await within(view).findAllByTestId("model-row");
    expect(rows.length).toBeGreaterThan(0);
    expect(within(rows[0]!).getByTestId("model-fit")).toHaveAttribute("data-fit", "ok");
  });

  it("changes the CPU limit", async () => {
    const user = userEvent.setup();
    const { client, onSettings } = setup();
    await user.click(await screen.findByTestId("ai-cpu-light"));
    expect(onSettings).toHaveBeenCalledWith({ transcriptionCpu: "light" });
    await waitFor(() => expect(screen.getByTestId("ai-threads")).toHaveTextContent("Uses 2 of 8 threads"));
    expect((await client.getSettings()).transcriptionCpu).toBe("light");
  });

  it("on a low-memory PC marks the model that does not fit", async () => {
    setup("low-memory");
    const rows = await screen.findAllByTestId("model-row");
    const medium = rows.find((r) => r.dataset.model === "whisper-medium")!;
    expect(within(medium).getByTestId("model-fit")).toHaveAttribute("data-fit", "too_big");
    expect(within(medium).getByTestId("model-fit")).toHaveTextContent("more than this PC has");
    expect(screen.getByTestId("ai-memory")).toHaveTextContent(/free of 4(\.0)? GB/);
  });

  it("lists a running transcription and cancels it", async () => {
    const user = userEvent.setup();
    const client = createFixtureClient();
    await client.transcribeLocally(LOCAL, "whisper-base", "tr");
    render(<AiProcessingView client={client} notify={vi.fn()} onSettings={vi.fn()} />);
    const job = await screen.findByTestId("ai-job");
    expect(job).toHaveAttribute("data-status", "running");
    expect(job).toHaveTextContent("Turkish");
    await user.click(within(job).getByTestId("ai-job-cancel"));
    await waitFor(() => expect(screen.getByTestId("ai-job")).toHaveAttribute("data-status", "canceled"));
    expect(screen.getByTestId("ai-job")).toHaveTextContent("Stopped (Base)");
  });
});
