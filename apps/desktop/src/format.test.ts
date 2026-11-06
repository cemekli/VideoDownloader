import { describe, expect, it } from "vitest";
import { formatDuration } from "./format";

describe("formatDuration", () => {
  it("shows unknown duration as a dash, never 0:00 (SPEC §11.1)", () => {
    expect(formatDuration(null)).toBe("—");
    expect(formatDuration(Number.NaN)).toBe("—");
    expect(formatDuration(-1)).toBe("—");
  });

  it("formats minutes and hours", () => {
    expect(formatDuration(0)).toBe("0:00");
    expect(formatDuration(59_000)).toBe("0:59");
    expect(formatDuration(61_500)).toBe("1:01");
    expect(formatDuration(3_725_000)).toBe("1:02:05");
  });
});
