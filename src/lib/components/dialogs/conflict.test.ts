// conflict.test.ts — Component test for the conflict resolution dialog.

import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";

const { resolveConflictMock } = vi.hoisted(() => ({ resolveConflictMock: vi.fn() }));
vi.mock("$lib/ipc/commands", () => ({
  resolveConflict: (...a: unknown[]) => resolveConflictMock(...a),
}));

import ConflictDialog from "./ConflictDialog.svelte";
import { conflicts } from "$lib/stores/conflicts.svelte";

beforeEach(() => {
  resolveConflictMock.mockReset();
  resolveConflictMock.mockResolvedValue(undefined);
  conflicts.clear();
});

function pushConflict(id = "t1") {
  conflicts.add({
    type: "conflict",
    id,
    dest: "/local/out.bin",
    existingSize: 100,
    existingMtime: 1,
    incomingSize: 200,
    incomingMtime: 2,
  });
}

describe("ConflictDialog", () => {
  it("shows the destination and resolves with overwrite", async () => {
    pushConflict();
    render(ConflictDialog);
    expect(screen.getByText("/local/out.bin")).toBeInTheDocument();

    await fireEvent.click(screen.getByRole("button", { name: "Overwrite" }));
    expect(resolveConflictMock).toHaveBeenCalledWith("t1", "overwrite", false);
    // The conflict is removed from the queue.
    expect(conflicts.current).toBeNull();
  });

  it("passes apply-to-all and clears the queue", async () => {
    pushConflict("a");
    conflicts.add({
      type: "conflict",
      id: "b",
      dest: "/local/b.bin",
      existingSize: 1,
      existingMtime: null,
      incomingSize: 1,
      incomingMtime: null,
    });
    render(ConflictDialog);

    await fireEvent.click(screen.getByRole("checkbox"));
    await fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    expect(resolveConflictMock).toHaveBeenCalledWith("a", "skip", true);
    expect(conflicts.count).toBe(0);
  });
});
