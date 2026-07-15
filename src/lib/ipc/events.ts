// events.ts — Route backend session events into stores.
//
// `initSessionEvents` subscribes once at app start; each event is dispatched by
// `routeSessionEvent` into the relevant runes store. Kept separate and exported
// so it can be unit-tested without a live Tauri channel.

import { subscribeSessionEvents, type SessionEvent } from "./commands";
import { sessions } from "$lib/stores/sessions.svelte";
import { prompts } from "$lib/stores/prompts.svelte";

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
 * Subscribe to backend session events for the lifetime of the app.
 *
 * @returns a promise that resolves once the subscription is registered.
 */
export function initSessionEvents(): Promise<void> {
  return subscribeSessionEvents(routeSessionEvent);
}
