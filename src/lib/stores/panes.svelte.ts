// panes.svelte.ts — Per-pane browsing state (Svelte 5 runes).
//
// One instance per pane (local + remote). Holds the current path, the loaded
// entries, loading/error state, the sort order, and the multi-selection. Sort
// and selection logic live here (pure and unit-tested); data loading is
// orchestrated by the shell, which knows whether a pane is local or remote.

import { SvelteSet } from "svelte/reactivity";
import type { DirEntry } from "$lib/ipc/commands";
import type { PaneKind } from "$lib/types";

/** Column a pane can be sorted by. */
export type SortKey = "name" | "size" | "mtime" | "permissions";

/** Modifier keys for a selection click. */
export interface SelectMods {
  ctrl: boolean;
  shift: boolean;
}

/** Compare two entries by a sort key (directories always sort first). */
function compareEntries(a: DirEntry, b: DirEntry, key: SortKey): number {
  const aDir = a.kind === "dir";
  const bDir = b.kind === "dir";
  if (aDir !== bDir) return aDir ? -1 : 1;
  switch (key) {
    case "size":
      return a.size - b.size;
    case "mtime":
      return (a.mtime ?? 0) - (b.mtime ?? 0);
    case "permissions":
      return (a.permissions ?? 0) - (b.permissions ?? 0);
    case "name":
    default:
      return a.name.localeCompare(b.name);
  }
}

/** Reactive state for one file pane. */
export class PaneStore {
  readonly kind: PaneKind;

  #path = $state("");
  #entries = $state<DirEntry[]>([]);
  #loading = $state(false);
  #error = $state<string | null>(null);
  #sortKey = $state<SortKey>("name");
  #sortAsc = $state(true);
  #selected = $state<SvelteSet<string>>(new SvelteSet());
  #anchor = $state<number | null>(null);

  constructor(kind: PaneKind) {
    this.kind = kind;
  }

  get path(): string {
    return this.#path;
  }
  get loading(): boolean {
    return this.#loading;
  }
  get error(): string | null {
    return this.#error;
  }
  get sortKey(): SortKey {
    return this.#sortKey;
  }
  get sortAsc(): boolean {
    return this.#sortAsc;
  }
  get selected(): SvelteSet<string> {
    return this.#selected;
  }

  /** Entries in the current sort order (directories first). */
  get sortedEntries(): DirEntry[] {
    const sorted = [...this.#entries].sort((a, b) => compareEntries(a, b, this.#sortKey));
    if (!this.#sortAsc) sorted.reverse();
    return sorted;
  }

  /** The currently selected entries. */
  get selectedEntries(): DirEntry[] {
    return this.#entries.filter((e) => this.#selected.has(e.name));
  }

  /** Begin loading a new path (clears selection + error, sets loading). */
  startLoad(path: string): void {
    this.#path = path;
    this.#loading = true;
    this.#error = null;
    this.#selected = new SvelteSet();
    this.#anchor = null;
  }

  /** Supply loaded entries and end the loading state. */
  setEntries(entries: DirEntry[]): void {
    this.#entries = entries;
    this.#loading = false;
  }

  /** Record a load failure. */
  setError(message: string): void {
    this.#error = message;
    this.#loading = false;
    this.#entries = [];
  }

  /**
   * Toggle/cycle the sort column. Clicking the active column flips direction.
   *
   * @param key - the column to sort by.
   */
  setSort(key: SortKey): void {
    if (this.#sortKey === key) {
      this.#sortAsc = !this.#sortAsc;
    } else {
      this.#sortKey = key;
      this.#sortAsc = true;
    }
  }

  /**
   * Apply a selection click with modifiers.
   *
   * @param name - the clicked entry's name.
   * @param mods - ctrl/cmd (toggle) and shift (range) modifiers.
   */
  select(name: string, mods: SelectMods): void {
    const order = this.sortedEntries;
    const index = order.findIndex((e) => e.name === name);
    if (index < 0) return;

    if (mods.shift && this.#anchor !== null) {
      const [lo, hi] = [this.#anchor, index].sort((a, b) => a - b);
      const range = new SvelteSet(order.slice(lo, hi + 1).map((e) => e.name));
      this.#selected = range;
    } else if (mods.ctrl) {
      const next = new SvelteSet(this.#selected);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      this.#selected = next;
      this.#anchor = index;
    } else {
      this.#selected = new SvelteSet([name]);
      this.#anchor = index;
    }
  }

  /** Clear the current selection. */
  clearSelection(): void {
    this.#selected = new SvelteSet();
    this.#anchor = null;
  }

  /** Reset the pane to an empty, pathless state (e.g. on disconnect). */
  reset(): void {
    this.#path = "";
    this.#entries = [];
    this.#loading = false;
    this.#error = null;
    this.#selected = new SvelteSet();
    this.#anchor = null;
  }
}

/** The two application panes. */
export const localPane = new PaneStore("local");
export const remotePane = new PaneStore("remote");
