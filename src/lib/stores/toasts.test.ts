// toasts.test.ts — Tests for the toasts runes store.

import { describe, it, expect, beforeEach } from "vitest";
import { toasts } from "./toasts.svelte";

beforeEach(() => {
  // Clear any leftover toasts between tests.
  for (const t of [...toasts.items]) toasts.dismiss(t.id);
});

describe("toasts store", () => {
  it("shows and returns distinct ids", () => {
    const a = toasts.show("one", "error", 0);
    const b = toasts.show("two", "info", 0);
    expect(a).not.toBe(b);
    expect(toasts.items.map((t) => t.message)).toEqual(["one", "two"]);
    expect(toasts.items[1].kind).toBe("info");
  });

  it("dismisses by id", () => {
    const id = toasts.show("bye", "error", 0);
    toasts.dismiss(id);
    expect(toasts.items).toHaveLength(0);
    // Dismissing again is a no-op.
    toasts.dismiss(id);
    expect(toasts.items).toHaveLength(0);
  });

  it("error and info are shorthands", () => {
    toasts.error("boom");
    toasts.info("fyi");
    expect(toasts.items.map((t) => t.kind)).toEqual(["error", "info"]);
  });
});
