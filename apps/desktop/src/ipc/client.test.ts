import { afterEach, describe, expect, it } from "vitest";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import contract from "@fixtures/ipc-contract.json";
import { CONTRACT_VERSION, IpcError, createFixtureClient, createTauriClient, toIpcError } from "./client";

// Unit tests for the real Tauri client (STATE.md F0.3). `mockIPC` replaces the
// webview bridge with a function, so these check what the client sends and how
// it maps answers; Rust `tests/ipc.rs` checks the backend side of the same
// contract (tests/fixtures/ipc-contract.json).

afterEach(() => clearMocks());

describe("Tauri client", () => {
  it("UI and fixture agree on the contract version", () => {
    expect(contract.contractVersion).toBe(CONTRACT_VERSION);
  });

  it("ping sends the nonce and returns the backend answer", async () => {
    const calls: Array<[string, unknown]> = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      return contract.commands.ping.response;
    });
    const pong = await createTauriClient().ping(contract.commands.ping.args.nonce);
    expect(calls).toEqual([["ping", { nonce: 42 }]]);
    expect(pong).toEqual(contract.commands.ping.response);
  });

  it("getStatus calls get_status", async () => {
    mockIPC((cmd) => (cmd === "get_status" ? contract.commands.get_status.response : Promise.reject("unexpected " + cmd)));
    await expect(createTauriClient().getStatus()).resolves.toEqual(contract.commands.get_status.response);
  });

  it("maps a backend DomainError to IpcError with the stable code (SPEC §18.3)", async () => {
    mockIPC(() => Promise.reject(contract.domainError));
    const err = await createTauriClient().getStatus().catch((e: unknown) => e);
    expect(err).toBeInstanceOf(IpcError);
    expect((err as IpcError).code).toBe("INTERNAL_ERROR");
    expect((err as IpcError).domain).toEqual(contract.domainError);
  });

  it("maps a Tauri-level rejection (ACL, unknown command) to IPC_FAILED", () => {
    const err = toIpcError("ping", "ping not allowed. Permissions associated with this command: allow-ping");
    expect(err.code).toBe("IPC_FAILED");
    expect(err.domain).toBeNull();
  });

  it("desktop library is empty until storage exists (F1)", async () => {
    await expect(createTauriClient().listItems()).resolves.toEqual([]);
  });
});

describe("fixture client", () => {
  it("reports itself as fixture data with the contract status shape", async () => {
    const c = createFixtureClient();
    expect(c.kind).toBe("fixture");
    expect(Object.keys(await c.getStatus()).sort()).toEqual(Object.keys(contract.commands.get_status.response).sort());
  });
});
