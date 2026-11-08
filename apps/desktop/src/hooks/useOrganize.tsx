import { useCallback, useEffect, useState } from "react";
import { ConfirmDialog, NameDialog, PickDialog } from "../components/Dialogs";
import { MergeDialog } from "../components/DuplicatesView";
import { OrphansDialog } from "../components/OrphansDialog";
import { SmartCollectionDialog } from "../components/SmartCollectionDialog";
import type { Scope, SidebarAction } from "../components/Sidebar";
import type { LibraryClient, LibraryItemDto, OpResult, Sidebar } from "../ipc/types";
import { opMessage, orgErrorMessage, undoMessage } from "../labels";
import { tagKeyOf } from "../tags";

export type OrganizeDialog =
  | { kind: "tag"; ids: string[] }
  | { kind: "add-to-collection"; ids: string[] }
  | { kind: "sidebar"; action: SidebarAction }
  | { kind: "delete-permanently"; ids: string[] }
  | { kind: "merge"; candidateId: string; target: LibraryItemDto }
  | { kind: "orphans" };

export interface Notify {
  (text: string, opts?: { tone?: "info" | "error"; undo?: boolean }): void;
}

/**
 * Organize actions and their dialogs (STATE.md F2.1–F2.3). Every change
 * reloads the list, the sidebar and the details panel; an undoable one
 * offers Undo in its toast (§7.3).
 */
export function useOrganize({ client, notify, scope, onScope }: { client: LibraryClient; notify: Notify; scope: Scope; onScope: (s: Scope) => void }) {
  const [sidebar, setSidebar] = useState<Sidebar | null>(null);
  const [dialog, setDialog] = useState<OrganizeDialog | null>(null);
  const [version, setVersion] = useState(0);

  const refresh = useCallback(() => setVersion((v) => v + 1), []);

  useEffect(() => {
    let live = true;
    client.getSidebar().then(
      (s) => live && setSidebar(s),
      () => {},
    );
    return () => {
      live = false;
    };
  }, [client, version]);

  /** Runs an operation and reports it; errors become a toast. */
  const run = useCallback(
    async (f: () => Promise<OpResult>, done: string, unchangedReason?: string) => {
      try {
        const r = await f();
        notify(opMessage(done, r, unchangedReason), { undo: !!r.undo });
        return r;
      } catch (e) {
        notify(orgErrorMessage(e), { tone: "error" });
        return null;
      } finally {
        refresh();
      }
    },
    [notify, refresh],
  );

  const undo = useCallback(async () => {
    try {
      const r = await client.undo();
      notify(r ? undoMessage(r) : "Nothing to undo.");
    } catch (e) {
      notify(orgErrorMessage(e), { tone: "error" });
    } finally {
      refresh();
    }
  }, [client, notify, refresh]);

  const actions = {
    favorite: (ids: string[], v: boolean) => run(() => client.setFavorite(ids, v), v ? "Added {n} to Favorites." : "Removed {n} from Favorites."),
    archive: (ids: string[], v: boolean) => run(() => client.setArchived(ids, v), v ? "Archived {n}." : "Moved {n} out of the archive."),
    remove: (ids: string[]) => run(() => client.removeItems(ids), "Moved {n} to Recently removed."),
    restore: (ids: string[]) => run(() => client.restoreItems(ids), "Restored {n}."),
    untag: (ids: string[], tagId: string, name: string) => run(() => client.untagItems(ids, tagId), `Removed #${name} from {n}.`, "did not have it"),
    removeFromCollection: (ids: string[], collectionId: string) => run(() => client.removeFromCollection(ids, collectionId), "Took {n} out of the collection.", "were not in it"),
  };

  const close = () => setDialog(null);
  const nameOf = (id: string) => sidebar?.collections.find((c) => c.id === id)?.name ?? "";
  const tags = sidebar?.tags ?? [];
  const collections = sidebar?.collections ?? [];
  const indent = (depth: number) => " ".repeat(depth);

  /** Leaves a view whose collection, tag or smart collection is gone. */
  const leaveIf = (kind: Scope["kind"], ids: string[]) => {
    if (scope.kind === kind && "id" in scope && ids.includes(scope.id)) onScope({ kind: "none" });
  };

  const node = (() => {
    if (!dialog) return null;
    switch (dialog.kind) {
      case "tag":
        return (
          <NameDialog
            title={dialog.ids.length === 1 ? "Add a tag" : `Tag ${dialog.ids.length} items`}
            label="Tag"
            confirm="Add tag"
            max={100}
            suggestions={tags.map((t) => t.name)}
            onClose={close}
            onSubmit={async (name) => {
              close();
              await run(() => client.tagItems(dialog.ids, name), `Tagged {n} with #${name}.`, "already had it");
            }}
          />
        );
      case "add-to-collection":
        return collections.length === 0 ? (
          <NameDialog
            title="New collection"
            label="Name"
            confirm="Create and add"
            max={200}
            onClose={close}
            onSubmit={async (name) => {
              close();
              try {
                const col = await client.createCollection(name, null);
                await run(() => client.addToCollection(dialog.ids, col.id), `Added {n} to ${col.name}.`, "were already in it");
              } catch (e) {
                notify(orgErrorMessage(e), { tone: "error" });
              }
            }}
          />
        ) : (
          <PickDialog
            title={dialog.ids.length === 1 ? "Add to collection" : `Add ${dialog.ids.length} items to a collection`}
            label="Collection"
            confirm="Add"
            options={collections.map((c) => ({
              value: c.id,
              label: `${indent(c.depth)}${c.name}`,
            }))}
            onClose={close}
            onSubmit={async (id) => {
              close();
              await run(() => client.addToCollection(dialog.ids, id), `Added {n} to ${nameOf(id)}.`, "were already in it");
            }}
          />
        );
      case "delete-permanently":
        return (
          <ConfirmDialog
            title={`Delete ${dialog.ids.length === 1 ? "this item" : `${dialog.ids.length} items`} for good?`}
            body={<p>This can't be undone. Notes, tags and collection places of these items are deleted. Imported files stay where they are.</p>}
            checkbox="Also delete video files VideoVault downloaded for them"
            confirm="Delete for good"
            danger
            onClose={close}
            onSubmit={async ({ checked }) => {
              close();
              try {
                const r = await client.deleteItemsPermanently(dialog.ids, checked);
                const kept = r.orphaned ? ` ${r.orphaned} downloaded ${r.orphaned === 1 ? "file was" : "files were"} kept in Orphaned media.` : "";
                const failed = r.files.failed ? ` ${r.files.failed} ${r.files.failed === 1 ? "file" : "files"} could not be deleted.` : "";
                notify(`Deleted ${r.items} ${r.items === 1 ? "item" : "items"}.${kept}${failed}`);
              } catch (e) {
                notify(orgErrorMessage(e), { tone: "error" });
              } finally {
                refresh();
              }
            }}
          />
        );
      case "merge":
        return (
          <MergeDialog
            client={client}
            candidateId={dialog.candidateId}
            target={dialog.target}
            onClose={close}
            onMerged={async () => {
              const r = await client.mergeItems(dialog.candidateId, dialog.target.id);
              close();
              notify(`Merged into “${dialog.target.title}”.`, {
                undo: !!r.undo,
              });
              refresh();
            }}
          />
        );
      case "orphans":
        return <OrphansDialog client={client} onChanged={(m) => (notify(m), refresh())} onClose={close} />;
      case "sidebar":
        return sidebarDialog(dialog.action);
    }
  })();

  function sidebarDialog(a: SidebarAction) {
    const safe = async (f: () => Promise<unknown>, msg: string) => {
      close();
      try {
        await f();
        notify(msg);
      } catch (e) {
        notify(orgErrorMessage(e), { tone: "error" });
      } finally {
        refresh();
      }
    };
    switch (a.kind) {
      case "new-collection":
        return (
          <NameDialog
            title={a.parentId ? `New collection in ${nameOf(a.parentId)}` : "New collection"}
            label="Name"
            confirm="Create"
            max={200}
            onClose={close}
            onSubmit={(name) => safe(() => client.createCollection(name, a.parentId), `Created ${name}.`)}
          />
        );
      case "rename-collection":
        return (
          <NameDialog
            title="Rename collection"
            label="Name"
            initial={a.collection.name}
            confirm="Rename"
            max={200}
            onClose={close}
            onSubmit={(name) => safe(() => client.renameCollection(a.collection.id, name), `Renamed to ${name}.`)}
          />
        );
      case "move-collection": {
        // Not into itself or below itself (no cycles, §7.3).
        const below = new Set<string>([a.collection.id]);
        for (const c of collections) if (c.parentId && below.has(c.parentId)) below.add(c.id);
        return (
          <PickDialog
            title={`Move ${a.collection.name}`}
            label="Put it in"
            confirm="Move"
            options={[
              { value: "__top", label: "Top level" },
              ...collections.map((c) => ({
                value: c.id,
                label: `${indent(c.depth)}${c.name}`,
                disabled: below.has(c.id),
              })),
            ].filter((o) => o.value !== (a.collection.parentId ?? "__top"))}
            onClose={close}
            onSubmit={(v) => safe(() => client.moveCollection(a.collection.id, v === "__top" ? null : v), `Moved ${a.collection.name}.`)}
          />
        );
      }
      case "delete-collection": {
        const hasChildren = collections.some((c) => c.parentId === a.collection.id);
        return (
          <ConfirmDialog
            title={`Delete ${a.collection.name}?`}
            body={<p>Items in it stay in your library, and no files are deleted. Only the collection and its list of items go.</p>}
            choices={
              hasChildren
                ? [
                    {
                      value: "move_up",
                      label: "Keep its sub-collections (move them up one level)",
                    },
                    {
                      value: "delete_all",
                      label: "Delete its sub-collections too",
                    },
                  ]
                : undefined
            }
            confirm="Delete collection"
            danger
            onClose={close}
            onSubmit={({ choice }) =>
              safe(async () => {
                const ids = choice === "delete_all" ? collections.filter((c) => c.id === a.collection.id || isBelow(c.id, a.collection.id)).map((c) => c.id) : [a.collection.id];
                await client.deleteCollection(a.collection.id, choice === "delete_all" ? "delete_all" : "move_up");
                leaveIf("collection", ids);
              }, `Deleted ${a.collection.name}. Its items are still in your library.`)
            }
          />
        );
      }
      case "new-smart":
      case "edit-smart":
        return (
          <SmartCollectionDialog
            initial={a.kind === "edit-smart" ? a.smart : null}
            tags={tags}
            collections={collections}
            onClose={close}
            onSubmit={async (name, rules) => {
              const r = await client.saveSmartCollection(a.kind === "edit-smart" ? a.smart.id : null, name, rules);
              close();
              notify(`Saved ${r.name}.`);
              refresh();
              onScope({ kind: "smart", id: r.id });
            }}
          />
        );
      case "delete-smart":
        return (
          <ConfirmDialog
            title={`Delete ${a.smart.name}?`}
            body={<p>Only the saved rules go; no item changes.</p>}
            confirm="Delete"
            danger
            onClose={close}
            onSubmit={() =>
              safe(async () => {
                await client.deleteSmartCollection(a.smart.id);
                leaveIf("smart", [a.smart.id]);
              }, `Deleted ${a.smart.name}.`)
            }
          />
        );
      case "rename-tag":
        return (
          <NameDialog
            title="Rename tag"
            label="Name"
            initial={a.tag.name}
            confirm="Rename"
            max={100}
            validate={(v) => (tags.some((t) => t.id !== a.tag.id && tagKeyOf(t.name) === tagKeyOf(v)) ? "A tag with this name already exists. Use Merge into… to combine them." : null)}
            onClose={close}
            onSubmit={async (name) => {
              close();
              await run(async () => client.renameTag(a.tag.id, name), `Renamed #${a.tag.name} to #${name}.`, "already had that name");
            }}
          />
        );
      case "merge-tag":
        return (
          <PickDialog
            title={`Merge #${a.tag.name} into…`}
            label="Tag to keep"
            confirm="Merge"
            note={`Items tagged #${a.tag.name} get the chosen tag, and #${a.tag.name} goes away. Undo brings it back.`}
            options={tags.filter((t) => t.id !== a.tag.id).map((t) => ({ value: t.id, label: `#${t.name}` }))}
            onClose={close}
            onSubmit={async (target) => {
              close();
              const r = await run(() => client.mergeTags(a.tag.id, target), `Merged #${a.tag.name}.`);
              if (r) leaveIf("tag", [a.tag.id]);
            }}
          />
        );
      case "delete-tag":
        return (
          <ConfirmDialog
            title={`Delete #${a.tag.name}?`}
            body={<p>The tag is taken off {a.tag.items === 1 ? "1 item" : `${a.tag.items} items`}. The items stay. This can't be undone.</p>}
            confirm="Delete tag"
            danger
            onClose={close}
            onSubmit={() =>
              safe(async () => {
                await client.deleteTag(a.tag.id);
                leaveIf("tag", [a.tag.id]);
              }, `Deleted #${a.tag.name}.`)
            }
          />
        );
    }
  }

  function isBelow(id: string, ancestor: string): boolean {
    let cur = collections.find((c) => c.id === id)?.parentId ?? null;
    while (cur) {
      if (cur === ancestor) return true;
      cur = collections.find((c) => c.id === cur)?.parentId ?? null;
    }
    return false;
  }

  return {
    sidebar,
    version,
    refresh,
    dialog: node,
    open: setDialog,
    undo,
    actions,
  };
}
