// diff.test.ts — Tests for the pane diff model (E8-S6).

import { describe, it, expect } from "vitest";
import {
  DIFF_GLYPHS,
  MTIME_TOLERANCE_SECONDS,
  diffPanes,
  differingEntries,
  relativePaths,
  type DiffMark,
} from "./diff";
import type { PaneRow } from "$lib/stores/panes.svelte";
import type { DirEntry } from "$lib/ipc/commands";

/**
 * Build a row for a file.
 *
 * @param path - the absolute path (its basename becomes the entry name unless
 *   `name` is given).
 * @param over - overrides for the entry fields and the row's depth.
 * @returns a PaneRow suitable for the diff functions.
 */
function row(
  path: string,
  over: Partial<DirEntry> & { depth?: number } = {},
): PaneRow & { entry: DirEntry } {
  const { depth = 0, ...entry } = over;
  const sep = path.includes("\\") ? "\\" : "/";
  return {
    entry: {
      name: path.split(sep).pop()!,
      path,
      kind: "file",
      size: 10,
      mtime: 1000,
      permissions: 0o644,
      linkTarget: null,
      ...entry,
    } as DirEntry,
    depth,
    expanded: false,
    loading: false,
  };
}

/**
 * Build a directory row.
 *
 * @param path - the absolute path.
 * @param over - overrides (typically `depth` and `expanded`).
 * @returns a PaneRow whose entry is a directory.
 */
function dir(path: string, over: Partial<DirEntry> & { depth?: number } = {}): PaneRow {
  return row(path, { kind: "dir", size: 0, ...over });
}

describe("relativePaths", () => {
  it("rebuilds nested paths from depth tags, not from the absolute path", () => {
    // A Windows-style local tree: reconstruction must not depend on separators.
    const rows = [
      dir("C:\\src\\app"),
      row("C:\\src\\app\\main.rs", { depth: 1 }),
      dir("C:\\src\\app\\util", { depth: 1 }),
      row("C:\\src\\app\\util\\fmt.rs", { depth: 2 }),
      row("C:\\src\\README", {}),
    ];
    const rel = relativePaths(rows);
    expect(rel.get("C:\\src\\app")).toBe("app");
    expect(rel.get("C:\\src\\app\\main.rs")).toBe("app/main.rs");
    expect(rel.get("C:\\src\\app\\util\\fmt.rs")).toBe("app/util/fmt.rs");
    expect(rel.get("C:\\src\\README")).toBe("README");
  });

  it("does not leak a deeper branch's ancestry into a later shallow row", () => {
    const rows = [
      dir("/a"),
      dir("/a/b", { depth: 1 }),
      row("/a/b/c", { depth: 2 }),
      row("/z", {}), // back to depth 0 after a depth-2 row
    ];
    expect(relativePaths(rows).get("/z")).toBe("z");
  });
});

describe("diffPanes", () => {
  /**
   * Diff two flat lists and read back one side's marks by relative name.
   *
   * @param local - local rows.
   * @param remote - remote rows.
   * @returns the marks keyed by absolute path for both panes.
   */
  const marksOf = (local: PaneRow[], remote: PaneRow[]) => diffPanes(local, remote);

  it("pairs entries across panes with completely different roots", () => {
    const { local, remote } = marksOf([row("/home/me/a.txt")], [row("/srv/deploy/a.txt")]);
    expect(local.get("/home/me/a.txt")).toBe<DiffMark>("same");
    expect(remote.get("/srv/deploy/a.txt")).toBe<DiffMark>("same");
  });

  it("marks a size mismatch as differs on both sides", () => {
    const { local, remote } = marksOf(
      [row("/l/a.txt", { size: 10 })],
      [row("/r/a.txt", { size: 11 })],
    );
    expect(local.get("/l/a.txt")).toBe<DiffMark>("differs");
    expect(remote.get("/r/a.txt")).toBe<DiffMark>("differs");
  });

  it("separates a timestamp-only difference from a real one", () => {
    // Same bytes, later clock — what a plain copy looks like, since transfers
    // do not preserve mtime. It must not be reported as `differs`.
    const { local } = marksOf(
      [row("/l/a.txt", { size: 10, mtime: 1000 })],
      [row("/r/a.txt", { size: 10, mtime: 9000 })],
    );
    expect(local.get("/l/a.txt")).toBe<DiffMark>("timestamp");
  });

  it("tolerates sub-second-granularity clock skew", () => {
    const within = marksOf(
      [row("/l/a.txt", { mtime: 1000 })],
      [row("/r/a.txt", { mtime: 1000 + MTIME_TOLERANCE_SECONDS })],
    );
    expect(within.local.get("/l/a.txt")).toBe<DiffMark>("same");

    const beyond = marksOf(
      [row("/l/a.txt", { mtime: 1000 })],
      [row("/r/a.txt", { mtime: 1000 + MTIME_TOLERANCE_SECONDS + 1 })],
    );
    expect(beyond.local.get("/l/a.txt")).toBe<DiffMark>("timestamp");
  });

  it("treats an unknown mtime on either side as no timestamp evidence", () => {
    const { local } = marksOf([row("/l/a.txt", { mtime: null })], [row("/r/a.txt", { mtime: 5 })]);
    expect(local.get("/l/a.txt")).toBe<DiffMark>("same");
  });

  it("marks unmatched entries `only` on the side that has them", () => {
    const { local, remote } = marksOf([row("/l/a.txt")], [row("/r/b.txt")]);
    expect(local.get("/l/a.txt")).toBe<DiffMark>("only");
    expect(remote.get("/r/b.txt")).toBe<DiffMark>("only");
  });

  it("compares directories by existence, ignoring their size and mtime", () => {
    const { local } = marksOf(
      [dir("/l/pkg", { size: 4096, mtime: 1 })],
      [dir("/r/pkg", { size: 0, mtime: 999999 })],
    );
    expect(local.get("/l/pkg")).toBe<DiffMark>("same");
  });

  it("flags a name that is a file on one side and a directory on the other", () => {
    const { local } = marksOf([row("/l/thing")], [dir("/r/thing")]);
    expect(local.get("/l/thing")).toBe<DiffMark>("differs");
  });

  it("compares nested expanded trees at matching relative depths", () => {
    const local = [
      dir("/l/app"),
      row("/l/app/main.rs", { depth: 1, size: 10 }),
      row("/l/app/only-local.rs", { depth: 1 }),
    ];
    const remote = [
      dir("/r/app"),
      row("/r/app/main.rs", { depth: 1, size: 99 }),
      row("/r/app/only-remote.rs", { depth: 1 }),
    ];
    const d = diffPanes(local, remote);
    expect(d.local.get("/l/app")).toBe<DiffMark>("same");
    expect(d.local.get("/l/app/main.rs")).toBe<DiffMark>("differs");
    expect(d.local.get("/l/app/only-local.rs")).toBe<DiffMark>("only");
    expect(d.remote.get("/r/app/only-remote.rs")).toBe<DiffMark>("only");
  });

  it("does not match same-named files sitting at different depths", () => {
    // `a.txt` at the root vs `app/a.txt` nested — different relative paths.
    const d = diffPanes([row("/l/a.txt")], [dir("/r/app"), row("/r/app/a.txt", { depth: 1 })]);
    expect(d.local.get("/l/a.txt")).toBe<DiffMark>("only");
    expect(d.remote.get("/r/app/a.txt")).toBe<DiffMark>("only");
  });

  it("marks every input row on both sides", () => {
    const local = [row("/l/a"), dir("/l/d"), row("/l/d/x", { depth: 1 })];
    const remote = [row("/r/a")];
    const d = diffPanes(local, remote);
    expect([...d.local.keys()]).toEqual(["/l/a", "/l/d", "/l/d/x"]);
    expect([...d.remote.keys()]).toEqual(["/r/a"]);
  });

  it("handles an empty pane without pairing anything", () => {
    const d = diffPanes([row("/l/a")], []);
    expect(d.local.get("/l/a")).toBe<DiffMark>("only");
    expect(d.remote.size).toBe(0);
  });
});

describe("differingEntries", () => {
  const local = [
    row("/l/new.txt"), //           only     → transfer
    row("/l/changed.txt", { size: 1 }), //   differs  → transfer
    row("/l/touched.txt"), //       timestamp→ skip
    row("/l/identical.txt"), //     same     → skip
    dir("/l/pkg"),
    row("/l/pkg/nested-new.txt", { depth: 1 }), // only, but nested → skip
  ];
  const remote = [
    row("/r/changed.txt", { size: 2 }),
    row("/r/touched.txt", { mtime: 99999 }),
    row("/r/identical.txt"),
    dir("/r/pkg"),
  ];

  it("selects only-here and size-differing top-level entries", () => {
    const { local: marks } = diffPanes(local, remote);
    expect(differingEntries(local, marks).map((e) => e.name)).toEqual(["new.txt", "changed.txt"]);
  });

  it("excludes timestamp-only rows so completed copies never re-transfer", () => {
    const { local: marks } = diffPanes(local, remote);
    const names = differingEntries(local, marks).map((e) => e.name);
    expect(names).not.toContain("touched.txt");
  });

  it("excludes nested rows, whose destination directory may not exist yet", () => {
    const { local: marks } = diffPanes(local, remote);
    expect(marks.get("/l/pkg/nested-new.txt")).toBe<DiffMark>("only");
    expect(differingEntries(local, marks).map((e) => e.name)).not.toContain("nested-new.txt");
  });

  it("returns nothing when the trees agree", () => {
    const same = [row("/l/a.txt")];
    const { local: marks } = diffPanes(same, [row("/r/a.txt")]);
    expect(differingEntries(same, marks)).toEqual([]);
  });
});

describe("DIFF_GLYPHS", () => {
  it("gives every mark a distinct single-column glyph", () => {
    const glyphs = Object.values(DIFF_GLYPHS);
    expect(new Set(glyphs).size).toBe(glyphs.length);
    for (const g of glyphs) expect(g).toHaveLength(1);
  });
});
