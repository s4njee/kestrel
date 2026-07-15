// transfers.test.ts — Tests for the transfers store and TransferRow rendering.

import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/svelte";
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

  it("seeds, updates state and progress, and counts active", () => {
    transfers.add({ id: "a", direction: "download", name: "a.bin", size: 100 });
    expect(transfers.activeCount).toBe(1);

    transfers.setProgress([{ id: "a", bytes: 50, rateBps: 500 }]);
    expect(transfers.list[0].bytes).toBe(50);

    transfers.setState("a", "done", null);
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

  it("shows Failed for a failed transfer and no cancel button", () => {
    render(TransferRow, {
      props: { transfer: transfer({ state: "failed", error: "denied" }), onCancel: vi.fn() },
    });
    expect(screen.getByText("Failed")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
  });

  it("renders a done transfer without a cancel button", () => {
    render(TransferRow, {
      props: { transfer: transfer({ state: "done", bytes: 1024 }), onCancel: vi.fn() },
    });
    expect(screen.getByText("done")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
  });
});
