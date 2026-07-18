// panes.test.ts — Tests for pane sort ordering, the expandable folder tree, and
// path-keyed multi-selection.

import { describe, it, expect } from "vitest";
import { PaneStore } from "./panes.svelte";
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
