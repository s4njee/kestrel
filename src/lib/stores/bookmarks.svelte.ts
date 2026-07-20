// bookmarks.svelte.ts — Saved-connection bookmarks (Svelte 5 runes).
//
// Mirrors the backend bookmark store: `load` fetches the list once, and the
// mutation helpers keep the in-memory list in sync with save/delete command
// results so the UI updates without a full reload. Secrets never live here — a
// bookmark only records `hasSavedSecret`.

import {
  listBookmarks,
  saveBookmark as saveBookmarkCmd,
  deleteBookmark as deleteBookmarkCmd,
  type Bookmark,
} from "$lib/ipc/commands";

class BookmarksStore {
  #items = $state<Bookmark[]>([]);
  #loaded = $state(false);

  /**
   * All saved bookmarks.
   *
   * @returns the live reactive array in backend order, with locally saved bookmarks appended;
   *   empty until load() resolves. Treat it as read-only — mutate via load/save/remove.
   */
  get items(): Bookmark[] {
    return this.#items;
  }

  /**
   * Whether the initial load has completed.
   *
   * @returns false until the first load() resolves, then true for the rest of the app run;
   *   never reset by save or remove.
   */
  get loaded(): boolean {
    return this.#loaded;
  }

  /**
   * Fetch the bookmark list from the backend (once per app run is enough).
   *
   * @returns a promise that resolves after the list is populated.
   */
  async load(): Promise<void> {
    this.#items = await listBookmarks();
    this.#loaded = true;
  }

  /**
   * Insert or replace a bookmark in the local list (matched by id).
   *
   * @param bookmark - the stored bookmark returned by the backend.
   */
  applyUpsert(bookmark: Bookmark): void {
    const idx = this.#items.findIndex((b) => b.id === bookmark.id);
    this.#items =
      idx >= 0 ? this.#items.map((b, i) => (i === idx ? bookmark : b)) : [...this.#items, bookmark];
  }

  /**
   * Save a bookmark (and optional secret), updating the local list.
   *
   * @param bookmark - the details to store (nil id ⇒ new).
   * @param secret - optional password/passphrase to persist in the keychain.
   * @returns the stored bookmark, with its assigned id.
   */
  async save(bookmark: Bookmark, secret?: string): Promise<Bookmark> {
    const saved = await saveBookmarkCmd(bookmark, secret);
    this.applyUpsert(saved);
    return saved;
  }

  /**
   * Delete a bookmark and drop it from the local list.
   *
   * @param id - the bookmark id.
   */
  async remove(id: string): Promise<void> {
    await deleteBookmarkCmd(id);
    this.#items = this.#items.filter((b) => b.id !== id);
  }
}

/** Application-wide bookmarks store singleton. */
export const bookmarks = new BookmarksStore();
