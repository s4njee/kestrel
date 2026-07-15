// bookmarks.test.ts — Tests for the bookmarks runes store.

import { describe, it, expect, vi, beforeEach } from "vitest";
import type { Bookmark } from "$lib/ipc/commands";

// Mock the IPC command wrappers the store depends on.
const { listMock, saveMock, deleteMock } = vi.hoisted(() => ({
  listMock: vi.fn(),
  saveMock: vi.fn(),
  deleteMock: vi.fn(),
}));
vi.mock("$lib/ipc/commands", () => ({
  listBookmarks: (...a: unknown[]) => listMock(...a),
  saveBookmark: (...a: unknown[]) => saveMock(...a),
  deleteBookmark: (...a: unknown[]) => deleteMock(...a),
}));

import { bookmarks } from "./bookmarks.svelte";

/** A password bookmark with overridable fields. */
function bm(over: Partial<Bookmark> = {}): Bookmark {
  return {
    id: "00000000-0000-0000-0000-000000000000",
    name: "srv",
    host: "example.com",
    port: 22,
    username: "u",
    authMethod: "password",
    keyPath: null,
    remoteDir: null,
    localDir: null,
    hasSavedSecret: false,
    ...over,
  };
}

beforeEach(async () => {
  listMock.mockReset();
  saveMock.mockReset();
  deleteMock.mockReset();
  // Start each test from a known-empty store.
  listMock.mockResolvedValueOnce([]);
  await bookmarks.load();
});

describe("bookmarks store", () => {
  it("load populates the list", async () => {
    listMock.mockResolvedValueOnce([bm({ id: "1", name: "a" }), bm({ id: "2", name: "b" })]);
    await bookmarks.load();
    expect(bookmarks.items.map((b) => b.name)).toEqual(["a", "b"]);
    expect(bookmarks.loaded).toBe(true);
  });

  it("save appends a new bookmark from the command result", async () => {
    const saved = bm({ id: "new-id", name: "fresh" });
    saveMock.mockResolvedValueOnce(saved);
    const result = await bookmarks.save(bm({ name: "fresh" }), "pw");
    expect(saveMock).toHaveBeenCalledWith(expect.objectContaining({ name: "fresh" }), "pw");
    expect(result.id).toBe("new-id");
    expect(bookmarks.items).toHaveLength(1);
    expect(bookmarks.items[0].id).toBe("new-id");
  });

  it("save replaces an existing bookmark matched by id", async () => {
    listMock.mockResolvedValueOnce([bm({ id: "1", name: "old" })]);
    await bookmarks.load();
    saveMock.mockResolvedValueOnce(bm({ id: "1", name: "renamed" }));
    await bookmarks.save(bm({ id: "1", name: "renamed" }));
    expect(bookmarks.items).toHaveLength(1);
    expect(bookmarks.items[0].name).toBe("renamed");
  });

  it("remove deletes via the command and drops it locally", async () => {
    listMock.mockResolvedValueOnce([bm({ id: "1" }), bm({ id: "2" })]);
    await bookmarks.load();
    deleteMock.mockResolvedValueOnce(undefined);
    await bookmarks.remove("1");
    expect(deleteMock).toHaveBeenCalledWith("1");
    expect(bookmarks.items.map((b) => b.id)).toEqual(["2"]);
  });
});
