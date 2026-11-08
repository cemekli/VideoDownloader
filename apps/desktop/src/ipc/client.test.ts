import { afterEach, describe, expect, it } from "vitest";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import contract from "@fixtures/ipc-contract.json";
import { CONTRACT_VERSION, IpcError, createFixtureClient, createTauriClient, scenarioFromUrl, toIpcError } from "./client";
import type { ListQuery } from "./types";

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

  it("getDiagnostics calls get_diagnostics", async () => {
    mockIPC((cmd) => (cmd === "get_diagnostics" ? contract.commands.get_diagnostics.response : Promise.reject("unexpected " + cmd)));
    await expect(createTauriClient().getDiagnostics()).resolves.toEqual(contract.commands.get_diagnostics.response);
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

  it("listItems sends the query to list_items (F1.7)", async () => {
    const spec = contract.commands.list_items;
    const calls: Array<[string, unknown]> = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      return spec.response;
    });
    const page = await createTauriClient().listItems(spec.args.query as ListQuery);
    expect(calls).toEqual([["list_items", spec.args]]);
    expect(page).toEqual(spec.response);
  });

  it("getItem and importMedia call their commands", async () => {
    const calls: string[] = [];
    mockIPC((cmd, args) => {
      calls.push(`${cmd} ${JSON.stringify(args)}`);
      return cmd === "get_item" ? contract.commands.get_item.response : contract.commands.import_media.response;
    });
    const c = createTauriClient();
    await expect(c.getItem("x")).resolves.toEqual(contract.commands.get_item.response);
    await expect(c.importMedia("folder")).resolves.toEqual(contract.commands.import_media.response);
    expect(calls).toEqual(['get_item {"id":"x"}', 'import_media {"source":"folder"}']);
  });

  it("an import refused for missing tools keeps its code", async () => {
    mockIPC(() => Promise.reject({ ...contract.domainError, code: "TOOL_MISSING", messageKey: "error.tool_missing" }));
    const err = await createTauriClient().importMedia("files").catch((e: unknown) => e);
    expect((err as IpcError).code).toBe("TOOL_MISSING");
  });
});

describe("fixture client", () => {
  it("reports itself as fixture data with the contract status shape", async () => {
    const c = createFixtureClient();
    expect(c.kind).toBe("fixture");
    expect(Object.keys(await c.getStatus()).sort()).toEqual(Object.keys(contract.commands.get_status.response).sort());
  });

  it("answers list_items with the contract shape", async () => {
    const page = await createFixtureClient().listItems({ ...(contract.commands.list_items.args.query as ListQuery), text: "" });
    expect(Object.keys(page).sort()).toEqual(Object.keys(contract.commands.list_items.response).sort());
    expect(Object.keys(page.items[0]!).sort()).toEqual(Object.keys(contract.commands.list_items.response.items[0]!).sort());
  });

  it("answers get_item with the contract shape", async () => {
    const d = await createFixtureClient().getItem("00000000-0000-4000-8000-000000000001");
    expect(Object.keys(d).sort()).toEqual(Object.keys(contract.commands.get_item.response).sort());
    expect(Object.keys(d.assets[0]!).sort()).toEqual(Object.keys(contract.commands.get_item.response.assets[0]!).sort());
  });

  it("picks a scenario from the URL, defaulting to the fixture set", () => {
    expect(scenarioFromUrl("?fixture=empty")).toBe("empty");
    expect(scenarioFromUrl("?fixture=many")).toBe("many");
    expect(scenarioFromUrl("?fixture=../../etc")).toBe("default");
    expect(scenarioFromUrl("")).toBe("default");
  });
});
