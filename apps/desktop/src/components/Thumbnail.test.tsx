import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, render } from "@testing-library/react";
import { Thumbnail } from "./ItemCard";
import type { LibraryItemDto } from "../ipc/types";

// A thumbnail that fails to load is tried again (0.5, 1, 2 s) before the
// placeholder stays; one early failure no longer hides it until a remount.
describe("Thumbnail retries", () => {
  const item = { id: "i1", platform: "local", durationMs: 1000, thumbnailUrl: "http://vvthumb.localhost/abc" } as unknown as LibraryItemDto;

  it("tries again after a failed load and keeps the placeholder after the last try", () => {
    vi.useFakeTimers();
    const { container } = render(<Thumbnail item={item} />);
    const img = () => container.querySelector("img");
    expect(img()?.getAttribute("src")).toBe("http://vvthumb.localhost/abc");
    fireEvent.error(img()!);
    expect(img()).toBeNull();
    act(() => vi.advanceTimersByTime(500));
    expect(img()?.getAttribute("src")).toBe("http://vvthumb.localhost/abc?try=1");
    fireEvent.error(img()!);
    act(() => vi.advanceTimersByTime(1000));
    expect(img()?.getAttribute("src")).toBe("http://vvthumb.localhost/abc?try=2");
    fireEvent.error(img()!);
    act(() => vi.advanceTimersByTime(2000));
    expect(img()?.getAttribute("src")).toBe("http://vvthumb.localhost/abc?try=3");
    fireEvent.error(img()!);
    act(() => vi.advanceTimersByTime(10000));
    expect(img()).toBeNull();
    expect(container.querySelector(".thumb")).toHaveAttribute("data-thumb", "missing");
    vi.useRealTimers();
  });
});
