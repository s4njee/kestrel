// path.test.ts — Tests for the separator-tolerant path helpers.

import { describe, it, expect } from "vitest";
import { parentPath, pathSegments } from "./path";

describe("parentPath", () => {
  it("returns the parent of a posix path", () => {
    expect(parentPath("/a/b/c")).toBe("/a/b");
    expect(parentPath("/a")).toBe("/");
    expect(parentPath("/")).toBe("/");
  });

  it("handles trailing slashes", () => {
    expect(parentPath("/a/b/")).toBe("/a");
  });
});

describe("pathSegments", () => {
  it("splits a posix path into cumulative segments", () => {
    const segs = pathSegments("/home/user/docs");
    expect(segs.map((s) => s.label)).toEqual(["/", "home", "user", "docs"]);
    expect(segs.map((s) => s.path)).toEqual(["/", "/home", "/home/user", "/home/user/docs"]);
  });

  it("handles the root path", () => {
    expect(pathSegments("/").map((s) => s.label)).toEqual(["/"]);
  });
});
