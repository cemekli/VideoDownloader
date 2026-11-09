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

// Organize commands (F2.1–F2.3): each client method sends the contract's
// argument names to the command of the same name.
const ORGANIZE_CALLS: Array<[string, (c: ReturnType<typeof createTauriClient>, a: Record<string, unknown>) => Promise<unknown>]> = [
  ["get_sidebar", (c) => c.getSidebar()],
  ["undo", (c) => c.undo()],
  ["tag_items", (c, a) => c.tagItems(a.itemIds as string[], a.name as string)],
  ["untag_items", (c, a) => c.untagItems(a.itemIds as string[], a.tagId as string)],
  ["add_to_collection", (c, a) => c.addToCollection(a.itemIds as string[], a.collectionId as string)],
  ["remove_from_collection", (c, a) => c.removeFromCollection(a.itemIds as string[], a.collectionId as string)],
  ["set_favorite", (c, a) => c.setFavorite(a.itemIds as string[], a.value as boolean)],
  ["set_archived", (c, a) => c.setArchived(a.itemIds as string[], a.value as boolean)],
  ["remove_items", (c, a) => c.removeItems(a.itemIds as string[])],
  ["restore_items", (c, a) => c.restoreItems(a.itemIds as string[])],
  ["delete_items_permanently", (c, a) => c.deleteItemsPermanently(a.itemIds as string[], a.deleteMedia as boolean)],
  ["rename_tag", (c, a) => c.renameTag(a.id as string, a.name as string)],
  ["merge_tags", (c, a) => c.mergeTags(a.sourceId as string, a.targetId as string)],
  ["delete_tag", (c, a) => c.deleteTag(a.id as string)],
  ["create_collection", (c, a) => c.createCollection(a.name as string, a.parentId as string | null)],
  ["rename_collection", (c, a) => c.renameCollection(a.id as string, a.name as string)],
  ["move_collection", (c, a) => c.moveCollection(a.id as string, a.parentId as string | null)],
  ["delete_collection", (c, a) => c.deleteCollection(a.id as string, a.policy as "move_up")],
  ["save_smart_collection", (c, a) => c.saveSmartCollection(a.id as string | null, a.name as string, a.rules as never)],
  ["delete_smart_collection", (c, a) => c.deleteSmartCollection(a.id as string)],
  ["set_notes", (c, a) => c.setNotes(a.id as string, a.revision as number, a.notes as string)],
  ["list_duplicates", (c) => c.listDuplicates()],
  ["preview_merge", (c, a) => c.previewMerge(a.candidateId as string, a.targetId as string)],
  ["merge_items", (c, a) => c.mergeItems(a.candidateId as string, a.targetId as string)],
  ["dismiss_duplicate", (c, a) => c.dismissDuplicate(a.candidateId as string)],
  ["list_orphans", (c) => c.listOrphans()],
  ["reimport_orphan", (c, a) => c.reimportOrphan(a.id as string)],
  ["delete_orphan", (c, a) => c.deleteOrphan(a.id as string)],
];

describe("Tauri client: organize commands", () => {
  it.each(ORGANIZE_CALLS)("%s sends the contract arguments", async (cmd, callIt) => {
    const spec = (contract.commands as Record<string, { args: Record<string, unknown>; response: unknown }>)[cmd]!;
    const calls: Array<[string, unknown]> = [];
    mockIPC((name, args) => {
      calls.push([name, args]);
      return spec.response;
    });
    await expect(callIt(createTauriClient(), spec.args)).resolves.toEqual(spec.response);
    expect(calls).toEqual([[cmd, spec.args]]);
  });
});

// Download commands (F3): the UI sends a preset name and ids, nothing else.
const DOWNLOAD_CALLS: Array<[string, (c: ReturnType<typeof createTauriClient>, a: Record<string, unknown>) => Promise<unknown>]> = [
  ["get_tool_health", (c) => c.getToolHealth()],
  ["list_jobs", (c) => c.listJobs()],
  ["get_download_options", (c, a) => c.getDownloadOptions(a.itemId as string)],
  ["start_download", (c, a) => c.startDownload(a.itemId as string, a.preset as "720")],
  ["fetch_metadata", (c, a) => c.fetchMetadata(a.itemId as string)],
  ["cancel_job", (c, a) => c.cancelJob(a.jobId as string)],
  ["retry_job", (c, a) => c.retryJob(a.jobId as string)],
  ["remove_partial", (c, a) => c.removePartial(a.jobId as string)],
  ["get_storage_usage", (c) => c.getStorageUsage()],
  ["set_storage_quota", (c, a) => c.setStorageQuota(a.bytes as number)],
  ["move_media_folder", (c) => c.moveMediaFolder()],
];

describe("Tauri client: download commands", () => {
  it.each(DOWNLOAD_CALLS)("%s sends the contract arguments", async (cmd, callIt) => {
    const spec = (contract.commands as Record<string, { args: Record<string, unknown>; response: unknown }>)[cmd]!;
    const calls: Array<[string, unknown]> = [];
    mockIPC((name, args) => {
      calls.push([name, args]);
      return spec.response;
    });
    await expect(callIt(createTauriClient(), spec.args)).resolves.toEqual(spec.response);
    expect(calls).toEqual([[cmd, spec.args]]);
  });
});

// Session commands (F5.2): a Firefox profile is named, never a folder.
const SESSION_CALLS: Array<[string, (c: ReturnType<typeof createTauriClient>, a: Record<string, unknown>) => Promise<unknown>]> = [
  ["list_sessions", (c) => c.listSessions()],
  ["connect_session", (c, a) => c.connectSession(a.provider as "reddit", "firefox", a.browserProfile as string, a.scope as ["download"])],
  ["set_session_consent", (c, a) => c.setSessionConsent(a.id as string, a.scope as ["download"])],
  ["clear_session", (c, a) => c.clearSession(a.id as string)],
  ["delete_session", (c, a) => c.deleteSession(a.id as string)],
];

describe("Tauri client: session commands", () => {
  it.each(SESSION_CALLS)("%s sends the contract arguments", async (cmd, callIt) => {
    const spec = (contract.commands as Record<string, { args: Record<string, unknown>; response: unknown }>)[cmd]!;
    const calls: Array<[string, unknown]> = [];
    mockIPC((name, args) => {
      calls.push([name, args]);
      return spec.response;
    });
    await expect(callIt(createTauriClient(), spec.args)).resolves.toEqual(spec.response);
    expect(calls).toEqual([[cmd, spec.args]]);
  });
});

describe("fixture client", () => {
  it("answers the session commands with the contract shapes", async () => {
    const c = createFixtureClient();
    const keys = (o: object) => Object.keys(o).sort();
    const o = await c.listSessions();
    expect(keys(o)).toEqual(keys(contract.commands.list_sessions.response));
    const s = await c.connectSession("reddit", "firefox", "default-release", ["download"]);
    expect(keys(s)).toEqual(keys(contract.commands.connect_session.response));
    expect(keys((await c.listSessions()).browserProfiles[0]!)).toEqual(keys(contract.commands.list_sessions.response.browserProfiles[0]!));
    await expect(c.connectSession("reddit", "firefox", "nope", ["download"])).rejects.toMatchObject({ code: "COOKIE_ACCESS_FAILED" });
  });

  it("answers the download reads with the contract shapes", async () => {
    const c = createFixtureClient();
    const keys = (o: object) => Object.keys(o).sort();
    const o = await c.getDownloadOptions("00000000-0000-4000-8000-000000000002");
    expect(keys(o)).toEqual(keys(contract.commands.get_download_options.response));
    const j = await c.startDownload("00000000-0000-4000-8000-000000000002", "720");
    expect(keys(j)).toEqual(keys(contract.commands.start_download.response));
    expect(keys(await c.getToolHealth())).toEqual(keys(contract.commands.get_tool_health.response));
    expect(keys(await c.getStorageUsage())).toEqual(keys(contract.commands.get_storage_usage.response));
    await expect(c.startDownload("00000000-0000-4000-8000-000000000005", "best")).rejects.toMatchObject({ code: "INPUT_REJECTED" });
  });

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

  it("answers the organize reads with the contract shapes", async () => {
    const c = createFixtureClient();
    const keys = (o: object) => Object.keys(o).sort();
    const sb = await c.getSidebar();
    expect(keys(sb)).toEqual(keys(contract.commands.get_sidebar.response));
    expect(keys(sb.tags[0]!)).toEqual(keys(contract.commands.get_sidebar.response.tags[0]!));
    const op = await c.tagItems(["00000000-0000-4000-8000-000000000001"], "deniz");
    expect(keys(op)).toEqual(keys(contract.commands.tag_items.response));
    const dups = await c.listDuplicates();
    expect(keys(dups[0]!)).toEqual(keys(contract.commands.list_duplicates.response[0]!));
    const p = await c.previewMerge(dups[0]!.id, dups[0]!.a.id);
    expect(keys(p)).toEqual(keys(contract.commands.preview_merge.response));
  });

  it("picks a scenario from the URL, defaulting to the fixture set", () => {
    expect(scenarioFromUrl("?fixture=empty")).toBe("empty");
    expect(scenarioFromUrl("?fixture=many")).toBe("many");
    expect(scenarioFromUrl("?fixture=../../etc")).toBe("default");
    expect(scenarioFromUrl("")).toBe("default");
  });
});
