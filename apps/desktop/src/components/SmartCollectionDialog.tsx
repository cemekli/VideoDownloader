import { useState } from "react";
import type { CollectionDto, ItemKind, Platform, SmartCollectionDto, SmartRules, TagDto } from "../ipc/types";
import { ITEM_KINDS, PLATFORMS } from "../ipc/types";
import { KIND_LABEL, PLATFORM_LABEL } from "../labels";
import { Modal } from "./Modal";

export const EMPTY_RULES: SmartRules = {
  text: "",
  platforms: [],
  kinds: [],
  tagIds: [],
  collectionIds: [],
  favoriteOnly: false,
  includeArchived: false,
  addedWithinDays: null,
  minDurationMs: null,
  maxDurationMs: null,
};

export function hasCondition(r: SmartRules): boolean {
  return !!(
    r.text.trim() ||
    r.platforms.length ||
    r.kinds.length ||
    r.tagIds.length ||
    r.collectionIds.length ||
    r.favoriteOnly ||
    r.addedWithinDays !== null ||
    r.minDurationMs !== null ||
    r.maxDurationMs !== null
  );
}

const toggle = <T,>(list: T[], v: T) => (list.includes(v) ? list.filter((x) => x !== v) : [...list, v]);
const minutes = (ms: number | null) => (ms === null ? "" : String(Math.round(ms / 60_000)));
const fromMinutes = (s: string) => (s.trim() === "" || Number.isNaN(Number(s)) ? null : Math.max(0, Math.round(Number(s) * 60_000)));

/**
 * Smart collection editor (ORG-07): every condition must hold. Only fixed
 * fields are stored, never a query string the backend would run (§7.3).
 */
export function SmartCollectionDialog({
  initial,
  tags,
  collections,
  onSubmit,
  onClose,
}: {
  initial: SmartCollectionDto | null;
  tags: TagDto[];
  collections: CollectionDto[];
  onSubmit: (name: string, rules: SmartRules) => Promise<void>;
  onClose: () => void;
}) {
  const [name, setName] = useState(initial?.name ?? "");
  // Rules naming a deleted tag/collection drop it here, so saving fixes them.
  const [r, setR] = useState<SmartRules>(() => {
    const base = structuredClone(initial?.rules ?? EMPTY_RULES);
    base.tagIds = base.tagIds.filter((t) => tags.some((x) => x.id === t));
    base.collectionIds = base.collectionIds.filter((c) => collections.some((x) => x.id === c));
    return base;
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const set = (patch: Partial<SmartRules>) => setR((x) => ({ ...x, ...patch }));
  const durationBad = r.minDurationMs !== null && r.maxDurationMs !== null && r.minDurationMs > r.maxDurationMs;
  const daysBad = r.addedWithinDays !== null && (r.addedWithinDays < 1 || r.addedWithinDays > 36_500);
  const ok = name.trim() !== "" && name.trim().length <= 200 && hasCondition(r) && !durationBad && !daysBad;
  return (
    <Modal title={initial ? "Edit smart collection" : "New smart collection"} onClose={onClose} testId="smart-dialog" wide>
      <form
        noValidate
        onSubmit={async (e) => {
          e.preventDefault();
          if (!ok || busy) return;
          setBusy(true);
          try {
            await onSubmit(name.trim(), r);
          } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
            setBusy(false);
          }
        }}
      >
        {initial?.invalid && (
          <p className="warning small" role="alert">
            A tag or collection these rules used was deleted. It has been taken out below; save to keep the rest.
          </p>
        )}
        <label className="field">
          <span>Name</span>
          <input type="text" value={name} onChange={(e) => setName(e.target.value)} data-testid="smart-name" />
        </label>
        <label className="field">
          <span>Words to search for</span>
          <input type="text" value={r.text} onChange={(e) => set({ text: e.target.value })} maxLength={500} data-testid="smart-text" />
        </label>
        <fieldset className="choices">
          <legend>Platform (any of)</legend>
          {PLATFORMS.map((p: Platform) => (
            <label key={p} className="check inline">
              <input type="checkbox" checked={r.platforms.includes(p)} onChange={() => set({ platforms: toggle(r.platforms, p) })} />
              {PLATFORM_LABEL[p]}
            </label>
          ))}
        </fieldset>
        <fieldset className="choices">
          <legend>Type (any of)</legend>
          {ITEM_KINDS.map((k: ItemKind) => (
            <label key={k} className="check inline">
              <input type="checkbox" checked={r.kinds.includes(k)} onChange={() => set({ kinds: toggle(r.kinds, k) })} />
              {KIND_LABEL[k]}
            </label>
          ))}
        </fieldset>
        {tags.length > 0 && (
          <fieldset className="choices">
            <legend>Tag (any of)</legend>
            {tags.map((t) => (
              <label key={t.id} className="check inline">
                <input type="checkbox" checked={r.tagIds.includes(t.id)} onChange={() => set({ tagIds: toggle(r.tagIds, t.id) })} data-testid="smart-tag" />#{t.name}
              </label>
            ))}
          </fieldset>
        )}
        {collections.length > 0 && (
          <fieldset className="choices">
            <legend>In collection (any of)</legend>
            {collections.map((c) => (
              <label key={c.id} className="check inline">
                <input type="checkbox" checked={r.collectionIds.includes(c.id)} onChange={() => set({ collectionIds: toggle(r.collectionIds, c.id) })} />
                {c.name}
              </label>
            ))}
          </fieldset>
        )}
        <div className="field-row">
          <label className="field">
            <span>Added in the last … days</span>
            <input
              type="number"
              min={1}
              max={36500}
              value={r.addedWithinDays ?? ""}
              onChange={(e) =>
                set({
                  addedWithinDays: e.target.value === "" ? null : Math.round(Number(e.target.value)),
                })
              }
              aria-invalid={daysBad || undefined}
            />
          </label>
          <label className="field">
            <span>Longer than (min)</span>
            <input type="number" min={0} value={minutes(r.minDurationMs)} onChange={(e) => set({ minDurationMs: fromMinutes(e.target.value) })} aria-invalid={durationBad || undefined} />
          </label>
          <label className="field">
            <span>Shorter than (min)</span>
            <input type="number" min={0} value={minutes(r.maxDurationMs)} onChange={(e) => set({ maxDurationMs: fromMinutes(e.target.value) })} aria-invalid={durationBad || undefined} />
          </label>
        </div>
        <label className="check">
          <input type="checkbox" checked={r.favoriteOnly} onChange={(e) => set({ favoriteOnly: e.target.checked })} data-testid="smart-favorite" />
          Favorites only
        </label>
        <label className="check">
          <input type="checkbox" checked={r.includeArchived} onChange={(e) => set({ includeArchived: e.target.checked })} />
          Include archived items
        </label>
        {!hasCondition(r) && <p className="muted small">Add at least one condition.</p>}
        {durationBad && <p className="field-error small">The shortest length is above the longest.</p>}
        {error && (
          <p className="field-error small" role="alert">
            {error}
          </p>
        )}
        <div className="modal-actions">
          <button type="button" className="button" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className="button button-primary" disabled={!ok || busy} data-testid="smart-save">
            Save
          </button>
        </div>
      </form>
    </Modal>
  );
}
