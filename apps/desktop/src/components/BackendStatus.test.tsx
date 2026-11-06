import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { BackendStatus } from "./BackendStatus";
import { createFixtureClient } from "../ipc/client";
import type { AppStatus, StatusClient } from "../ipc/types";
import contract from "@fixtures/ipc-contract.json";

const status = contract.commands.get_status.response as AppStatus;
const desktop = (getStatus: () => Promise<AppStatus>): StatusClient => ({
  kind: "desktop",
  getStatus,
  ping: async (n) => ({ nonce: n, contractVersion: 1 }),
  getDiagnostics: async () => contract.commands.get_diagnostics.response,
});

describe("BackendStatus", () => {
  it("labels fixture data so it is not mistaken for a real library", async () => {
    render(<BackendStatus client={createFixtureClient()} />);
    expect(await screen.findByText("Fixture data")).toHaveAttribute("data-state", "ok");
  });

  it("shows the desktop app version when the backend answers", async () => {
    render(<BackendStatus client={desktop(async () => status)} />);
    expect(await screen.findByText("Desktop v0.0.1")).toBeInTheDocument();
  });

  it("shows when the backend is unreachable", async () => {
    render(<BackendStatus client={desktop(() => Promise.reject(new Error("x")))} />);
    expect(await screen.findByText("Backend unavailable")).toHaveAttribute("data-state", "down");
  });

  it("flags a contract version mismatch instead of misreading DTOs", async () => {
    render(<BackendStatus client={desktop(async () => ({ ...status, contractVersion: 2 }))} />);
    expect(await screen.findByText(/Version mismatch/)).toHaveAttribute("data-state", "mismatch");
  });
});
