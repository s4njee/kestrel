// prompts.svelte.ts — Pending interactive prompts (Svelte 5 runes).
//
// Holds the current host-key prompt awaiting a user decision. Set by routed
// `hostKeyPrompt` events (ipc/events.ts); consumed by HostKeyDialog (E1-S9),
// which calls `respondPrompt` and then clears it.

import type { SessionEvent } from "$lib/ipc/commands";

/** A pending host-key prompt (the `hostKeyPrompt` event payload). */
export type HostKeyPrompt = Extract<SessionEvent, { type: "hostKeyPrompt" }>;

class PromptsStore {
  #hostKey = $state<HostKeyPrompt | null>(null);

  /** The pending host-key prompt, or null when none is awaiting a decision. */
  get hostKey(): HostKeyPrompt | null {
    return this.#hostKey;
  }

  /**
   * Set the pending host-key prompt.
   *
   * @param prompt - the prompt event to surface to the user.
   */
  setHostKeyPrompt(prompt: HostKeyPrompt): void {
    this.#hostKey = prompt;
  }

  /** Clear the pending host-key prompt (after it has been answered). */
  clearHostKey(): void {
    this.#hostKey = null;
  }
}

/** Application-wide prompts store singleton. */
export const prompts = new PromptsStore();
