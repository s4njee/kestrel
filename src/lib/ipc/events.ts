// events.ts — Route backend session events into stores.
//
// `initSessionEvents` subscribes once at app start; each event is dispatched by
// `routeSessionEvent` into the relevant runes store. Kept separate and exported
// so it can be unit-tested without a live Tauri channel.

import {
  subscribeSessionEvents,
  subscribeTransferEvents,
  type SessionEvent,
  type TransferEvent,
} from "./commands";
import { sessions } from "$lib/stores/sessions.svelte";
import { prompts } from "$lib/stores/prompts.svelte";
import { transfers } from "$lib/stores/transfers.svelte";

/**
 * Dispatch a single session event into the stores.
 *
 * @param event - the event received from the backend channel.
 */
export function routeSessionEvent(event: SessionEvent): void {
  switch (event.type) {
    case "connectionState":
      sessions.setConnectionState(event.sessionId, event.state);
      break;
    case "hostKeyPrompt":
      prompts.setHostKeyPrompt(event);
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
      break;
    case "progressBatch":
      transfers.setProgress(event.items);
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
