// file-pane-filter.test.ts — Component tests for the pane filter bar (E8-S13).

import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import FilePane from "./FilePane.svelte";
import { PaneStore } from "$lib/stores/panes.svelte";
import type { DirEntry } from "$lib/ipc/commands";

function entry(name: string, kind: DirEntry["kind"] = "file"): DirEntry {
  return {
    name,
    path: `/${name}`,
    kind,
    size: 10,
    mtime: 0,
    permissions: 0o644,
    linkTarget: null,
  };
}

function pane(): PaneStore {
  const p = new PaneStore("remote");
  p.startLoad("/");
  p.setEntries([entry("alpha.txt"), entry("beta.log"), entry("gamma.txt")]);
  return p;
}

function props(p: PaneStore) {
  return {
    pane: p,
    active: true,
    onActivate: vi.fn(),
    onNavigate: vi.fn(),
  };
}

describe("FilePane filter bar", () => {
  it("is hidden until the filter is opened", async () => {
    const p = pane();
    render(FilePane, { props: props(p) });
    expect(screen.queryByLabelText("Filter remote rows")).toBeNull();

    // Opening it (what the `/` shortcut does) reveals the bar. The store change
    // is outside an event handler, so wait for Svelte to flush before asserting.
    p.setFilter("");
    await tick();
    expect(screen.getByLabelText("Filter remote rows")).toBeInTheDocument();
  });

  it("narrows the rendered rows as the query changes", async () => {
    const p = pane();
    p.setFilter("");
    render(FilePane, { props: props(p) });

    expect(screen.getByText("beta.log")).toBeInTheDocument();
    await fireEvent.input(screen.getByLabelText("Filter remote rows"), {
      target: { value: "txt" },
    });

    expect(screen.queryByText("beta.log")).toBeNull();
    expect(screen.getByText("alpha.txt")).toBeInTheDocument();
    expect(screen.getByText("gamma.txt")).toBeInTheDocument();
  });

  it("shows a live match count", async () => {
    const p = pane();
    p.setFilter("");
    render(FilePane, { props: props(p) });
    expect(screen.getByText("3 matches")).toBeInTheDocument();

    await fireEvent.input(screen.getByLabelText("Filter remote rows"), {
      target: { value: "beta" },
    });
    expect(screen.getByText("1 match")).toBeInTheDocument();
  });

  it("Escape clears and closes the bar", async () => {
    const p = pane();
    p.setFilter("beta");
    render(FilePane, { props: props(p) });

    await fireEvent.keyDown(screen.getByLabelText("Filter remote rows"), { key: "Escape" });
    expect(p.filter).toBeNull();
    expect(screen.queryByLabelText("Filter remote rows")).toBeNull();
    // All rows are back.
    expect(screen.getByText("alpha.txt")).toBeInTheDocument();
  });

  it("Enter keeps the filter applied (only moves focus)", async () => {
    const p = pane();
    p.setFilter("beta");
    render(FilePane, { props: props(p) });

    await fireEvent.keyDown(screen.getByLabelText("Filter remote rows"), { key: "Enter" });
    expect(p.filter).toBe("beta");
    expect(screen.getByLabelText("Filter remote rows")).toBeInTheDocument();
  });

  it("the clear button closes the bar", async () => {
    const p = pane();
    p.setFilter("beta");
    render(FilePane, { props: props(p) });

    await fireEvent.click(screen.getByLabelText("Clear filter"));
    expect(p.filter).toBeNull();
    expect(screen.queryByLabelText("Filter remote rows")).toBeNull();
  });
});
