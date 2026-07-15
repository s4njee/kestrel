// settings.test.ts — Tests for the settings runes store.

import { describe, it, expect, vi, beforeEach } from "vitest";
import type { Settings } from "$lib/ipc/commands";

const { getMock, saveMock } = vi.hoisted(() => ({
  getMock: vi.fn(),
  saveMock: vi.fn(),
}));
vi.mock("$lib/ipc/commands", () => ({
  getSettings: (...a: unknown[]) => getMock(...a),
  saveSettings: (...a: unknown[]) => saveMock(...a),
}));

import { settings } from "./settings.svelte";

function s(over: Partial<Settings> = {}): Settings {
  return {
    concurrency: 3,
    defaultConflict: "ask",
    defaultLocalDir: null,
    showHidden: false,
    ...over,
  };
}

beforeEach(() => {
  getMock.mockReset();
  saveMock.mockReset();
});

describe("settings store", () => {
  it("load populates the value and flags loaded", async () => {
    getMock.mockResolvedValueOnce(s({ concurrency: 5, showHidden: true }));
    await settings.load();
    expect(settings.loaded).toBe(true);
    expect(settings.value.concurrency).toBe(5);
    expect(settings.showHidden).toBe(true);
  });

  it("save stores the backend-normalized result", async () => {
    // Backend clamps concurrency to 8.
    saveMock.mockResolvedValueOnce(s({ concurrency: 8, defaultLocalDir: "/tmp" }));
    const result = await settings.save(s({ concurrency: 99, defaultLocalDir: "/tmp" }));
    expect(saveMock).toHaveBeenCalledWith(expect.objectContaining({ concurrency: 99 }));
    expect(result.concurrency).toBe(8);
    expect(settings.defaultLocalDir).toBe("/tmp");
  });
});
