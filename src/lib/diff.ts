// diff.ts — Pane diff model: compare two loaded trees by relative path (E8-S6).
//
// Pure and component-free so the comparison is unit-testable without rendering.
// The panes hand over their already-flattened rows (`PaneStore.rows`); nothing
// here touches IPC, so diffing costs one pass over what is already on screen —
// no extra listing, and explicitly **no hashing** in this version of the story.
//
// Keying is by path **relative to each pane's root**, rebuilt from the flattened
// tree's depth tags rather than by string-slicing absolute paths. That keeps the
// comparison separator-agnostic: a Windows local pane (`C:\src\app\main.rs`) and
// a POSIX remote pane (`/srv/app/main.rs`) still line up on `app/main.rs`.
//
// ## Why four marks and not three
//
// The story asks for "differs (size or mtime)" as one mark. Taken literally that
// mark would fire on nearly every row, because **this app does not preserve
// mtime across a transfer** — the engine writes the destination and lets the
// filesystem stamp it, so a file uploaded a minute ago already has two different
// timestamps with identical bytes. A single `≠` would therefore mean "these are
// different, or you copied one to the other, who knows", which is no signal at
// all; ignoring mtime instead would hide same-size edits, which is the other
// failure. So the two cases are kept apart:
//
//   `≠` sizes differ        — the contents certainly differ.
//   `~` sizes match, mtimes differ beyond the tolerance — *may* differ; this is
//       also what a plain copy looks like, so it is deliberately the weaker mark
//       and is **not** included in "transfer the differences".
//
// See {@link MTIME_TOLERANCE_SECONDS} for why the comparison is not exact.

import type { PaneRow } from "$lib/stores/panes.svelte";
import type { DirEntry } from "$lib/ipc/commands";

/**
 * How a row compares with its counterpart in the other pane.
 *
 * - `same` — present on both sides, same size, same mtime (within tolerance).
 * - `differs` — present on both sides with different sizes (or mismatched kinds).
 * - `timestamp` — same size, different mtime. See the note above.
 * - `only` — no counterpart in the other pane.
 */
export type DiffMark = "same" | "differs" | "timestamp" | "only";

/** The glyph shown for each mark, in the terminal-grid idiom. */
export const DIFF_GLYPHS: Record<DiffMark, string> = {
  same: "=",
  differs: "≠",
  timestamp: "~",
  only: "+",
};

/**
 * Slack allowed when comparing modification times, in seconds.
 *
 * Timestamps are not directly comparable across filesystems: SFTP carries whole
 * seconds, FAT/exFAT store even seconds only, and servers in a different
 * timezone still report epoch seconds but may be a second off from NTP drift. A
 * two-second window is the long-standing rsync convention for exactly this.
 */
export const MTIME_TOLERANCE_SECONDS = 2;

/**
 * Rebuild each row's path relative to its pane root.
 *
 * The flattened tree lists a parent immediately before its children, so a stack
 * indexed by depth is enough to reconstruct the ancestry without any string
 * surgery on absolute paths.
 *
 * @param rows - one pane's flattened rows, parents before their children.
 * @returns a map from absolute entry path to slash-joined relative path. Absolute
 *   paths are unique within a pane, which makes this map directly usable as a
 *   per-row lookup key at render time.
 */
export function relativePaths(rows: PaneRow[]): Map<string, string> {
  const out = new Map<string, string>();
  const ancestors: string[] = [];
  for (const row of rows) {
    ancestors.length = row.depth;
    ancestors[row.depth] = row.entry.name;
    out.set(row.entry.path, ancestors.join("/"));
  }
  return out;
}

/**
 * Compare one entry against its counterpart in the other pane.
 *
 * Directories are compared by existence only: their reported size is not a
 * measure of their contents, and their mtime changes whenever a child is
 * touched, so neither says anything useful about whether the trees agree.
 *
 * @param a - the entry in this pane.
 * @param b - the counterpart in the other pane.
 * @returns the mark describing how they relate; never `only` (both exist).
 */
function compare(a: DirEntry, b: DirEntry): DiffMark {
  if (a.kind !== b.kind) return "differs";
  if (a.kind === "dir") return "same";
  if (a.size !== b.size) return "differs";
  if (a.mtime == null || b.mtime == null) return "same";
  return Math.abs(a.mtime - b.mtime) > MTIME_TOLERANCE_SECONDS ? "timestamp" : "same";
}

/** A diff of two panes, as per-row lookups keyed by absolute entry path. */
export interface PaneDiff {
  /** Marks for the local pane's rows. */
  local: Map<string, DiffMark>;
  /** Marks for the remote pane's rows. */
  remote: Map<string, DiffMark>;
}

/**
 * Diff two panes' visible trees.
 *
 * Only what is currently loaded takes part: the root listings plus the children
 * of expanded folders. A collapsed directory is compared as a directory (does it
 * exist on both sides), not by its contents — expanding it is what asks for the
 * deeper answer, and the marks refresh from the newly visible rows.
 *
 * @param localRows - the local pane's flattened rows.
 * @param remoteRows - the remote pane's flattened rows.
 * @returns marks for both panes. Every input row appears in its side's map, so a
 *   renderer can treat a missing key as "diff mode is off" rather than "unknown".
 */
export function diffPanes(localRows: PaneRow[], remoteRows: PaneRow[]): PaneDiff {
  const localRel = relativePaths(localRows);
  const remoteRel = relativePaths(remoteRows);

  /**
   * Index one pane's entries by their relative path.
   *
   * @param rows - that pane's rows.
   * @param rel - its absolute→relative map from {@link relativePaths}.
   * @returns the entries keyed by relative path, ready to look counterparts up in.
   */
  const byRelative = (rows: PaneRow[], rel: Map<string, string>): Map<string, DirEntry> => {
    const index = new Map<string, DirEntry>();
    for (const row of rows) index.set(rel.get(row.entry.path)!, row.entry);
    return index;
  };
  const localIndex = byRelative(localRows, localRel);
  const remoteIndex = byRelative(remoteRows, remoteRel);

  /**
   * Mark every row in one pane against the other pane's index.
   *
   * @param rows - the pane's rows.
   * @param rel - its absolute→relative map.
   * @param other - the other pane's entries, keyed by relative path.
   * @returns a mark for each row, keyed by absolute entry path.
   */
  const mark = (
    rows: PaneRow[],
    rel: Map<string, string>,
    other: Map<string, DirEntry>,
  ): Map<string, DiffMark> => {
    const marks = new Map<string, DiffMark>();
    for (const row of rows) {
      const counterpart = other.get(rel.get(row.entry.path)!);
      marks.set(row.entry.path, counterpart ? compare(row.entry, counterpart) : "only");
    }
    return marks;
  };

  return {
    local: mark(localRows, localRel, remoteIndex),
    remote: mark(remoteRows, remoteRel, localIndex),
  };
}

/**
 * The top-level entries worth transferring to the other pane.
 *
 * Restricted to depth 0 on purpose. A nested row's destination is a directory
 * *inside* the other pane's tree, which may not exist there at all — enqueueing
 * into it would fail at transfer time, after the user has been told the work was
 * queued. Root-level rows always have a destination that exists (the other
 * pane's current directory), and a root-level directory marked `only` carries
 * its whole subtree along with it, so this covers most of what the marks show.
 * Deeper differences stay visible as marks; acting on them is navigating into
 * the folder, where they become root-level.
 *
 * `timestamp` rows are excluded: same size and a different clock is precisely
 * what a completed copy looks like here (see the note at the top of this file),
 * so including them would re-transfer the same bytes forever.
 *
 * @param rows - the pane's flattened rows.
 * @param marks - that pane's marks from {@link diffPanes}.
 * @returns the depth-0 entries marked `only` or `differs`, in row order.
 */
export function differingEntries(rows: PaneRow[], marks: Map<string, DiffMark>): DirEntry[] {
  return rows
    .filter((row) => {
      if (row.depth !== 0) return false;
      const mark = marks.get(row.entry.path);
      return mark === "only" || mark === "differs";
    })
    .map((row) => row.entry);
}
