// FileTable.svelte.test.ts — Component tests for sorting and selection wiring.

import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import { SvelteSet } from "svelte/reactivity";
import FileTable from "./FileTable.svelte";
import type { DirEntry } from "$lib/ipc/commands";

function entry(name: string, kind: DirEntry["kind"] = "file"): DirEntry {
  return { name, path: `/${name}`, kind, size: 10, mtime: 0, permissions: 0o644, linkTarget: null };
}

function baseProps() {
  return {
    entries: [entry("a.txt"), entry("b.txt"), entry("c.txt")],
    paneKind: "remote" as const,
    sortKey: "name" as const,
    sortAsc: true,
    selected: new SvelteSet<string>(),
    onSort: vi.fn(),
    onSelect: vi.fn(),
    onOpen: vi.fn(),
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

  it("calls onSelect with modifiers on row click", async () => {
    const props = baseProps();
    render(FileTable, { props });
    await fireEvent.click(screen.getByText("b.txt"), { ctrlKey: true });
    expect(props.onSelect).toHaveBeenCalledWith("b.txt", { ctrl: true, shift: false });
  });

  it("calls onOpen on double click", async () => {
    const props = baseProps();
    render(FileTable, { props });
    await fireEvent.dblClick(screen.getByText("c.txt"));
    expect(props.onOpen).toHaveBeenCalledWith(expect.objectContaining({ name: "c.txt" }));
  });
});
