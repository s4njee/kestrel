// panes.svelte.ts — Per-pane browsing state (Svelte 5 runes).
//
// One instance per pane (local + remote). Holds the current path, the loaded
// entries, loading/error state, the sort order, the expanded-folder tree, and
// the multi-selection. Sort, flatten, and selection logic live here (pure and
// unit-tested); data loading is orchestrated by the shell, which knows whether a
// pane is local or remote and fetches a folder's children when it is expanded.
//
// Directories expand in place: `rows` flattens the root listing plus any
// expanded folders' children into a single depth-tagged list, which is what the
// virtualized table renders. Selection is keyed by **path**, not name, because a
// flattened tree can show two entries with the same name at different depths.

import { SvelteMap, SvelteSet } from "svelte/reactivity";
import type { DirEntry } from "$lib/ipc/commands";
import type { PaneKind } from "$lib/types";
import { settings } from "./settings.svelte";

/** Column a pane can be sorted by. */
export type SortKey = "name" | "size" | "mtime" | "permissions";

/** Modifier keys for a selection click. */
export interface SelectMods {
  ctrl: boolean;
  shift: boolean;
}

/** One visible line of the flattened tree. */
export interface PaneRow {
  /** The entry this row shows. */
  entry: DirEntry;
  /** Nesting level; 0 for the root listing. */
  depth: number;
  /** For directories: whether their children are currently shown. */
  expanded: boolean;
  /** For directories: whether their children are being fetched right now. */
  loading: boolean;
}

/**
 * Compare two entries by a sort key (directories always sort first).
 *
 * @param a - the first entry.
 * @param b - the second entry.
 * @param key - the column to compare by; unknown keys fall back to name.
 * @returns a negative, zero, or positive number ordering `a` relative to `b`.
 */
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
  /** Selected entry paths (not names — a tree can repeat names across depths). */
  #selected = $state<SvelteSet<string>>(new SvelteSet());
  #anchor = $state<number | null>(null);
  /** Loaded children per directory path (populated on first expand). */
  #children = $state<SvelteMap<string, DirEntry[]>>(new SvelteMap());
  /** Directory paths whose children are currently shown. */
  #expanded = $state<SvelteSet<string>>(new SvelteSet());
  /** Directory paths whose children are being fetched. */
  #childLoading = $state<SvelteSet<string>>(new SvelteSet());
  /** Live row filter: null = bar closed, "" = open but empty (E8-S13). */
  #filter = $state<string | null>(null);

  constructor(kind: PaneKind) {
    this.kind = kind;
  }

  /** @returns the pane's current directory path; "" before the first load and after a reset. */
  get path(): string {
    return this.#path;
  }
  /** @returns true while a root listing is being fetched. */
  get loading(): boolean {
    return this.#loading;
  }
  /** @returns the last load failure message, or null when the pane loaded cleanly. */
  get error(): string | null {
    return this.#error;
  }
  /** @returns the active sort column; "name" until {@link setSort} changes it. */
  get sortKey(): SortKey {
    return this.#sortKey;
  }
  /** @returns true when the sort is ascending, which is the initial direction. */
  get sortAsc(): boolean {
    return this.#sortAsc;
  }
  /**
   * The selected entry paths (not names — a flattened tree can repeat names).
   *
   * @returns the live selection set, empty when nothing is selected. Selection
   *   changes replace the set wholesale, so callers must not cache the reference.
   */
  get selected(): SvelteSet<string> {
    return this.#selected;
  }

  /**
   * The live filter query, or null while the filter bar is closed.
   *
   * @returns the raw, untrimmed query text; "" when the bar is open but empty,
   *   and null when it is closed.
   */
  get filter(): string | null {
    return this.#filter;
  }

  /**
   * Set or clear the live row filter.
   *
   * @param query - the substring to narrow rows by; "" opens the bar without
   *   narrowing; null closes it and shows everything.
   */
  setFilter(query: string | null): void {
    this.#filter = query;
  }

  /**
   * Filter hidden entries and apply the current sort to one level of entries.
   *
   * @param entries - the entries of a single directory level.
   * @returns a new array, dot-files removed unless the "show hidden" setting is
   *   on, ordered by the active sort column and direction (dirs first).
   */
  #ordered(entries: DirEntry[]): DirEntry[] {
    let visible = settings.showHidden ? entries : entries.filter((e) => !e.name.startsWith("."));
    const query = this.#filter?.trim().toLowerCase();
    if (query) {
      // Expanded directories stay visible while filtered so their matching
      // children remain reachable (the walk only recurses into visible rows).
      visible = visible.filter(
        (e) =>
          e.name.toLowerCase().includes(query) || (e.kind === "dir" && this.#expanded.has(e.path)),
      );
    }
    const sorted = [...visible].sort((a, b) => compareEntries(a, b, this.#sortKey));
    if (!this.#sortAsc) sorted.reverse();
    return sorted;
  }

  /**
   * The root listing in the current sort order (directories first). Hidden files
   * are omitted unless the "show hidden files" setting is on. Does not include
   * expanded children — see {@link rows}.
   *
   * @returns a freshly built array on every read; empty before the first load.
   */
  get sortedEntries(): DirEntry[] {
    return this.#ordered(this.#entries);
  }

  /**
   * The flattened tree: the root listing plus the children of every expanded
   * directory, depth-tagged and in sort order. This is what the table renders.
   *
   * @returns the visible rows, parents immediately followed by their children.
   */
  get rows(): PaneRow[] {
    const out: PaneRow[] = [];
    /**
     * Append one level of entries to `out`, recursing into expanded folders.
     *
     * @param entries - the raw entries at this level; filtered and sorted here.
     * @param depth - the nesting level to tag these rows with.
     */
    const walk = (entries: DirEntry[], depth: number): void => {
      for (const entry of this.#ordered(entries)) {
        const expanded = this.#expanded.has(entry.path);
        out.push({
          entry,
          depth,
          expanded,
          loading: this.#childLoading.has(entry.path),
        });
        if (entry.kind === "dir" && expanded) {
          // Cached children; empty until they load.
          walk(this.#children.get(entry.path) ?? [], depth + 1);
        }
      }
    };
    walk(this.#entries, 0);
    return out;
  }

  /**
   * The currently selected entries, across every visible level of the tree.
   *
   * @returns the entries whose path is selected, in visible-row order.
   */
  get selectedEntries(): DirEntry[] {
    return this.rows.filter((r) => this.#selected.has(r.entry.path)).map((r) => r.entry);
  }

  /**
   * Whether a directory is currently expanded.
   *
   * @param path - the directory path.
   * @returns true if its children are shown.
   */
  isExpanded(path: string): boolean {
    return this.#expanded.has(path);
  }

  /**
   * Whether a directory's children have already been fetched.
   *
   * @param path - the directory path.
   * @returns true if children are cached (so expanding needs no refetch).
   */
  hasChildren(path: string): boolean {
    return this.#children.has(path);
  }

  /**
   * Cache a directory's children (called by the shell after listing it).
   *
   * @param path - the directory path.
   * @param entries - its listing.
   */
  setChildren(path: string, entries: DirEntry[]): void {
    this.#children.set(path, entries);
  }

  /**
   * Mark a directory's children as loading (drives the row spinner).
   *
   * @param path - the directory path.
   * @param loading - whether a fetch is in flight.
   */
  setChildLoading(path: string, loading: boolean): void {
    if (loading) this.#childLoading.add(path);
    else this.#childLoading.delete(path);
  }

  /**
   * Show a directory's children (no-op until they have been cached).
   *
   * @param path - the directory path.
   */
  expand(path: string): void {
    this.#expanded.add(path);
  }

  /**
   * Hide a directory's children. The cached listing is kept so re-expanding is
   * instant; a refresh or navigation clears it.
   *
   * @param path - the directory path.
   */
  collapse(path: string): void {
    this.#expanded.delete(path);
  }

  /**
   * Begin loading a new path (clears selection, error, filter, and the expanded tree).
   *
   * @param path - the directory this pane is navigating to.
   */
  startLoad(path: string): void {
    this.#path = path;
    this.#loading = true;
    this.#error = null;
    this.#filter = null;
    this.#selected = new SvelteSet();
    this.#anchor = null;
    this.#collapseTree();
  }

  /** Drop every expanded folder's state (used on navigate/refresh). */
  #collapseTree(): void {
    this.#children = new SvelteMap();
    this.#expanded = new SvelteSet();
    this.#childLoading = new SvelteSet();
  }

  /**
   * Supply loaded entries and end the loading state.
   *
   * @param entries - the root listing for the current path, unsorted and
   *   unfiltered; ordering and hidden-file filtering happen on read.
   */
  setEntries(entries: DirEntry[]): void {
    this.#entries = entries;
    this.#loading = false;
  }

  /**
   * Record a load failure, discarding the entries and the expanded tree.
   *
   * @param message - the error text to surface in the pane.
   */
  setError(message: string): void {
    this.#error = message;
    this.#loading = false;
    this.#entries = [];
    this.#collapseTree();
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
   * Apply a selection click with modifiers. Ranges follow the flattened visible
   * order, so shift-clicking across an expanded folder selects its children too.
   *
   * @param path - the clicked entry's path.
   * @param mods - ctrl/cmd (toggle) and shift (range) modifiers.
   */
  select(path: string, mods: SelectMods): void {
    const order = this.rows;
    const index = order.findIndex((r) => r.entry.path === path);
    if (index < 0) return;

    if (mods.shift && this.#anchor !== null) {
      const [lo, hi] = [this.#anchor, index].sort((a, b) => a - b);
      this.#selected = new SvelteSet(order.slice(lo, hi + 1).map((r) => r.entry.path));
    } else if (mods.ctrl) {
      const next = new SvelteSet(this.#selected);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      this.#selected = next;
      this.#anchor = index;
    } else {
      this.#selected = new SvelteSet([path]);
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
    this.#filter = null;
    this.#loading = false;
    this.#error = null;
    this.#selected = new SvelteSet();
    this.#anchor = null;
    this.#collapseTree();
  }
}

/** The two application panes. */
export const localPane = new PaneStore("local");
export const remotePane = new PaneStore("remote");
