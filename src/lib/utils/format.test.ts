// format.test.ts — Unit tests for the presentation helpers in format.ts.

import { describe, it, expect } from "vitest";
import { formatBytes, formatRate, formatMtime } from "./format";

describe("formatBytes", () => {
  it("handles zero and negatives", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(-5)).toBe("0 B");
  });

  it("formats bytes without decimals", () => {
    expect(formatBytes(512)).toBe("512 B");
  });

  it("formats larger units with one decimal by default", () => {
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(2 * 1024 * 1024 * 1024)).toBe("2.0 GB");
  });
});

describe("formatRate", () => {
  it("appends /s", () => {
    expect(formatRate(1024)).toBe("1.0 KB/s");
  });
});

describe("formatMtime", () => {
  it("returns an em dash for missing input", () => {
    expect(formatMtime(null)).toBe("—");
    expect(formatMtime(undefined)).toBe("—");
  });

  it("formats a real timestamp to a non-empty string", () => {
    const out = formatMtime(1_700_000_000);
    expect(out).not.toBe("—");
    expect(out.length).toBeGreaterThan(0);
  });
});
