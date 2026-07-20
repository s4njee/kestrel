// conflicts.svelte.ts — Pending transfer conflicts (Svelte 5 runes).
//
// Holds a queue of destination-exists conflicts awaiting a user decision. The
// ConflictDialog (E3-S6) shows the first pending one; answering it (via
// resolveConflict) removes it and reveals the next.

import type { TransferEvent } from "$lib/ipc/commands";

/** A pending conflict (the `conflict` event payload). */
export type Conflict = Extract<TransferEvent, { type: "conflict" }>;

class ConflictsStore {
  #queue = $state<Conflict[]>([]);

  /**
   * The conflict currently awaiting a decision, or null.
   *
   * @returns the head of the queue (the oldest unresolved conflict), or null when nothing
   *   is pending.
   */
  get current(): Conflict | null {
    return this.#queue[0] ?? null;
  }

  /**
   * Number of pending conflicts.
   *
   * @returns the queue length; 0 once every conflict has been resolved or cleared.
   */
  get count(): number {
    return this.#queue.length;
  }

  /**
   * Enqueue a new conflict (ignores duplicates by id).
   *
   * @param conflict - the conflict event.
   */
  add(conflict: Conflict): void {
    if (!this.#queue.some((c) => c.id === conflict.id)) {
      this.#queue = [...this.#queue, conflict];
    }
  }

  /**
   * Remove a resolved conflict by id.
   *
   * @param id - the transfer id that was resolved.
   */
  resolve(id: string): void {
    this.#queue = this.#queue.filter((c) => c.id !== id);
  }

  /** Clear all pending conflicts (e.g. after apply-to-all). */
  clear(): void {
    this.#queue = [];
  }
}

/** Application-wide conflicts store singleton. */
export const conflicts = new ConflictsStore();
