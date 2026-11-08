import { useEffect, useId, useRef } from "react";

/**
 * A modal dialog: Esc closes, Tab stays inside, focus goes to the first
 * field (or button) on open and back to the opener on close (§9.4).
 */
export function Modal({ title, onClose, children, testId, wide }: { title: string; onClose: () => void; children: React.ReactNode; testId: string; wide?: boolean }) {
  const box = useRef<HTMLDivElement>(null);
  const titleId = useId();
  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null;
    const first =
      box.current?.querySelector<HTMLElement>("[data-autofocus], input:not([type=checkbox]):not([type=radio]), select, textarea") ??
      box.current?.querySelector<HTMLElement>("input, button:not(:disabled)");
    first?.focus();
    if (first instanceof HTMLInputElement && first.type === "text") first.select();
    return () => {
      if (opener?.isConnected && opener !== document.body) opener.focus();
    };
  }, []);
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onClose();
      return;
    }
    if (e.key !== "Tab") return;
    const f = [...(box.current?.querySelectorAll<HTMLElement>("input, select, textarea, button:not(:disabled)") ?? [])];
    const i = f.indexOf(document.activeElement as HTMLElement);
    if (e.shiftKey && i <= 0) {
      e.preventDefault();
      f[f.length - 1]?.focus();
    } else if (!e.shiftKey && i === f.length - 1) {
      e.preventDefault();
      f[0]?.focus();
    }
  };
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div ref={box} className={`modal${wide ? " modal-wide" : ""}`} role="dialog" aria-modal="true" aria-labelledby={titleId} onKeyDown={onKeyDown} data-testid={testId}>
        <h2 id={titleId}>{title}</h2>
        {children}
      </div>
    </div>
  );
}
