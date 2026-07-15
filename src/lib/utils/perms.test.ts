// perms.test.ts — Tests for the permission-bit helpers.

import { describe, it, expect } from "vitest";
import { modeToBits, bitsToMode, formatOctal, parseOctal, formatRwx } from "./perms";

describe("perms", () => {
  it("round-trips mode ↔ bits", () => {
    for (const mode of [0o644, 0o755, 0o600, 0o777, 0o000]) {
      expect(bitsToMode(modeToBits(mode))).toBe(mode);
    }
  });

  it("maps 0o644 to the expected bits", () => {
    // owner rw-, group r--, other r--
    expect(modeToBits(0o644)).toEqual([true, true, false, true, false, false, true, false, false]);
  });

  it("formats octal", () => {
    expect(formatOctal(0o644)).toBe("644");
    expect(formatOctal(0o7)).toBe("007");
  });

  it("parses octal, rejecting garbage", () => {
    expect(parseOctal("644")).toBe(0o644);
    expect(parseOctal("755")).toBe(0o755);
    expect(parseOctal("9")).toBeNull();
    expect(parseOctal("")).toBeNull();
    expect(parseOctal("abc")).toBeNull();
  });

  it("formats rwx", () => {
    expect(formatRwx(0o644)).toBe("rw-r--r--");
    expect(formatRwx(0o755)).toBe("rwxr-xr-x");
  });
});
