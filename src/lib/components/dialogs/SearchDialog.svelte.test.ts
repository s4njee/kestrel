// SearchDialog.svelte.test.ts — Component tests for the remote-search dialog
// (E8-S7): explicit search, hit selection, context-sensitive Escape, and the
// honesty of the result footer.

import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import SearchDialog from "./SearchDialog.svelte";
import type { SearchState } from "$lib/stores/search.svelte";
import type { SearchResult } from "$lib/ipc/commands";

/**
 * Build a search result.
 *
 * @param names - base names of the hits.
 * @param over - overrides for strategy/truncated.
 * @returns a SearchResult.
 */
function result(names: string[], over: Partial<SearchResult> = {}): SearchResult {
  return {
    hits: names.map((n) => ({ name: n, path: `/srv/app/${n}` })),
    strategy: "exec",
    truncated: false,
    ...over,
  };
}

/**
 * Render the dialog with a given store state.
 *
 * @param state - overrides for the search state.
 * @returns the props, whose callbacks are spies.
 */
function renderDialog(state: Partial<SearchState> = {}) {
  const props = {
    root: "/srv/app",
    search: { running: false, query: "", result: null, error: null, ...state },
    onSearch: vi.fn(),
    onCancel: vi.fn(),
    onOpen: vi.fn(),
    onClose: vi.fn(),
  };
  render(SearchDialog, { props });
  return props;
}

/**
 * The dialog's query input.
 *
 * @returns the input element.
 */
function input(): HTMLInputElement {
  return screen.getByLabelText("Search query") as HTMLInputElement;
}

describe("SearchDialog", () => {
  it("shows the root it will search and does not search while typing", async () => {
    const props = renderDialog();
    expect(screen.getByTitle("/srv/app")).toBeInTheDocument();
    await fireEvent.input(input(), { target: { value: "notes" } });
    // Search is a round-trip, so it must wait for Enter, unlike the `/` filter.
    expect(props.onSearch).not.toHaveBeenCalled();
  });

  it("runs the trimmed query on Enter", async () => {
    const props = renderDialog();
    await fireEvent.input(input(), { target: { value: "  notes  " } });
    await fireEvent.keyDown(input(), { key: "Enter" });
    expect(props.onSearch).toHaveBeenCalledWith("notes");
  });

  it("does not run an empty query", async () => {
    const props = renderDialog();
    await fireEvent.keyDown(input(), { key: "Enter" });
    await fireEvent.input(input(), { target: { value: "   " } });
    await fireEvent.keyDown(input(), { key: "Enter" });
    expect(props.onSearch).not.toHaveBeenCalled();
  });

  it("lists hits and opens the one that is clicked", async () => {
    const props = renderDialog({ query: "notes", result: result(["a.txt", "b.txt"]) });
    await fireEvent.click(screen.getByText("b.txt"));
    expect(props.onOpen).toHaveBeenCalledWith(expect.objectContaining({ path: "/srv/app/b.txt" }));
    expect(props.onClose).toHaveBeenCalled();
  });

  it("moves the highlight with the arrow keys and opens it with Enter", async () => {
    const props = renderDialog({ query: "notes", result: result(["a.txt", "b.txt"]) });
    await fireEvent.input(input(), { target: { value: "notes" } });
    await fireEvent.keyDown(input(), { key: "ArrowDown" });
    await fireEvent.keyDown(input(), { key: "Enter" });
    expect(props.onOpen).toHaveBeenCalledWith(expect.objectContaining({ path: "/srv/app/b.txt" }));
  });

  it("re-runs rather than opening when the query has been edited since", async () => {
    const props = renderDialog({ query: "notes", result: result(["a.txt"]) });
    await fireEvent.input(input(), { target: { value: "other" } });
    await fireEvent.keyDown(input(), { key: "Enter" });
    expect(props.onOpen).not.toHaveBeenCalled();
    expect(props.onSearch).toHaveBeenCalledWith("other");
  });

  it("Escape cancels a running search instead of closing the dialog", async () => {
    const props = renderDialog({ running: true });
    await fireEvent.keyDown(input(), { key: "Escape" });
    expect(props.onCancel).toHaveBeenCalled();
    expect(props.onClose).not.toHaveBeenCalled();
  });

  it("Escape closes once nothing is in flight", async () => {
    const props = renderDialog({ result: result([]) });
    await fireEvent.keyDown(input(), { key: "Escape" });
    expect(props.onClose).toHaveBeenCalled();
    expect(props.onCancel).not.toHaveBeenCalled();
  });

  it("says a result was truncated rather than passing it off as complete", () => {
    renderDialog({ query: "a", result: result(["a.txt"], { truncated: true }) });
    expect(screen.getByText(/truncated/i)).toBeInTheDocument();
  });

  it("discloses when the slow fallback strategy was used", () => {
    renderDialog({ query: "a", result: result(["a.txt"], { strategy: "walk" }) });
    expect(screen.getByText(/no find/i)).toBeInTheDocument();
  });

  it("stays quiet about the strategy on the fast path", () => {
    renderDialog({ query: "a", result: result(["a.txt"], { strategy: "exec" }) });
    expect(screen.queryByText(/no find/i)).not.toBeInTheDocument();
  });

  it("distinguishes an empty result from not having searched yet", () => {
    const { unmount } = render(SearchDialog, {
      props: {
        root: "/srv",
        search: { running: false, query: "", result: null, error: null },
        onSearch: vi.fn(),
        onCancel: vi.fn(),
        onOpen: vi.fn(),
        onClose: vi.fn(),
      },
    });
    expect(screen.getByText(/press return/i)).toBeInTheDocument();
    unmount();

    renderDialog({ query: "zzz", result: result([]) });
    expect(screen.getByText(/no matches/i)).toBeInTheDocument();
  });

  it("surfaces an error as an alert", () => {
    renderDialog({ error: "no such session" });
    expect(screen.getByRole("alert")).toHaveTextContent("no such session");
  });
});
