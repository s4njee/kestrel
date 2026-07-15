// transfers.svelte.ts — Transfer queue state (Svelte 5 runes).
//
// Fed by the transfer event channel (see ipc/events.ts). Entries are seeded by
// the download/upload actions when a transfer is enqueued (they know the name,
// direction, and size), then updated in place by state and progress events.

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

/** Seed info supplied when a transfer is enqueued. */
export interface TransferSeed {
  id: string;
  direction: TransferDirection;
  name: string;
  size: number;
}

class TransfersStore {
  #list = $state<Transfer[]>([]);

  /** All tracked transfers, newest last. */
  get list(): Transfer[] {
    return this.#list;
  }

  /** Number of transfers that are queued or running. */
  get activeCount(): number {
    return this.#list.filter((t) => t.state === "queued" || t.state === "running").length;
  }

  /**
   * Seed a newly enqueued transfer.
   *
   * @param seed - id/direction/name/size known at enqueue time.
   */
  add(seed: TransferSeed): void {
    this.#list = [
      ...this.#list,
      {
        id: seed.id,
        direction: seed.direction,
        name: seed.name,
        state: "queued",
        bytes: 0,
        size: seed.size,
        rateBps: 0,
        error: null,
      },
    ];
  }

  /**
   * Apply a state change from an event.
   *
   * @param id - the transfer id.
   * @param state - the new state.
   * @param error - failure message, if any.
   */
  setState(id: string, state: TransferStateStr, error: string | null): void {
    this.#list = this.#list.map((t) => (t.id === id ? { ...t, state, error } : t));
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

  /** Remove completed/failed/canceled transfers. */
  clearCompleted(): void {
    this.#list = this.#list.filter((t) => t.state === "queued" || t.state === "running");
  }
}

/** Application-wide transfers store singleton. */
export const transfers = new TransfersStore();
