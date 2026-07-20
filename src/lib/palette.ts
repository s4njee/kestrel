// palette.ts — Command-palette model: commands, fuzzy matching, ranking.
//
// Pure and component-free so the interesting logic (matching, ranking, the
// command inventory) is unit-testable without rendering. CommandPalette.svelte
// renders what these functions return; the shell (+page) supplies the handlers.
//
// The inventory is built from the keymap's `ShortcutAction` union via a
// `Record<ShortcutAction, …>` parameter, so adding a shortcut without giving the
// palette a way to run it is a compile error — the palette can never silently
// lag behind the keymap.

import type { ShortcutAction } from "$lib/keymap";
import type { Bookmark } from "$lib/ipc/commands";

/** One runnable entry in the command palette. */
export interface PaletteCommand {
  /** Stable identifier (ShortcutActions use their own name). */
  id: string;
  /** The text shown and matched against, terminal-grid lowercase. */
  label: string;
  /** Dim right-aligned annotation (shortcut chord, host, …). */
  hint?: string;
  /** Perform the command. The palette closes itself after calling this. */
  run: () => void;
}

/**
 * Score a fuzzy match of `query` against `text`.
 *
 * Case-insensitive subsequence match: every query character must appear in
 * order. Scoring prefers what people mean when they type fragments — matches at
 * the start of the text or of a word, and consecutive runs — over characters
 * scattered far apart.
 *
 * @param query - what the user typed (may be empty).
 * @param text - the candidate label.
 * @returns a score (higher is better; 0 for an empty query), or null when
 *   `query` is not a subsequence of `text`.
 */
export function fuzzyMatch(query: string, text: string): number | null {
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (q.length === 0) return 0;

  let score = 0;
  let ti = 0;
  let previous = -2;
  for (const ch of q) {
    const found = t.indexOf(ch, ti);
    if (found < 0) return null;
    // Word-start and consecutive-run bonuses; distance penalty.
    if (found === 0 || t[found - 1] === " " || t[found - 1] === ":") score += 3;
    if (found === previous + 1) score += 2;
    score -= (found - ti) * 0.1;
    previous = found;
    ti = found + 1;
  }
  return score;
}

/**
 * Filter and rank commands for a query.
 *
 * @param commands - the full inventory, in canonical order.
 * @param query - what the user typed; empty keeps the canonical order.
 * @returns matching commands, best score first; ties keep inventory order.
 */
export function filterCommands(commands: PaletteCommand[], query: string): PaletteCommand[] {
  if (query.trim() === "") return commands;
  return commands
    .map((command, index) => ({ command, index, score: fuzzyMatch(query, command.label) }))
    .filter((m): m is { command: PaletteCommand; index: number; score: number } => m.score !== null)
    .sort((a, b) => b.score - a.score || a.index - b.index)
    .map((m) => m.command);
}

/** Everything the shell must supply to build the inventory. */
export interface PaletteDeps {
  /**
   * A handler for every keymap action. Typed as a total record so a new
   * `ShortcutAction` without a palette entry fails to compile.
   */
  actions: Record<ShortcutAction, () => void>;
  /** Whether a session is connected (flips the connect/disconnect label). */
  connected: boolean;
  /** Enablement for the transfer directions. */
  canUpload: boolean;
  canDownload: boolean;
  /** Open the connect dialog / disconnect the session. */
  onConnect: () => void;
  /** Open the settings dialog. */
  onSettings: () => void;
  /** Toggle the transfer queue. */
  onQueue: () => void;
  /** Create a folder in the active pane. */
  onNewFolder: () => void;
  /** Saved bookmarks, each becoming a `connect: <name>` command. */
  bookmarks: Bookmark[];
  /** Connect using a bookmark. */
  onConnectBookmark: (bookmark: Bookmark) => void;
  /** Whether a remote search can run (connected, with a remote path). */
  canSearch: boolean;
  /** Open the remote-search dialog. */
  onSearch: () => void;
  /** Whether pane diff marks are currently shown (E8-S6). */
  diffMode: boolean;
  /** Turn the pane diff marks on or off. */
  onToggleDiff: () => void;
  /** Transfer the active pane's differences to the other pane. */
  onTransferDifferences: () => void;
}

/**
 * Build the palette inventory from the app's current state.
 *
 * Commands that cannot run right now (download with nothing selected, upload
 * while disconnected) are omitted rather than shown disabled — the palette is
 * for doing, not browsing. The `palette` action itself is excluded (opening the
 * palette from the palette is noise).
 *
 * @param deps - handlers and state from the shell.
 * @returns the inventory in canonical order (session, transfer, files, view).
 */
export function buildCommands(deps: PaletteDeps): PaletteCommand[] {
  const commands: PaletteCommand[] = [];

  commands.push({
    id: "connect",
    label: deps.connected ? "disconnect from server" : "connect to server…",
    run: deps.onConnect,
  });
  for (const bookmark of deps.bookmarks) {
    commands.push({
      id: `bookmark:${bookmark.id}`,
      label: `connect: ${bookmark.name}`,
      hint: `${bookmark.username}@${bookmark.host}`,
      run: () => deps.onConnectBookmark(bookmark),
    });
  }

  if (deps.canUpload) {
    commands.push({
      id: "upload",
      label: "upload selection",
      hint: "cmd/ctrl+u",
      run: deps.actions.upload,
    });
  }
  if (deps.canDownload) {
    commands.push({
      id: "download",
      label: "download selection",
      hint: "cmd/ctrl+d",
      run: deps.actions.download,
    });
  }

  commands.push(
    { id: "refresh", label: "refresh active pane", hint: "cmd/ctrl+r", run: deps.actions.refresh },
    { id: "focusPath", label: "focus path field", hint: "cmd/ctrl+l", run: deps.actions.focusPath },
    { id: "switchPane", label: "switch pane", hint: "tab", run: deps.actions.switchPane },
    { id: "filter", label: "filter rows…", hint: "/", run: deps.actions.filter },
    { id: "rename", label: "rename selected…", hint: "f2", run: deps.actions.rename },
    { id: "delete", label: "delete selected…", hint: "del", run: deps.actions.delete },
    { id: "newFolder", label: "new folder…", run: deps.onNewFolder },
  );

  // Search is remote-only, so unlike the in-pane filter it needs a session.
  if (deps.canSearch) {
    commands.push({
      id: "search",
      label: "search remote files…",
      hint: "cmd/ctrl+f",
      run: deps.onSearch,
    });
  }

  // Diff mode compares the two panes, so it needs a remote pane to compare with.
  if (deps.connected) {
    commands.push({
      id: "diff",
      label: deps.diffMode ? "hide pane differences" : "compare panes (diff mode)",
      run: deps.onToggleDiff,
    });
    // "Transfer the differences" is only offered while the marks are actually on
    // screen: without them it names a set the user has not been shown.
    if (deps.diffMode) {
      commands.push({
        id: "transferDifferences",
        label: "transfer differences to the other pane",
        run: deps.onTransferDifferences,
      });
    }
  }

  commands.push(
    { id: "queue", label: "toggle transfer queue", run: deps.onQueue },
    { id: "settings", label: "settings…", run: deps.onSettings },
  );

  return commands;
}
