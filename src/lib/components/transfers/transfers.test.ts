// transfers.test.ts — Tests for the transfers store and TransferRow rendering.

import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import TransferRow from "./TransferRow.svelte";
import { transfers } from "$lib/stores/transfers.svelte";
import type { Transfer } from "$lib/stores/transfers.svelte";

function transfer(overrides: Partial<Transfer> = {}): Transfer {
  return {
    id: "t1",
    direction: "download",
    name: "file.bin",
    state: "running",
    bytes: 512,
    size: 1024,
    rateBps: 2048,
    error: null,
    ...overrides,
  };
}

describe("transfers store", () => {
  beforeEach(() => transfers.clearCompleted());

  it("upserts from state events, updates progress, and counts active", () => {
    transfers.applyState({
      id: "a",
      state: "running",
      name: "a.bin",
      size: 100,
      bytes: 0,
      direction: "download",
      error: null,
    });
    expect(transfers.activeCount).toBe(1);
    expect(transfers.list[0].name).toBe("a.bin");

    transfers.setProgress([{ id: "a", bytes: 50, rateBps: 500 }]);
    expect(transfers.list[0].bytes).toBe(50);

    transfers.applyState({
      id: "a",
      state: "done",
      name: "a.bin",
      size: 100,
      bytes: 100,
      direction: "download",
      error: null,
    });
    expect(transfers.activeCount).toBe(0);

    transfers.clearCompleted();
    expect(transfers.list).toHaveLength(0);
  });
});

describe("TransferRow", () => {
  it("shows rate and a cancel button while running", () => {
    const onCancel = vi.fn();
    const { container } = render(TransferRow, {
      props: { transfer: transfer({ state: "running" }), onCancel },
    });
    expect(container.textContent).toContain("/s");
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
  });

  it("shows failed for a failed transfer and no cancel button", () => {
    render(TransferRow, {
      props: { transfer: transfer({ state: "failed", error: "denied" }), onCancel: vi.fn() },
    });
    expect(screen.getByText("failed")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
  });

  it("distinguishes an integrity verification failure", () => {
    render(TransferRow, {
      props: {
        transfer: transfer({
          state: "failedVerification",
          error: "local and remote checksums differ",
        }),
        onCancel: vi.fn(),
      },
    });
    expect(screen.getByText("verification failed")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
  });

  it("renders a done transfer at 100% without a cancel button", () => {
    const { container } = render(TransferRow, {
      props: { transfer: transfer({ state: "done", bytes: 1024 }), onCancel: vi.fn() },
    });
    expect(container.textContent).toContain("100%");
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
  });

  it("shows a resume button (not pause) when paused", async () => {
    const onResume = vi.fn();
    render(TransferRow, {
      props: { transfer: transfer({ state: "paused" }), onCancel: vi.fn(), onResume },
    });
    expect(screen.queryByRole("button", { name: "Pause" })).toBeNull();
    const resume = screen.getByRole("button", { name: "Resume" });
    await fireEvent.click(resume);
    expect(onResume).toHaveBeenCalledWith("t1");
  });
});
