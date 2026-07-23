// ui.svelte.ts — Global UI state (Svelte 5 runes).
//
// Holds view-only state that outlives any single component: the split-pane
// ratio and the console/shell height (both persisted to localStorage so they
// survive reload), which pane is active, and whether the transfer dock is
// expanded. This is a runes store —
// a singleton class instance whose `$state` fields are reactive anywhere they
// are read. No IPC or file access here.

import type { PaneKind } from "$lib/types";

const RATIO_KEY = "kestrel.splitRatio";
const MIN_RATIO = 0.15;
const MAX_RATIO = 0.85;

const FOLLOW_KEY = "kestrel.followShellCwd";
const CONSOLE_KEY = "kestrel.consoleHeight";
/** Default console height: 18 text lines plus the tab strip and padding. */
const DEFAULT_CONSOLE_HEIGHT = 360;
/** Keep at least a couple of lines visible, and always leave room for the panes. */
const MIN_CONSOLE_HEIGHT = 64;
/** Never let the console take more than this share of the window. */
const MAX_CONSOLE_FRACTION = 0.8;

/**
 * Clamp a split ratio into the allowed range.
 *
 * @param value - proposed left-pane fraction (0..1).
 * @returns the value constrained to [MIN_RATIO, MAX_RATIO]; falls back to 0.5
 *   for non-finite input.
 */
function clampRatio(value: number): number {
  if (!Number.isFinite(value)) return 0.5;
  return Math.min(MAX_RATIO, Math.max(MIN_RATIO, value));
}

/**
 * Clamp a console height into the allowed range.
 *
 * The upper bound is a fraction of the current window so the console can never
 * squeeze the file panes off screen; it falls back to a fixed ceiling when no
 * window is available (build/SSR).
 *
 * @param value - proposed height in pixels.
 * @returns the value constrained to [MIN_CONSOLE_HEIGHT, 80% of the window];
 *   falls back to the default for non-finite input.
 */
function clampConsoleHeight(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_CONSOLE_HEIGHT;
  const ceiling =
    typeof window !== "undefined" && window.innerHeight > 0
      ? window.innerHeight * MAX_CONSOLE_FRACTION
      : 1000;
  return Math.min(Math.max(MIN_CONSOLE_HEIGHT, value), Math.max(MIN_CONSOLE_HEIGHT, ceiling));
}

/**
 * The Window's localStorage, or null outside a browser/jsdom context.
 *
 * Accessed via `window` (not the bare global) so Node's experimental, inert
 * `globalThis.localStorage` never shadows jsdom's real implementation in tests.
 *
 * @returns a Storage instance, or null when no DOM is present (SSR/build).
 */
function storage(): Storage | null {
  return typeof window !== "undefined" ? window.localStorage : null;
}

/**
 * Read the persisted split ratio from localStorage.
 *
 * @returns the stored ratio, or 0.5 when unavailable/invalid. Guarded so it is
 *   safe to call in non-browser (build) contexts.
 */
function loadRatio(): number {
  const raw = storage()?.getItem(RATIO_KEY);
  return raw == null ? 0.5 : clampRatio(Number.parseFloat(raw));
}

/**
 * Read the persisted console height from localStorage.
 *
 * @returns the stored height, or the default when unavailable/invalid.
 */
function loadConsoleHeight(): number {
  const raw = storage()?.getItem(CONSOLE_KEY);
  return raw == null ? DEFAULT_CONSOLE_HEIGHT : clampConsoleHeight(Number.parseFloat(raw));
}

/**
 * Read the persisted "follow shell cwd" preference.
 *
 * @returns true when the remote pane should follow the shell's directory;
 *   defaults to false (opt-in) when unset or unavailable.
 */
function loadFollowCwd(): boolean {
  return storage()?.getItem(FOLLOW_KEY) === "true";
}

/** Reactive singleton backing the application shell. */
class UiStore {
  #splitRatio = $state(loadRatio());
  #consoleHeight = $state(loadConsoleHeight());
  #followShellCwd = $state(loadFollowCwd());
  #diffMode = $state(false);
  #activePane = $state<PaneKind>("local");
  #transferPanelExpanded = $state(false);

  /**
   * Left-pane fraction of the split (0..1).
   *
   * @returns the ratio restored from localStorage, clamped to [0.15, 0.85];
   *   0.5 when nothing was stored.
   */
  get splitRatio(): number {
    return this.#splitRatio;
  }

  /**
   * Set and persist the split ratio.
   *
   * @param value - proposed left-pane fraction; clamped before storing.
   */
  set splitRatio(value: number) {
    this.#splitRatio = clampRatio(value);
    storage()?.setItem(RATIO_KEY, String(this.#splitRatio));
  }

  /**
   * Height of the bottom console/shell region, in pixels.
   *
   * @returns the height restored from localStorage, clamped to at least 64px and
   *   at most 80% of the window; 360 when nothing was stored.
   */
  get consoleHeight(): number {
    return this.#consoleHeight;
  }

  /**
   * Set and persist the console/shell height.
   *
   * @param value - proposed height in pixels; clamped before storing.
   */
  set consoleHeight(value: number) {
    this.#consoleHeight = clampConsoleHeight(value);
    storage()?.setItem(CONSOLE_KEY, String(this.#consoleHeight));
  }

  /**
   * Whether the remote pane follows the shell's working directory (E8-S5).
   *
   * @returns the persisted preference; false (opt-in) when nothing was stored.
   */
  get followShellCwd(): boolean {
    return this.#followShellCwd;
  }

  /** Toggle whether the remote pane follows the shell's directory. */
  toggleFollowShellCwd(): void {
    this.#followShellCwd = !this.#followShellCwd;
    storage()?.setItem(FOLLOW_KEY, String(this.#followShellCwd));
  }

  /**
   * Whether both panes are marked up with comparison glyphs (E8-S6).
   *
   * Deliberately **not** persisted, unlike the other toggles here: diff mode is
   * an inspection mode for a particular pair of directories, so restoring it on
   * launch would decorate two unrelated panes with marks the user never asked
   * for. It also turns itself off on disconnect, since one side is then gone.
   *
   * @returns true when diff marks should be shown; false initially.
   */
  get diffMode(): boolean {
    return this.#diffMode;
  }

  /** Toggle the pane diff marks. */
  toggleDiffMode(): void {
    this.#diffMode = !this.#diffMode;
  }

  /**
   * Force diff mode on or off (used to clear it on disconnect).
   *
   * @param enabled - the desired state.
   */
  setDiffMode(enabled: boolean): void {
    this.#diffMode = enabled;
  }

  /**
   * The pane that currently has keyboard focus / is the action target.
   *
   * @returns "local" or "remote"; "local" until {@link setActivePane} is called.
   *   Not persisted, so it resets on reload.
   */
  get activePane(): PaneKind {
    return this.#activePane;
  }

  /**
   * Whether the bottom transfer dock is expanded.
   *
   * @returns true when the dock is open; false initially and after reload, since
   *   this state is not persisted.
   */
  get transferPanelExpanded(): boolean {
    return this.#transferPanelExpanded;
  }

  /**
   * Mark a pane as active.
   *
   * @param pane - the pane to focus.
   */
  setActivePane(pane: PaneKind): void {
    this.#activePane = pane;
  }

  /** Toggle the transfer dock between expanded and collapsed. */
  toggleTransferPanel(): void {
    this.#transferPanelExpanded = !this.#transferPanelExpanded;
  }

  /**
   * Force the transfer dock open or closed.
   *
   * @param expanded - desired expanded state.
   */
  setTransferPanelExpanded(expanded: boolean): void {
    this.#transferPanelExpanded = expanded;
  }
}

/** Application-wide UI store singleton. */
export const ui = new UiStore();
