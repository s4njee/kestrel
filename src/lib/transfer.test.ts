// transfer.test.ts — Tests for building transfer requests from a selection.

import { describe, it, expect } from "vitest";
import { buildTransferRequests, dropDirection, uploadRequestsForPaths } from "./transfer";
import type { DirEntry } from "$lib/ipc/commands";

function entry(name: string, path: string, kind: DirEntry["kind"], size = 0): DirEntry {
  return { name, path, kind, size, mtime: 0, permissions: null, linkTarget: null };
}

describe("buildTransferRequests", () => {
  it("builds one download request per selected file into the dest dir", () => {
    const entries = [
      entry("a.txt", "/remote/a.txt", "file", 10),
      entry("b.bin", "/remote/b.bin", "file", 20),
    ];
    const reqs = buildTransferRequests("download", "s1", entries, "/local/dir");
    expect(reqs).toEqual([
      {
        sessionId: "s1",
        direction: "download",
        src: "/remote/a.txt",
        dest: "/local/dir/a.txt",
        size: 10,
      },
      {
        sessionId: "s1",
        direction: "download",
        src: "/remote/b.bin",
        dest: "/local/dir/b.bin",
        size: 20,
      },
    ]);
  });

  it("skips directories (recursion is a later story)", () => {
    const entries = [
      entry("dir", "/remote/dir", "dir"),
      entry("f.txt", "/remote/f.txt", "file", 5),
    ];
    const reqs = buildTransferRequests("download", "s1", entries, "/local");
    expect(reqs).toHaveLength(1);
    expect(reqs[0].src).toBe("/remote/f.txt");
  });

  it("builds upload requests into a remote (posix) dir", () => {
    const entries = [entry("photo.jpg", "/Users/me/photo.jpg", "file", 100)];
    const reqs = buildTransferRequests("upload", "s1", entries, "/upload");
    expect(reqs[0]).toEqual({
      sessionId: "s1",
      direction: "upload",
      src: "/Users/me/photo.jpg",
      dest: "/upload/photo.jpg",
      size: 100,
    });
  });
});

describe("dropDirection", () => {
  it("maps cross-pane drags to a direction", () => {
    expect(dropDirection("local", "remote")).toBe("upload");
    expect(dropDirection("remote", "local")).toBe("download");
  });
  it("returns null for same-pane drops", () => {
    expect(dropDirection("local", "local")).toBeNull();
    expect(dropDirection("remote", "remote")).toBeNull();
  });
});

describe("uploadRequestsForPaths", () => {
  it("builds upload requests into a remote dir", () => {
    const reqs = uploadRequestsForPaths("s1", ["/Users/me/a.txt", "/Users/me/b.bin"], "/upload");
    expect(reqs).toEqual([
      {
        sessionId: "s1",
        direction: "upload",
        src: "/Users/me/a.txt",
        dest: "/upload/a.txt",
        size: 0,
      },
      {
        sessionId: "s1",
        direction: "upload",
        src: "/Users/me/b.bin",
        dest: "/upload/b.bin",
        size: 0,
      },
    ]);
  });
});
