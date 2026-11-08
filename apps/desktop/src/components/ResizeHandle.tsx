import { useRef } from "react";
import { NAV_WIDTH, clampNavWidth } from "../hooks/usePrefs";

const STEP = 16;

/**
 * Draggable edge between the navigation and the results. Pointer drag,
 * arrow keys (Shift for bigger steps), Home/End, and double click to reset.
 */
export function ResizeHandle({ width, onWidth }: { width: number; onWidth: (w: number) => void }) {
  const start = useRef<{ x: number; w: number } | null>(null);
  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    start.current = { x: e.clientX, w: width };
    document.body.classList.add("resizing");
  };
  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!start.current) return;
    onWidth(clampNavWidth(start.current.w + e.clientX - start.current.x));
  };
  const end = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!start.current) return;
    start.current = null;
    document.body.classList.remove("resizing");
    if (e.currentTarget.hasPointerCapture(e.pointerId)) e.currentTarget.releasePointerCapture(e.pointerId);
  };
  const onKeyDown = (e: React.KeyboardEvent) => {
    const step = e.shiftKey ? STEP * 4 : STEP;
    const next = e.key === "ArrowLeft" ? width - step : e.key === "ArrowRight" ? width + step : e.key === "Home" ? NAV_WIDTH.min : e.key === "End" ? NAV_WIDTH.max : null;
    if (next === null) return;
    e.preventDefault();
    onWidth(clampNavWidth(next));
  };
  return (
    <div
      className="resize-handle"
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize navigation"
      aria-valuemin={NAV_WIDTH.min}
      aria-valuemax={NAV_WIDTH.max}
      aria-valuenow={width}
      tabIndex={0}
      title="Drag to resize. Double-click to reset."
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={end}
      onPointerCancel={end}
      onDoubleClick={() => onWidth(NAV_WIDTH.initial)}
      onKeyDown={onKeyDown}
      data-testid="nav-resize"
    />
  );
}
