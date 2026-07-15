// ui.svelte.ts — Global UI state (Svelte 5 runes).
//
// Holds view-only state that outlives any single component: the split-pane
// ratio (persisted to localStorage so it survives reload), which pane is
// active, and whether the transfer dock is expanded. This is a runes store —
// a singleton class instance whose `$state` fields are reactive anywhere they
// are read. No IPC or file access here.

import type { PaneKind } from "$lib/types";

const RATIO_KEY = "sftpapp.splitRatio";
const MIN_RATIO = 0.15;
const MAX_RATIO = 0.85;

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

/** Reactive singleton backing the application shell. */
class UiStore {
  #splitRatio = $state(loadRatio());
  #activePane = $state<PaneKind>("local");
  #transferPanelExpanded = $state(false);

  /** Left-pane fraction of the split (0..1). */
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

  /** The pane that currently has keyboard focus / is the action target. */
  get activePane(): PaneKind {
    return this.#activePane;
  }

  /** Whether the bottom transfer dock is expanded. */
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
