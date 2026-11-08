import type { ItemKind, Platform } from "../ipc/types";
import { ITEM_KINDS } from "../ipc/types";
import { KIND_LABEL, PLATFORM_LABEL } from "../labels";

/** Filter chips (SEA-02 UI). Each chip is a toggle button with its state in `aria-pressed`. */
export function FilterBar({
  kinds,
  platforms,
  onToggleKind,
  onRemovePlatform,
  onClear,
}: {
  kinds: ItemKind[];
  platforms: Platform[];
  onToggleKind: (k: ItemKind) => void;
  onRemovePlatform: (p: Platform) => void;
  onClear: () => void;
}) {
  const active = kinds.length + platforms.length > 0;
  return (
    <div className="filter-bar" role="group" aria-label="Filters" data-testid="filter-bar">
      {ITEM_KINDS.map((k) => (
        <button key={k} type="button" className="chip" aria-pressed={kinds.includes(k)} onClick={() => onToggleKind(k)} data-testid={`chip-kind-${k}`}>
          {KIND_LABEL[k]}
        </button>
      ))}
      {platforms.map((p) => (
        <button key={p} type="button" className="chip chip-removable" aria-pressed="true" aria-label={`Platform: ${PLATFORM_LABEL[p]}, remove filter`} onClick={() => onRemovePlatform(p)} data-testid={`chip-platform-${p}`}>
          {PLATFORM_LABEL[p]} <span aria-hidden="true">×</span>
        </button>
      ))}
      {active && (
        <button type="button" className="link-button" onClick={onClear} data-testid="clear-filters">
          Clear filters
        </button>
      )}
    </div>
  );
}
