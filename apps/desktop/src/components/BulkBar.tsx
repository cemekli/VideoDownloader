import type { LibraryItemDto } from "../ipc/types";

/**
 * Actions on the selected items (ORG-04): one bar for one or many. In
 * Recently removed it offers Restore and Delete for good instead.
 */
export function BulkBar({
  selected,
  removedView,
  collectionName,
  onTag,
  onAddToCollection,
  onRemoveFromCollection,
  onFavorite,
  onArchive,
  onRemove,
  onRestore,
  onDeleteForGood,
}: {
  selected: LibraryItemDto[];
  removedView: boolean;
  /** Set inside a manual collection: offers "Remove from …". */
  collectionName: string | null;
  onTag: () => void;
  onAddToCollection: () => void;
  onRemoveFromCollection: () => void;
  onFavorite: (v: boolean) => void;
  onArchive: (v: boolean) => void;
  onRemove: () => void;
  onRestore: () => void;
  onDeleteForGood: () => void;
}) {
  if (selected.length === 0) return null;
  const allFav = selected.every((i) => i.isFavorite);
  const allArchived = selected.every((i) => i.archived);
  return (
    <div className="bulk-bar" role="toolbar" aria-label={`Actions for ${selected.length} selected`} data-testid="bulk-bar">
      {removedView ? (
        <>
          <button type="button" className="button" onClick={onRestore} data-testid="bulk-restore">
            Restore
          </button>
          <button type="button" className="button button-danger" onClick={onDeleteForGood} data-testid="bulk-delete">
            Delete for good…
          </button>
        </>
      ) : (
        <>
          <button type="button" className="button" onClick={onTag} data-testid="bulk-tag">
            Tag…
          </button>
          <button type="button" className="button" onClick={onAddToCollection} data-testid="bulk-collect">
            Add to collection…
          </button>
          {collectionName && (
            <button type="button" className="button" onClick={onRemoveFromCollection} data-testid="bulk-uncollect">
              Remove from {collectionName}
            </button>
          )}
          <button type="button" className="button" aria-pressed={allFav} onClick={() => onFavorite(!allFav)} data-testid="bulk-favorite">
            {allFav ? "★ Unfavorite" : "☆ Favorite"}
          </button>
          <button type="button" className="button" onClick={() => onArchive(!allArchived)} data-testid="bulk-archive">
            {allArchived ? "Unarchive" : "Archive"}
          </button>
          <button type="button" className="button" onClick={onRemove} data-testid="bulk-remove">
            Remove
          </button>
        </>
      )}
    </div>
  );
}
