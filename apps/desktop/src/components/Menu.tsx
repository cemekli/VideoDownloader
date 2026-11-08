import { useEffect, useId, useRef, useState } from "react";

export interface MenuItem {
  label: string;
  onSelect: () => void;
  danger?: boolean;
  testId?: string;
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
        <div ref={menu} id={id} className="menu" role="menu" aria-label={label} onKeyDown={onKeyDown}>
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
