// panes.test.ts — Tests for pane sort ordering, the expandable folder tree, and
// path-keyed multi-selection.

import { describe, it, expect } from "vitest";
import { PaneStore, forgetRemotePane, remotePaneCount, remotePaneFor } from "./panes.svelte";
import type { DirEntry } from "$lib/ipc/commands";

function entry(name: string, kind: DirEntry["kind"], size = 0, mtime = 0, path?: string): DirEntry {
  return {
    name,
    path: path ?? `/${name}`,
    kind,
    size,
    mtime,
    permissions: 0o644,
    linkTarget: null,
  };
}

function pane(): PaneStore {
  const p = new PaneStore("remote");
  p.startLoad("/");
  p.setEntries([
    entry("banana.txt", "file", 300, 30),
    entry("apple.txt", "file", 100, 10),
    entry("cherry.txt", "file", 200, 20),
    entry("zdir", "dir"),
    entry("adir", "dir"),
  ]);
  return p;
}

describe("PaneStore sorting", () => {
  it("sorts directories first, then by name ascending", () => {
    const names = pane().sortedEntries.map((e) => e.name);
    expect(names).toEqual(["adir", "zdir", "apple.txt", "banana.txt", "cherry.txt"]);
  });

  it("flips direction when the active column is re-selected", () => {
    const p = pane();
    p.setSort("name"); // already name asc → becomes desc
    const names = p.sortedEntries.map((e) => e.name);
    // Reversed overall order (dirs no longer first because reverse flips all).
    expect(names[0]).toBe("cherry.txt");
  });

  it("sorts by size within files (dirs still first)", () => {
    const p = pane();
    p.setSort("size");
    const files = p.sortedEntries.filter((e) => e.kind === "file").map((e) => e.name);
    expect(files).toEqual(["apple.txt", "cherry.txt", "banana.txt"]);
  });
});

describe("PaneStore selection", () => {
  it("single click selects one (keyed by path)", () => {
    const p = pane();
    p.select("/apple.txt", { ctrl: false, shift: false });
    expect([...p.selected]).toEqual(["/apple.txt"]);
    expect(p.selectedEntries.map((e) => e.name)).toEqual(["apple.txt"]);
  });

  it("ctrl click toggles membership", () => {
    const p = pane();
    p.select("/apple.txt", { ctrl: false, shift: false });
    p.select("/banana.txt", { ctrl: true, shift: false });
    expect(p.selected.has("/apple.txt")).toBe(true);
    expect(p.selected.has("/banana.txt")).toBe(true);
    p.select("/apple.txt", { ctrl: true, shift: false });
    expect(p.selected.has("/apple.txt")).toBe(false);
  });

  it("shift click selects a contiguous range in sort order", () => {
    const p = pane();
    // Sort order: adir, zdir, apple.txt, banana.txt, cherry.txt
    p.select("/zdir", { ctrl: false, shift: false });
    p.select("/banana.txt", { ctrl: false, shift: true });
    expect([...p.selected].sort()).toEqual(["/apple.txt", "/banana.txt", "/zdir"]);
  });

  it("startLoad clears selection", () => {
    const p = pane();
    p.select("/apple.txt", { ctrl: false, shift: false });
    p.startLoad("/other");
    expect(p.selected.size).toBe(0);
  });

  it("distinguishes same-named entries at different depths", () => {
    const p = pane();
    // Both the root and adir/ contain a "readme.md".
    p.setEntries([entry("adir", "dir"), entry("readme.md", "file")]);
    p.setChildren("/adir", [entry("readme.md", "file", 0, 0, "/adir/readme.md")]);
    p.expand("/adir");

    p.select("/adir/readme.md", { ctrl: false, shift: false });
    // Only the child is selected — the root's same-named file is untouched.
    expect(p.selectedEntries.map((e) => e.path)).toEqual(["/adir/readme.md"]);
  });
});

describe("PaneStore folder tree", () => {
  it("lists only the root level until a folder is expanded", () => {
    const p = pane();
    expect(p.rows.every((r) => r.depth === 0)).toBe(true);
    expect(p.rows).toHaveLength(5);
  });

  it("flattens an expanded folder's children beneath it, indented", () => {
    const p = pane();
    p.setChildren("/adir", [
      entry("nested.txt", "file", 5, 0, "/adir/nested.txt"),
      entry("sub", "dir", 0, 0, "/adir/sub"),
    ]);
    p.expand("/adir");

    const shape = p.rows.map((r) => [r.entry.name, r.depth]);
    // adir opens with its children (dirs first) directly under it; zdir follows.
    expect(shape.slice(0, 4)).toEqual([
      ["adir", 0],
      ["sub", 1],
      ["nested.txt", 1],
      ["zdir", 0],
    ]);
    expect(p.rows[0].expanded).toBe(true);
    expect(p.isExpanded("/adir")).toBe(true);
  });

  it("nests deeper levels", () => {
    const p = pane();
    p.setChildren("/adir", [entry("sub", "dir", 0, 0, "/adir/sub")]);
    p.setChildren("/adir/sub", [entry("deep.txt", "file", 1, 0, "/adir/sub/deep.txt")]);
    p.expand("/adir");
    p.expand("/adir/sub");
    expect(p.rows.map((r) => [r.entry.name, r.depth]).slice(0, 3)).toEqual([
      ["adir", 0],
      ["sub", 1],
      ["deep.txt", 2],
    ]);
  });

  it("collapse hides children but keeps them cached", () => {
    const p = pane();
    p.setChildren("/adir", [entry("nested.txt", "file", 5, 0, "/adir/nested.txt")]);
    p.expand("/adir");
    expect(p.rows).toHaveLength(6);

    p.collapse("/adir");
    expect(p.rows).toHaveLength(5);
    expect(p.isExpanded("/adir")).toBe(false);
    // Cached, so re-expanding needs no refetch.
    expect(p.hasChildren("/adir")).toBe(true);
  });

  it("children follow the active sort", () => {
    const p = pane();
    p.setChildren("/adir", [
      entry("b.txt", "file", 1, 0, "/adir/b.txt"),
      entry("a.txt", "file", 9, 0, "/adir/a.txt"),
    ]);
    p.expand("/adir");
    const kids = () => p.rows.filter((r) => r.depth === 1).map((r) => r.entry.name);
    expect(kids()).toEqual(["a.txt", "b.txt"]);
    p.setSort("size");
    expect(kids()).toEqual(["b.txt", "a.txt"]);
  });

  it("navigating collapses the tree and drops cached children", () => {
    const p = pane();
    p.setChildren("/adir", [entry("nested.txt", "file", 5, 0, "/adir/nested.txt")]);
    p.expand("/adir");
    p.startLoad("/elsewhere");
    expect(p.isExpanded("/adir")).toBe(false);
    expect(p.hasChildren("/adir")).toBe(false);
  });

  it("tracks per-folder loading state", () => {
    const p = pane();
    p.setChildLoading("/adir", true);
    expect(p.rows.find((r) => r.entry.name === "adir")?.loading).toBe(true);
    p.setChildLoading("/adir", false);
    expect(p.rows.find((r) => r.entry.name === "adir")?.loading).toBe(false);
  });

  it("expanding a folder with no cached children shows nothing extra", () => {
    const p = pane();
    p.expand("/adir");
    expect(p.rows).toHaveLength(5);
  });
});

describe("PaneStore row filter", () => {
  it("is closed by default and narrows rows case-insensitively once set", () => {
    const p = pane();
    expect(p.filter).toBeNull();
    expect(p.rows).toHaveLength(5);

    p.setFilter("AN"); // matches banana.txt
    expect(p.rows.map((r) => r.entry.name)).toEqual(["banana.txt"]);
  });

  it("an empty query opens the bar without narrowing", () => {
    const p = pane();
    p.setFilter("");
    expect(p.filter).toBe("");
    expect(p.rows).toHaveLength(5);
  });

  it("composes with sorting", () => {
    const p = pane();
    p.setFilter(".txt");
    p.setSort("size");
    expect(p.rows.map((r) => r.entry.name)).toEqual(["apple.txt", "cherry.txt", "banana.txt"]);
  });

  it("keeps an expanded directory visible so its matching children stay reachable", () => {
    const p = pane();
    p.setChildren("/adir", [
      entry("target.log", "file", 1, 0, "/adir/target.log"),
      entry("other.txt", "file", 1, 0, "/adir/other.txt"),
    ]);
    p.expand("/adir");
    p.setFilter("target");

    const shape = p.rows.map((r) => [r.entry.name, r.depth]);
    // The parent survives (it is expanded) and only its matching child shows.
    expect(shape).toEqual([
      ["adir", 0],
      ["target.log", 1],
    ]);
  });

  it("hides a collapsed non-matching directory", () => {
    const p = pane();
    p.setFilter("banana");
    expect(p.rows.map((r) => r.entry.name)).toEqual(["banana.txt"]);
  });

  it("clearing restores every row", () => {
    const p = pane();
    p.setFilter("banana");
    p.setFilter(null);
    expect(p.filter).toBeNull();
    expect(p.rows).toHaveLength(5);
  });

  it("navigating clears the filter", () => {
    const p = pane();
    p.setFilter("banana");
    p.startLoad("/elsewhere");
    expect(p.filter).toBeNull();
  });

  it("selectedEntries only sees filtered-in rows", () => {
    const p = pane();
    p.select("/apple.txt", { ctrl: false, shift: false });
    expect(p.selectedEntries.map((e) => e.name)).toEqual(["apple.txt"]);
    // apple.txt is filtered out, so it is no longer among the visible selection.
    p.setFilter("banana");
    expect(p.selectedEntries).toEqual([]);
  });
});

describe("per-session remote panes (E8-S9)", () => {
  it("gives each session its own independent pane state", () => {
    const a = remotePaneFor("s1");
    const b = remotePaneFor("s2");
    expect(a).not.toBe(b);

    a.startLoad("/srv/a");
    a.setEntries([entry("one.txt", "file")]);
    a.select("/one.txt", { ctrl: false, shift: false });
    b.startLoad("/srv/b");
    b.setEntries([entry("two.txt", "file")]);

    // Switching tabs must restore what that host looked like, not leak across.
    expect(a.path).toBe("/srv/a");
    expect(b.path).toBe("/srv/b");
    expect(a.sortedEntries.map((e) => e.name)).toEqual(["one.txt"]);
    expect(b.sortedEntries.map((e) => e.name)).toEqual(["two.txt"]);
    expect([...a.selected]).toEqual(["/one.txt"]);
    expect(b.selected.size).toBe(0);
  });

  it("returns the same pane for the same session, so state survives a switch", () => {
    const first = remotePaneFor("s3");
    first.startLoad("/kept");
    expect(remotePaneFor("s3")).toBe(first);
    expect(remotePaneFor("s3").path).toBe("/kept");
  });

  it("hands out a parking pane when nothing is connected", () => {
    const detached = remotePaneFor(null);
    expect(detached.kind).toBe("remote");
    expect(remotePaneFor(null)).toBe(detached);
  });

  it("frees a closed session's pane, and a later session of that id starts clean", () => {
    remotePaneFor("s4").startLoad("/old");
    const before = remotePaneCount();
    forgetRemotePane("s4");
    expect(remotePaneCount()).toBe(before - 1);
    expect(remotePaneFor("s4").path).toBe("");
  });

  it("closing one session leaves the others untouched", () => {
    const keep = remotePaneFor("s5");
    keep.startLoad("/still/here");
    remotePaneFor("s6").startLoad("/going");
    forgetRemotePane("s6");
    expect(keep.path).toBe("/still/here");
  });
});
