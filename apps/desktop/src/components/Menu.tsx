import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";

export interface MenuItem {
  label: string;
  onSelect: () => void;
  danger?: boolean;
  testId?: string;
}

const GAP = 4;
const EDGE = 8;

/**
 * Where the menu goes: fixed to the viewport, so a scrolling or narrow
 * parent (the navigation column) cannot clip it. Below the button and
 * toward the right by default; flipped when it would leave the window.
 */
export function placeMenu(button: DOMRect, menu: { width: number; height: number }, view: { width: number; height: number }): { top: number; left: number } {
  let left = button.left;
  if (left + menu.width > view.width - EDGE) left = button.right - menu.width;
  left = Math.max(EDGE, Math.min(left, view.width - EDGE - menu.width));
  let top = button.bottom + GAP;
  if (top + menu.height > view.height - EDGE && button.top - GAP - menu.height >= EDGE) top = button.top - GAP - menu.height;
  return { top: Math.max(EDGE, top), left };
}

/**
 * "⋯" button with a small action menu (ARIA menu button). Arrow keys move,
 * Esc closes and returns focus to the button.
 */
export function MenuButton({ label, items, testId }: { label: string; items: MenuItem[]; testId?: string }) {
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const id = useId();
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);
  useLayoutEffect(() => {
    if (!open) {
      setPos(null);
      return;
    }
    const b = button.current?.getBoundingClientRect();
    const m = menu.current;
    if (!b || !m) return;
    setPos(placeMenu(b, { width: m.offsetWidth, height: m.offsetHeight }, { width: window.innerWidth, height: window.innerHeight }));
  }, [open]);
  useEffect(() => {
    if (!open) return;
    // A fixed menu would drift from its button; close it instead.
    const shut = (e: Event) => {
      if (e.type === "scroll" && menu.current?.contains(e.target as Node)) return;
      setOpen(false);
    };
    window.addEventListener("resize", shut);
    window.addEventListener("scroll", shut, true);
    return () => {
      window.removeEventListener("resize", shut);
      window.removeEventListener("scroll", shut, true);
    };
  }, [open]);
  useEffect(() => {
    if (!open) return;
    menu.current?.querySelector<HTMLElement>("[role=menuitem]")?.focus();
    const away = (e: MouseEvent) => {
      if (!menu.current?.contains(e.target as Node) && e.target !== button.current) setOpen(false);
    };
    document.addEventListener("mousedown", away);
    return () => document.removeEventListener("mousedown", away);
  }, [open]);
  const close = () => {
    setOpen(false);
    button.current?.focus();
  };
  const onKeyDown = (e: React.KeyboardEvent) => {
    const all = [...(menu.current?.querySelectorAll<HTMLElement>("[role=menuitem]") ?? [])];
    const i = all.indexOf(document.activeElement as HTMLElement);
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      all[(i + 1) % all.length]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      all[(i - 1 + all.length) % all.length]?.focus();
    } else if (e.key === "Tab") setOpen(false);
  };
  return (
    <span className="menu-anchor">
      <button
        ref={button}
        type="button"
        className="icon-button menu-button"
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        onClick={() => setOpen((o) => !o)}
        data-testid={testId}
      >
        ⋯
      </button>
      {open && (
        <div
          ref={menu}
          id={id}
          className="menu menu-floating"
          role="menu"
          aria-label={label}
          onKeyDown={onKeyDown}
          style={pos ?? { top: 0, left: 0, opacity: 0 }}
        >
          {items.map((it) => (
            <button
              key={it.label}
              type="button"
              role="menuitem"
              tabIndex={-1}
              className={`menu-item${it.danger ? " danger" : ""}`}
              onClick={() => {
                setOpen(false);
                it.onSelect();
              }}
              data-testid={it.testId}
            >
              {it.label}
            </button>
          ))}
        </div>
      )}
    </span>
  );
}
