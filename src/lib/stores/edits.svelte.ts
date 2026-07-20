// edits.svelte.ts — Live remote-file edit-and-sync sessions.
//
// Seeded by start/list command results and kept current by engine lifecycle
// events. Each row points at a managed local copy; no file bytes enter JS.

import type { EditSession } from "$lib/ipc/commands";

class EditsStore {
  #list = $state<EditSession[]>([]);

  /**
   * Every live edit session, oldest first.
   *
   * @returns the live reactive array in insertion order; empty before the first upsert or
   *   replace. Treat it as read-only — mutate via upsert/replace/remove.
   */
  get list(): EditSession[] {
    return this.#list;
  }

  /**
   * Number of live edit sessions.
   *
   * @returns the length of the tracked list; 0 when nothing is being edited.
   */
  get count(): number {
    return this.#list.length;
  }

  /**
   * Insert a new session or replace its latest state snapshot.
   *
   * @param session - backend edit-session snapshot.
   */
  upsert(session: EditSession): void {
    const existing = this.#list.some((edit) => edit.id === session.id);
    this.#list = existing
      ? this.#list.map((edit) => (edit.id === session.id ? session : edit))
      : [...this.#list, session];
  }

  /**
   * Replace the store from a backend list operation.
   *
   * @param sessions - all currently live sessions.
   */
  replace(sessions: EditSession[]): void {
    this.#list = [...sessions];
  }

  /**
   * Forget a closed edit session.
   *
   * @param id - edit session id.
   */
  remove(id: string): void {
    this.#list = this.#list.filter((edit) => edit.id !== id);
  }

  /**
   * Remove sessions whose SSH connection ended.
   *
   * @param sessionId - disconnected SSH session.
   */
  removeForSession(sessionId: string): void {
    this.#list = this.#list.filter((edit) => edit.sessionId !== sessionId);
  }
}

/** Application-wide managed edit-session store. */
export const edits = new EditsStore();
