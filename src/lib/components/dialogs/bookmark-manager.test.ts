// bookmark-manager.test.ts — Component tests for the bookmark list/CRUD.

import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import type { Bookmark } from "$lib/ipc/commands";

// Mock the IPC command wrappers used by the bookmarks store.
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

import BookmarkManager from "./BookmarkManager.svelte";
import { bookmarks } from "$lib/stores/bookmarks.svelte";

/** A password bookmark with overridable fields. */
function bm(over: Partial<Bookmark> = {}): Bookmark {
  return {
    id: "1",
    name: "prod",
    host: "example.com",
    port: 22,
    username: "deploy",
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
  listMock.mockResolvedValueOnce([bm()]);
  await bookmarks.load();
});

describe("BookmarkManager", () => {
  it("renders a row and connects on the Connect button", async () => {
    const onConnect = vi.fn();
    render(BookmarkManager, { props: { onConnect, onEdit: vi.fn(), onAdd: vi.fn() } });

    expect(screen.getByText("prod")).toBeInTheDocument();
    expect(screen.getByText("deploy@example.com:22")).toBeInTheDocument();

    await fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    expect(onConnect).toHaveBeenCalledWith(expect.objectContaining({ id: "1" }));
  });

  it("invokes onAdd and onEdit", async () => {
    const onAdd = vi.fn();
    const onEdit = vi.fn();
    render(BookmarkManager, { props: { onConnect: vi.fn(), onEdit, onAdd } });

    await fireEvent.click(screen.getByRole("button", { name: "Add…" }));
    expect(onAdd).toHaveBeenCalled();

    await fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    expect(onEdit).toHaveBeenCalledWith(expect.objectContaining({ id: "1" }));
  });

  it("deletes after confirmation", async () => {
    vi.spyOn(window, "confirm").mockReturnValue(true);
    deleteMock.mockResolvedValueOnce(undefined);
    render(BookmarkManager, { props: { onConnect: vi.fn(), onEdit: vi.fn(), onAdd: vi.fn() } });

    await fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(deleteMock).toHaveBeenCalledWith("1");
  });

  it("shows an empty state when there are no bookmarks", async () => {
    listMock.mockResolvedValueOnce([]);
    await bookmarks.load();
    render(BookmarkManager, { props: { onConnect: vi.fn(), onEdit: vi.fn(), onAdd: vi.fn() } });
    expect(screen.getByText(/No saved connections/)).toBeInTheDocument();
  });
});
