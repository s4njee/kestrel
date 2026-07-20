// toasts.svelte.ts — Transient notifications (Svelte 5 runes).
//
// Per the error-handling invariant, transient failures surface as toasts (while
// listing failures stay as inline pane banners). Each toast auto-dismisses after
// a TTL; callers can also dismiss early. Kept UI-agnostic — Toasts.svelte
// renders the list.

/** Severity of a toast (affects styling). */
export type ToastKind = "error" | "info";

/** A single transient notification. */
export interface Toast {
  id: number;
  message: string;
  kind: ToastKind;
}

/** How long a toast stays before auto-dismissing (ms). */
const DEFAULT_TTL = 6000;

class ToastsStore {
  #items = $state<Toast[]>([]);
  #nextId = 0;

  /**
   * The active toasts, oldest first.
   *
   * @returns the live reactive array (not a copy); empty when nothing is showing.
   */
  get items(): Toast[] {
    return this.#items;
  }

  /**
   * Show a toast.
   *
   * @param message - the text to display.
   * @param kind - severity ("error" or "info"); defaults to "error".
   * @param ttl - auto-dismiss delay in ms (0 disables auto-dismiss).
   * @returns the new toast's id.
   */
  show(message: string, kind: ToastKind = "error", ttl = DEFAULT_TTL): number {
    const id = this.#nextId++;
    this.#items = [...this.#items, { id, message, kind }];
    if (ttl > 0) setTimeout(() => this.dismiss(id), ttl);
    return id;
  }

  /**
   * Shorthand for an error toast, shown with the default TTL.
   *
   * @param message - the text to display.
   * @returns the new toast's id, usable with `dismiss`.
   */
  error(message: string): number {
    return this.show(message, "error");
  }

  /**
   * Shorthand for an info toast, shown with the default TTL.
   *
   * @param message - the text to display.
   * @returns the new toast's id, usable with `dismiss`.
   */
  info(message: string): number {
    return this.show(message, "info");
  }

  /**
   * Dismiss a toast by id (no-op if already gone).
   *
   * @param id - the toast to remove.
   */
  dismiss(id: number): void {
    this.#items = this.#items.filter((t) => t.id !== id);
  }
}

/** Application-wide toasts store singleton. */
export const toasts = new ToastsStore();
