// FileTable.svelte.test.ts — Component tests for sorting, selection, and the
// folder-disclosure wiring.

import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import { SvelteSet } from "svelte/reactivity";
import FileTable from "./FileTable.svelte";
import type { PaneRow } from "$lib/stores/panes.svelte";
import type { DirEntry } from "$lib/ipc/commands";

function entry(name: string, kind: DirEntry["kind"] = "file", path?: string): DirEntry {
  return {
    name,
    path: path ?? `/${name}`,
    kind,
    size: 10,
    mtime: 0,
    permissions: 0o644,
    linkTarget: null,
  };
}

function row(e: DirEntry, depth = 0, expanded = false): PaneRow {
  return { entry: e, depth, expanded, loading: false };
}

function baseProps() {
  return {
    rows: [row(entry("a.txt")), row(entry("b.txt")), row(entry("c.txt"))],
    paneKind: "remote" as const,
    sortKey: "name" as const,
    sortAsc: true,
    selected: new SvelteSet<string>(),
    onSort: vi.fn(),
    onSelect: vi.fn(),
    onOpen: vi.fn(),
    onToggleExpand: vi.fn(),
  };
}

describe("FileTable", () => {
  it("renders rows and calls onSort when a header is clicked", async () => {
    const props = baseProps();
    render(FileTable, { props });
    expect(screen.getByText("a.txt")).toBeInTheDocument();

    await fireEvent.click(screen.getByRole("button", { name: /Size/ }));
    expect(props.onSort).toHaveBeenCalledWith("size");
  });

  it("calls onSelect with the entry PATH and modifiers on row click", async () => {
    const props = baseProps();
    render(FileTable, { props });
    await fireEvent.click(screen.getByText("b.txt"), { ctrlKey: true });
    expect(props.onSelect).toHaveBeenCalledWith("/b.txt", { ctrl: true, shift: false });
  });

  it("calls onOpen on double click", async () => {
    const props = baseProps();
    render(FileTable, { props });
    await fireEvent.dblClick(screen.getByText("c.txt"));
    expect(props.onOpen).toHaveBeenCalledWith(expect.objectContaining({ name: "c.txt" }));
  });

  it("toggles expansion when a directory's disclosure glyph is clicked", async () => {
    const props = baseProps();
    const dir = entry("src", "dir");
    props.rows = [row(dir)];
    const { container } = render(FileTable, { props });

    const twisty = container.querySelector(".twisty") as HTMLElement;
    expect(twisty).toBeTruthy();
    expect(twisty.textContent).toBe("▸");

    await fireEvent.click(twisty, { detail: 1 });
    expect(props.onToggleExpand).toHaveBeenCalledWith(expect.objectContaining({ name: "src" }));
    // The label is one target, so it selects too.
    expect(props.onSelect).toHaveBeenCalledWith("/src", { ctrl: false, shift: false });
  });

  it("toggles expansion when a directory's NAME is clicked, not just the arrow", async () => {
    const props = baseProps();
    props.rows = [row(entry("src", "dir"))];
    render(FileTable, { props });

    // Click the name text itself — the whole label is the hit area.
    await fireEvent.click(screen.getByText("src"), { detail: 1 });
    expect(props.onToggleExpand).toHaveBeenCalledWith(expect.objectContaining({ name: "src" }));
  });

  it("does not expand on a modifier click (so range-select does not flap folders)", async () => {
    const props = baseProps();
    props.rows = [row(entry("src", "dir"))];
    render(FileTable, { props });

    await fireEvent.click(screen.getByText("src"), { detail: 1, ctrlKey: true });
    expect(props.onToggleExpand).not.toHaveBeenCalled();
    expect(props.onSelect).toHaveBeenCalledWith("/src", { ctrl: true, shift: false });

    await fireEvent.click(screen.getByText("src"), { detail: 1, shiftKey: true });
    expect(props.onToggleExpand).not.toHaveBeenCalled();
  });

  it("does not expand on the second click of a double-click (navigate wins)", async () => {
    const props = baseProps();
    props.rows = [row(entry("src", "dir"))];
    render(FileTable, { props });

    // detail=2 marks the second click of a double-click.
    await fireEvent.click(screen.getByText("src"), { detail: 2 });
    expect(props.onToggleExpand).not.toHaveBeenCalled();
  });

  it("clicking a non-label column selects without expanding", async () => {
    const props = baseProps();
    const dir = entry("src", "dir");
    props.rows = [row(dir)];
    const { container } = render(FileTable, { props });

    // Scope to the row: the column header also carries a .col-perms class.
    await fireEvent.click(container.querySelector(".row .col-perms") as HTMLElement, { detail: 1 });
    expect(props.onSelect).toHaveBeenCalledWith("/src", { ctrl: false, shift: false });
    expect(props.onToggleExpand).not.toHaveBeenCalled();
  });

  it("shows an expanded directory's glyph and indents its children", () => {
    const props = baseProps();
    const dir = entry("src", "dir");
    const child = entry("main.rs", "file", "/src/main.rs");
    props.rows = [row(dir, 0, true), row(child, 1)];
    const { container } = render(FileTable, { props });

    expect((container.querySelector(".twisty") as HTMLElement).textContent).toBe("▾");
    const childRow = container.querySelector('[data-depth="1"]') as HTMLElement;
    expect(childRow).toBeTruthy();
    expect(childRow.textContent).toContain("main.rs");
    // Depth drives the indent.
    const nameCell = childRow.querySelector(".col-name") as HTMLElement;
    expect(nameCell.style.paddingLeft).toBe("14px");
  });

  it("selects (does not expand) when a file's label is clicked", async () => {
    const props = baseProps();
    const { container } = render(FileTable, { props });
    const glyph = container.querySelector(".icon") as HTMLElement;
    await fireEvent.click(glyph, { detail: 1 });
    expect(props.onToggleExpand).not.toHaveBeenCalled();
    expect(props.onSelect).toHaveBeenCalled();
  });
});
