// sessions.svelte.ts — Connected-session state (Svelte 5 runes).
//
// Tracks the set of live sessions and which one is active. Updated by the
// connect flow (on a successful `connect`) and by routed `connectionState`
// events (see ipc/events.ts).

import type { SessionInfo } from "$lib/ipc/commands";

/** Connection state of a tracked session. */
export type ConnState = "connected" | "disconnected" | "reconnecting";

/** A tracked session plus its current connection state. */
export interface SessionEntry {
  info: SessionInfo;
  state: ConnState;
}

class SessionsStore {
  #entries = $state<SessionEntry[]>([]);
  #activeId = $state<string | null>(null);

  /** All tracked sessions. */
  get entries(): SessionEntry[] {
    return this.#entries;
  }

  /** The active session id, or null when none is selected. */
  get activeId(): string | null {
    return this.#activeId;
  }

  /** The active session entry, or null. */
  get active(): SessionEntry | null {
    return this.#entries.find((e) => e.info.id === this.#activeId) ?? null;
  }

  /**
   * Track a newly connected session and make it active.
   *
   * @param info - the session info returned by `connect`.
   */
  add(info: SessionInfo): void {
    if (!this.#entries.some((e) => e.info.id === info.id)) {
      this.#entries = [...this.#entries, { info, state: "connected" }];
    }
    this.#activeId = info.id;
  }

  /**
   * Remove a session and clear it as active if needed.
   *
   * @param id - the session id to drop.
   */
  remove(id: string): void {
    this.#entries = this.#entries.filter((e) => e.info.id !== id);
    if (this.#activeId === id) {
      this.#activeId = this.#entries.at(-1)?.info.id ?? null;
    }
  }

  /**
   * Set the active session.
   *
   * @param id - the session id to activate.
   */
  setActive(id: string): void {
    if (this.#entries.some((e) => e.info.id === id)) this.#activeId = id;
  }

  /**
   * Apply a connection-state change from an event.
   *
   * @param id - the affected session.
   * @param state - the new connection state; "disconnected" drops the session.
   */
  setConnectionState(id: string, state: ConnState): void {
    if (state === "disconnected") {
      this.remove(id);
      return;
    }
    this.#entries = this.#entries.map((e) => (e.info.id === id ? { ...e, state } : e));
  }
}

/** Application-wide sessions store singleton. */
export const sessions = new SessionsStore();
