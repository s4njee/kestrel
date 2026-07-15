// prompts.svelte.ts — Pending interactive prompts (Svelte 5 runes).
//
// Holds the current host-key prompt awaiting a user decision. Set by routed
// `hostKeyPrompt` events (ipc/events.ts); consumed by HostKeyDialog (E1-S9),
// which calls `respondPrompt` and then clears it.

import type { SessionEvent } from "$lib/ipc/commands";

/** A pending host-key prompt (the `hostKeyPrompt` event payload). */
export type HostKeyPrompt = Extract<SessionEvent, { type: "hostKeyPrompt" }>;
/** A pending keyboard-interactive auth prompt (the `authPrompt` event payload). */
export type AuthPrompt = Extract<SessionEvent, { type: "authPrompt" }>;

class PromptsStore {
  #hostKey = $state<HostKeyPrompt | null>(null);
  #auth = $state<AuthPrompt | null>(null);

  /** The pending host-key prompt, or null when none is awaiting a decision. */
  get hostKey(): HostKeyPrompt | null {
    return this.#hostKey;
  }

  /** The pending keyboard-interactive auth prompt, or null. */
  get auth(): AuthPrompt | null {
    return this.#auth;
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

  /**
   * Set the pending keyboard-interactive auth prompt.
   *
   * @param prompt - the auth challenge to surface to the user.
   */
  setAuthPrompt(prompt: AuthPrompt): void {
    this.#auth = prompt;
  }

  /** Clear the pending auth prompt (after it has been answered). */
  clearAuth(): void {
    this.#auth = null;
  }
}

/** Application-wide prompts store singleton. */
export const prompts = new PromptsStore();
