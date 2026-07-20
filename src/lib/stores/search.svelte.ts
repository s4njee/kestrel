// search.svelte.ts — Remote tree search state (Svelte 5 runes, E8-S7).
//
// One search at a time, which is deliberate: a search is a real round-trip (and
// on a server without `find`, a directory walk), so letting several pile up
// would compete for the same connection the panes browse over. Starting a new
// search therefore **cancels the one in flight** rather than queueing behind it.
//
// Cancellation crosses IPC by id: the backend command is awaited and offers no
// handle, so this store generates the id, hands it to `search_remote`, and
// `cancel_search` reaches the running search through it.
//
// A cancelled search leaves no error on screen. The user asked for it to stop;
// reporting "canceled" back to them as a failure would be noise.

import { cancelSearch, searchRemote, type SearchResult } from "$lib/ipc/commands";

/** The store's externally visible state (also the SearchDialog's prop shape). */
export interface SearchState {
  /** Whether a search is in flight. */
  running: boolean;
  /** The query the current `result` belongs to ("" before the first search). */
  query: string;
  /** The last completed result, or null before one has completed. */
  result: SearchResult | null;
  /** A failure message, or null. Cancellation is not a failure. */
  error: string | null;
}

/** Fallback id source when WebCrypto is unavailable. */
let counter = 0;

/**
 * Generate an id for one search.
 *
 * Uses `crypto.randomUUID` where available, falling back to a counter-based id
 * so the store still works in a test environment without WebCrypto.
 *
 * @returns a string unique among this session's searches.
 */
function nextSearchId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  counter += 1;
  return `search-${counter}`;
}

class SearchStore {
  #running = $state(false);
  #query = $state("");
  #result = $state<SearchResult | null>(null);
  #error = $state<string | null>(null);
  /** The in-flight search's id, or null when nothing is running. */
  #activeId: string | null = null;

  /**
   * The live state, in the shape SearchDialog consumes.
   *
   * @returns a snapshot object rebuilt on each read; the fields are reactive, so
   *   reading it inside a component re-renders when any of them changes.
   */
  get state(): SearchState {
    return {
      running: this.#running,
      query: this.#query,
      result: this.#result,
      error: this.#error,
    };
  }

  /**
   * Run a search, cancelling any search already in flight.
   *
   * @param sessionId - the session to search on.
   * @param root - the absolute directory to search under.
   * @param query - the substring to look for.
   * @returns a promise that resolves when the search settles. A result that
   *   arrives after a newer search has started is discarded, so a slow earlier
   *   search can never overwrite a fast later one.
   */
  async run(sessionId: string, root: string, query: string): Promise<void> {
    // Claim the slot **synchronously**, before any await. Awaiting the previous
    // search's cancellation first would leave a window in which two calls both
    // see "nothing running" and both become active.
    const previous = this.#activeId;
    const id = nextSearchId();
    this.#activeId = id;
    this.#running = true;
    this.#error = null;
    this.#query = query;
    this.#result = null;
    if (previous) void cancelSearch(previous).catch(() => {});
    try {
      const result = await searchRemote(sessionId, id, root, query);
      if (this.#activeId !== id) return;
      this.#result = result;
    } catch (e) {
      if (this.#activeId !== id) return;
      const message = String(e);
      // "canceled" is what the user asked for, not something to report.
      if (!/cancel/i.test(message)) this.#error = message;
    } finally {
      if (this.#activeId === id) {
        this.#activeId = null;
        this.#running = false;
      }
    }
  }

  /**
   * Cancel the in-flight search, if any.
   *
   * @returns a promise that resolves once the backend has been told. Safe to
   *   call when nothing is running.
   */
  async cancel(): Promise<void> {
    const id = this.#activeId;
    if (!id) return;
    this.#activeId = null;
    this.#running = false;
    await cancelSearch(id).catch(() => {});
  }

  /** Clear results and errors (on close, disconnect, or a new root). */
  reset(): void {
    this.#query = "";
    this.#result = null;
    this.#error = null;
  }
}

/** Application-wide search store singleton. */
export const search = new SearchStore();
