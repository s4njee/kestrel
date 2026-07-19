// health.svelte.ts — Connection round-trip samples for the topbar HUD.
//
// Holds a short ring of latency samples per session, fed by routed
// `latencySample` events (ipc/events.ts). The Toolbar renders the active
// session's ring as an ASCII sparkline plus the latest value, tinted by
// threshold. Pure helpers (sparkline, level) live here so they are unit-testable
// without a component.

import { SvelteMap } from "svelte/reactivity";

/** How many samples the sparkline keeps per session. */
const RING = 12;

/** Latency bands for the HUD tint. */
export type LatencyLevel = "good" | "warn" | "bad";

/** Sparkline glyphs, shortest to tallest. */
const BARS = "▁▂▃▄▅▆▇█";

/**
 * Classify a round-trip time into a HUD band.
 *
 * @param rttMs - the round trip in milliseconds.
 * @returns "good" under 80ms, "warn" under 250ms, else "bad".
 */
export function latencyLevel(rttMs: number): LatencyLevel {
  if (rttMs < 80) return "good";
  if (rttMs < 250) return "warn";
  return "bad";
}

/**
 * Render samples as an ASCII sparkline.
 *
 * Bars scale to the window's own maximum (min 1ms so an all-zero ring still
 * draws), so the shape shows relative variation rather than an absolute scale —
 * what you want for "did it just spike".
 *
 * @param samples - round trips in milliseconds, oldest first.
 * @returns one glyph per sample ("" when empty).
 */
export function sparkline(samples: number[]): string {
  if (samples.length === 0) return "";
  const top = Math.max(1, ...samples);
  return samples
    .map((s) => BARS[Math.min(BARS.length - 1, Math.floor((s / top) * (BARS.length - 1)))])
    .join("");
}

class HealthStore {
  /** Recent samples per session id, oldest first. */
  #rings = $state<SvelteMap<string, number[]>>(new SvelteMap());

  /**
   * Record a latency sample for a session.
   *
   * @param sessionId - the session the sample belongs to.
   * @param rttMs - the measured round trip in milliseconds.
   */
  record(sessionId: string, rttMs: number): void {
    const ring = [...(this.#rings.get(sessionId) ?? []), rttMs];
    this.#rings.set(sessionId, ring.slice(-RING));
  }

  /**
   * The recent samples for a session, oldest first.
   *
   * @param sessionId - the session to read.
   * @returns up to the last 12 samples (empty when none).
   */
  samples(sessionId: string): number[] {
    return this.#rings.get(sessionId) ?? [];
  }

  /**
   * The most recent sample for a session.
   *
   * @param sessionId - the session to read.
   * @returns the latest round trip in ms, or null before the first sample.
   */
  latest(sessionId: string): number | null {
    return this.samples(sessionId).at(-1) ?? null;
  }

  /**
   * Drop a session's samples (on disconnect).
   *
   * @param sessionId - the session to forget.
   */
  forget(sessionId: string): void {
    this.#rings.delete(sessionId);
  }
}

/** Application-wide health store singleton. */
export const health = new HealthStore();
