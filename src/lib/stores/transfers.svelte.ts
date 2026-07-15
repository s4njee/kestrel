// transfers.svelte.ts — Transfer queue state (Svelte 5 runes).
//
// Driven entirely by the transfer event channel (see ipc/events.ts): each state
// event upserts a row (self-contained with name/size/direction), and progress
// events update bytes/rate. No frontend seeding is needed, so recursive
// directory transfers — whose per-file rows are created backend-side — appear
// automatically.

import type { TransferDirection, TransferStateStr } from "$lib/ipc/commands";

/** A tracked transfer row. */
export interface Transfer {
  id: string;
  direction: TransferDirection;
  name: string;
  state: TransferStateStr;
  bytes: number;
  size: number;
  rateBps: number;
  error: string | null;
}

/** Fields carried by a transfer state event. */
export interface TransferStateUpdate {
  id: string;
  state: TransferStateStr;
  name: string;
  size: number;
  bytes: number;
  direction: TransferDirection;
  error: string | null;
}

class TransfersStore {
  #list = $state<Transfer[]>([]);

  /** All tracked transfers, newest last. */
  get list(): Transfer[] {
    return this.#list;
  }

  /**
   * Whether a state is active (not finished).
   *
   * @param state - a transfer lifecycle state.
   * @returns true for queued/running/paused, false for terminal states.
   */
  static isActive(state: TransferStateStr): boolean {
    return state === "queued" || state === "running" || state === "paused";
  }

  /** Number of active (queued/running/paused) transfers. */
  get activeCount(): number {
    return this.#list.filter((t) => TransfersStore.isActive(t.state)).length;
  }

  /**
   * Upsert a transfer from a (self-contained) state event.
   *
   * @param update - id/state/name/size/bytes/direction/error from the event.
   */
  applyState(update: TransferStateUpdate): void {
    const existing = this.#list.find((t) => t.id === update.id);
    if (existing) {
      this.#list = this.#list.map((t) =>
        t.id === update.id ? { ...t, state: update.state, error: update.error } : t,
      );
    } else {
      this.#list = [
        ...this.#list,
        {
          id: update.id,
          direction: update.direction,
          name: update.name,
          state: update.state,
          bytes: update.bytes,
          size: update.size,
          rateBps: 0,
          error: update.error,
        },
      ];
    }
  }

  /**
   * Apply a batch of progress samples.
   *
   * @param items - per-transfer byte counts and rates.
   */
  setProgress(items: { id: string; bytes: number; rateBps: number }[]): void {
    if (items.length === 0) return;
    const byId = new Map(items.map((i) => [i.id, i]));
    this.#list = this.#list.map((t) => {
      const p = byId.get(t.id);
      return p ? { ...t, bytes: p.bytes, rateBps: p.rateBps } : t;
    });
  }

  /** Remove completed/failed/canceled transfers (keeps active + paused). */
  clearCompleted(): void {
    this.#list = this.#list.filter((t) => TransfersStore.isActive(t.state));
  }
}

/** Application-wide transfers store singleton. */
export const transfers = new TransfersStore();
