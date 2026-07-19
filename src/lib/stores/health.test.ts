// health.test.ts — Tests for the connection-health store and its pure helpers.

import { describe, it, expect } from "vitest";
import { health, latencyLevel, sparkline } from "./health.svelte";

describe("latencyLevel", () => {
  it("bands round trips into good/warn/bad", () => {
    expect(latencyLevel(0)).toBe("good");
    expect(latencyLevel(79)).toBe("good");
    expect(latencyLevel(80)).toBe("warn");
    expect(latencyLevel(249)).toBe("warn");
    expect(latencyLevel(250)).toBe("bad");
    expect(latencyLevel(4000)).toBe("bad");
  });
});

describe("sparkline", () => {
  it("is empty for no samples", () => {
    expect(sparkline([])).toBe("");
  });

  it("renders one glyph per sample, scaled to the window max", () => {
    const line = sparkline([10, 40, 80]);
    expect([...line]).toHaveLength(3);
    // The max sample maps to the tallest glyph.
    expect(line.endsWith("█")).toBe(true);
  });

  it("draws a flat floor for all-zero samples instead of dividing by zero", () => {
    expect(sparkline([0, 0, 0])).toBe("▁▁▁");
  });

  it("shows a spike as the tall glyph among short ones", () => {
    const line = [...sparkline([5, 5, 500, 5])];
    expect(line[2]).toBe("█");
    expect(line[0]).toBe("▁");
  });
});

describe("health store", () => {
  it("records per-session rings, keeps the last 12, and forgets on demand", () => {
    for (let i = 1; i <= 15; i++) health.record("s1", i);
    health.record("s2", 99);

    expect(health.samples("s1")).toHaveLength(12);
    // Oldest three (1..3) fell out of the ring.
    expect(health.samples("s1")[0]).toBe(4);
    expect(health.latest("s1")).toBe(15);
    // Rings are independent.
    expect(health.samples("s2")).toEqual([99]);

    health.forget("s1");
    expect(health.samples("s1")).toEqual([]);
    expect(health.latest("s1")).toBeNull();
    expect(health.latest("s2")).toBe(99);
    health.forget("s2");
  });
});
