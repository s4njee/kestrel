// panes.test.ts — Tests for pane sort ordering and multi-selection.

import { describe, it, expect } from "vitest";
import { PaneStore } from "./panes.svelte";
import type { DirEntry } from "$lib/ipc/commands";

function entry(name: string, kind: DirEntry["kind"], size = 0, mtime = 0): DirEntry {
  return { name, path: `/${name}`, kind, size, mtime, permissions: 0o644, linkTarget: null };
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
  it("single click selects one", () => {
    const p = pane();
    p.select("apple.txt", { ctrl: false, shift: false });
    expect([...p.selected]).toEqual(["apple.txt"]);
  });

  it("ctrl click toggles membership", () => {
    const p = pane();
    p.select("apple.txt", { ctrl: false, shift: false });
    p.select("banana.txt", { ctrl: true, shift: false });
    expect(p.selected.has("apple.txt")).toBe(true);
    expect(p.selected.has("banana.txt")).toBe(true);
    p.select("apple.txt", { ctrl: true, shift: false });
    expect(p.selected.has("apple.txt")).toBe(false);
  });

  it("shift click selects a contiguous range in sort order", () => {
    const p = pane();
    // Sort order: adir, zdir, apple.txt, banana.txt, cherry.txt
    p.select("zdir", { ctrl: false, shift: false });
    p.select("banana.txt", { ctrl: false, shift: true });
    expect([...p.selected].sort()).toEqual(["apple.txt", "banana.txt", "zdir"]);
  });

  it("startLoad clears selection", () => {
    const p = pane();
    p.select("apple.txt", { ctrl: false, shift: false });
    p.startLoad("/other");
    expect(p.selected.size).toBe(0);
  });
});
