import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { ERROR_KEYS, errorMessage } from "./labels";

// Every backend error code (crates/vv-domain/src/error.rs, SPEC §19) has its
// own sentence, so the Downloads screen never falls back to "({code})".
describe("error messages", () => {
  const src = readFileSync(resolve(__dirname, "../../../crates/vv-domain/src/error.rs"), "utf8");
  const list = src.slice(src.indexOf("pub const ALL_ERROR_CODES"), src.indexOf("];", src.indexOf("pub const ALL_ERROR_CODES")));
  const codes = [...list.matchAll(/ErrorCode::(\w+)/g)].map((m) => m[1]!.replace(/(?<!^)([A-Z])/g, "_$1").toUpperCase());

  it("covers every backend code", () => {
    expect(codes.length).toBeGreaterThan(30);
    expect(codes.filter((c) => !ERROR_KEYS.has(c))).toEqual([]);
  });

  it("falls back for an unknown code", () => {
    expect(errorMessage("OFFLINE_MODE")).toMatch(/Offline Mode/);
    expect(errorMessage("NOT_A_CODE")).toBe("That did not work (NOT_A_CODE).");
  });
});
