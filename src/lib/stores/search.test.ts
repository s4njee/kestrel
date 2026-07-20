// search.test.ts — Store tests for remote search (E8-S7).
//
// The IPC boundary is mocked, so these cover the store's own contract: one
// search at a time, stale results discarded, and cancellation treated as an
// outcome rather than an error.

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SearchResult } from "$lib/ipc/commands";

const searchRemote = vi.fn();
const cancelSearch = vi.fn();

vi.mock("$lib/ipc/commands", () => ({
  searchRemote: (...args: unknown[]) => searchRemote(...args),
  cancelSearch: (...args: unknown[]) => cancelSearch(...args),
}));

const { search } = await import("./search.svelte");

/**
 * Build a search result.
 *
 * @param names - base names of the hits.
 * @param over - overrides for strategy/truncated.
 * @returns a SearchResult.
 */
function result(names: string[], over: Partial<SearchResult> = {}): SearchResult {
  return {
    hits: names.map((n) => ({ name: n, path: `/srv/${n}` })),
    strategy: "exec",
    truncated: false,
    ...over,
  };
}

/**
 * A promise plus its resolve/reject handles, for controlling call ordering.
 *
 * @returns the promise and its settlers.
 */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

beforeEach(() => {
  searchRemote.mockReset();
  cancelSearch.mockReset();
  cancelSearch.mockResolvedValue(undefined);
  search.reset();
});

describe("search store", () => {
  it("runs a search and exposes its result", async () => {
    searchRemote.mockResolvedValue(result(["notes.txt"]));
    await search.run("s1", "/srv", "notes");
    expect(search.state.running).toBe(false);
    expect(search.state.query).toBe("notes");
    expect(search.state.result?.hits.map((h) => h.name)).toEqual(["notes.txt"]);
    expect(search.state.error).toBeNull();
  });

  it("passes a distinct id per search so each can be cancelled individually", async () => {
    searchRemote.mockResolvedValue(result([]));
    await search.run("s1", "/srv", "a");
    await search.run("s1", "/srv", "b");
    const [firstId, secondId] = searchRemote.mock.calls.map((c) => c[1]);
    expect(firstId).not.toBe(secondId);
  });

  it("reports a failure but never reports a cancellation as one", async () => {
    searchRemote.mockRejectedValue(new Error("no such session"));
    await search.run("s1", "/srv", "a");
    expect(search.state.error).toMatch(/no such session/);

    searchRemote.mockRejectedValue(new Error("canceled"));
    await search.run("s1", "/srv", "b");
    expect(search.state.error).toBeNull();
  });

  it("cancels the in-flight search when a new one starts", async () => {
    const first = deferred<SearchResult>();
    searchRemote.mockReturnValueOnce(first.promise);
    const running = search.run("s1", "/srv", "slow");
    expect(search.state.running).toBe(true);

    searchRemote.mockResolvedValueOnce(result(["fast.txt"]));
    await search.run("s1", "/srv", "fast");

    expect(cancelSearch).toHaveBeenCalledWith(searchRemote.mock.calls[0][1]);
    expect(search.state.result?.hits.map((h) => h.name)).toEqual(["fast.txt"]);

    // The superseded search now answers late — it must not clobber the newer
    // result, which is the bug a naive "last write wins" store would have.
    first.resolve(result(["slow.txt"]));
    await running;
    expect(search.state.result?.hits.map((h) => h.name)).toEqual(["fast.txt"]);
  });

  it("a superseded search's late failure does not surface as an error", async () => {
    const first = deferred<SearchResult>();
    searchRemote.mockReturnValueOnce(first.promise);
    const running = search.run("s1", "/srv", "slow");

    searchRemote.mockResolvedValueOnce(result(["fast.txt"]));
    await search.run("s1", "/srv", "fast");

    first.reject(new Error("connection lost"));
    await running;
    expect(search.state.error).toBeNull();
    expect(search.state.result?.hits.map((h) => h.name)).toEqual(["fast.txt"]);
  });

  it("cancel clears the running flag and is a no-op when nothing runs", async () => {
    const pending = deferred<SearchResult>();
    searchRemote.mockReturnValueOnce(pending.promise);
    const running = search.run("s1", "/srv", "slow");
    expect(search.state.running).toBe(true);

    await search.cancel();
    expect(search.state.running).toBe(false);
    expect(cancelSearch).toHaveBeenCalledTimes(1);

    await search.cancel();
    expect(cancelSearch).toHaveBeenCalledTimes(1);

    pending.resolve(result(["late.txt"]));
    await running;
    expect(search.state.result).toBeNull();
  });

  it("keeps the truncation flag and strategy so the UI can qualify the answer", async () => {
    searchRemote.mockResolvedValue(result(["a"], { truncated: true, strategy: "walk" }));
    await search.run("s1", "/srv", "a");
    expect(search.state.result?.truncated).toBe(true);
    expect(search.state.result?.strategy).toBe("walk");
  });

  it("reset clears results without touching a running search's bookkeeping", async () => {
    searchRemote.mockResolvedValue(result(["a"]));
    await search.run("s1", "/srv", "a");
    search.reset();
    expect(search.state.result).toBeNull();
    expect(search.state.query).toBe("");
    expect(search.state.error).toBeNull();
  });
});
