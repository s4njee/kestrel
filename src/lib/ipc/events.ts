// events.ts — Route backend session events into stores.
//
// `initSessionEvents` subscribes once at app start; each event is dispatched by
// `routeSessionEvent` into the relevant runes store. Kept separate and exported
// so it can be unit-tested without a live Tauri channel.

import {
  fromBase64,
  subscribeSessionEvents,
  subscribeTransferEvents,
  type SessionEvent,
  type TransferEvent,
} from "./commands";
import { sessions } from "$lib/stores/sessions.svelte";
import { prompts } from "$lib/stores/prompts.svelte";
import { transfers } from "$lib/stores/transfers.svelte";
import { conflicts } from "$lib/stores/conflicts.svelte";
import { toasts } from "$lib/stores/toasts.svelte";
import { logs } from "$lib/stores/logs.svelte";
import { health } from "$lib/stores/health.svelte";
import { edits } from "$lib/stores/edits.svelte";

/** Handler invoked with decoded shell output (set by the terminal component). */
let shellDataHandler: ((shellId: string, data: Uint8Array) => void) | null = null;
/** Handler invoked when a shell ends (set by the terminal component). */
let shellClosedHandler: ((shellId: string) => void) | null = null;

/**
 * Register the interactive-shell handlers.
 *
 * The terminal component owns rendering, so it injects these rather than this
 * routing module reaching into the DOM.
 *
 * @param onData - called with decoded output bytes, or null to clear.
 * @param onClosed - called when the shell ends, or null to clear.
 */
export function setShellHandlers(
  onData: ((shellId: string, data: Uint8Array) => void) | null,
  onClosed: ((shellId: string) => void) | null,
): void {
  shellDataHandler = onData;
  shellClosedHandler = onClosed;
}

/** Handler invoked when the watched local directory changes (set by the shell). */
let localDirChangedHandler: ((path: string) => void) | null = null;

/**
 * Register the local-directory-change handler.
 *
 * The reload is a side-effecting IPC call owned by the shell (`+page`), so it is
 * injected here rather than performed in this routing module.
 *
 * @param handler - called with the changed directory path, or null to clear.
 */
export function setLocalDirChangedHandler(handler: ((path: string) => void) | null): void {
  localDirChangedHandler = handler;
}

/**
 * Dispatch a single session event into the stores.
 *
 * @param event - the event received from the backend channel.
 */
export function routeSessionEvent(event: SessionEvent): void {
  switch (event.type) {
    case "connectionState":
      sessions.setConnectionState(event.sessionId, event.state);
      if (event.state === "disconnected") health.forget(event.sessionId);
      if (event.state === "reconnecting") logs.status("Connection lost — reconnecting…");
      else if (event.state === "disconnected")
        logs.status(`Disconnected${event.reason ? ` — ${event.reason}` : ""}`);
      else if (event.state === "connected") logs.status("Connection re-established", true);
      if (event.state === "disconnected") edits.removeForSession(event.sessionId);
      break;
    case "hostKeyPrompt":
      prompts.setHostKeyPrompt(event);
      break;
    case "authPrompt":
      prompts.setAuthPrompt(event);
      break;
    case "localDirChanged":
      localDirChangedHandler?.(event.path);
      break;
    case "editSessionChanged":
      edits.upsert(event.session);
      if (event.session.state === "conflict") {
        toasts.error(`Edit conflict: ${event.session.remotePath} changed remotely`);
      } else if (event.session.state === "error" && event.session.error) {
        toasts.error(`Edit sync failed: ${event.session.error}`);
      }
      break;
    case "editSessionClosed":
      edits.remove(event.editId);
      break;
    case "shellData":
      shellDataHandler?.(event.shellId, fromBase64(event.data));
      break;
    case "shellClosed":
      shellClosedHandler?.(event.shellId);
      break;
    case "latencySample":
      health.record(event.sessionId, event.rttMs);
      break;
  }
}

/**
 * Dispatch a single transfer event into the transfers store.
 *
 * @param event - the event received from the transfer channel.
 */
export function routeTransferEvent(event: TransferEvent): void {
  switch (event.type) {
    case "state":
      transfers.applyState({
        id: event.id,
        state: event.state,
        name: event.name,
        size: event.size,
        bytes: event.bytes,
        direction: event.direction,
        error: event.error,
      });
      // Integrity failures get a precise message; ordinary failures keep the
      // existing transfer toast (the queue row remains the source of truth).
      if (event.state === "failedVerification") {
        toasts.error(`Integrity verification failed: ${event.name}`);
      } else if (event.state === "failed" && event.error) {
        toasts.error(`Transfer failed: ${event.name} — ${event.error}`);
      }
      break;
    case "progressBatch":
      transfers.setProgress(event.items);
      break;
    case "conflict":
      conflicts.add(event);
      break;
  }
}

/**
 * Subscribe to backend session + transfer events for the app's lifetime.
 *
 * @returns a promise that resolves once both subscriptions are registered.
 */
export function initSessionEvents(): Promise<void> {
  void subscribeTransferEvents(routeTransferEvent);
  return subscribeSessionEvents(routeSessionEvent);
}
