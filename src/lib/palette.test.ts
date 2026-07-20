// palette.test.ts — Tests for the command-palette model (matching, ranking,
// inventory completeness).

import { describe, it, expect, vi } from "vitest";
import { buildCommands, filterCommands, fuzzyMatch, type PaletteDeps } from "./palette";
import type { ShortcutAction } from "./keymap";
import type { Bookmark } from "$lib/ipc/commands";

function bookmark(over: Partial<Bookmark> = {}): Bookmark {
  return {
    id: "b1",
    name: "prod",
    host: "example.com",
    port: 22,
    username: "deploy",
    authMethod: "password",
    keyPath: null,
    remoteDir: null,
    localDir: null,
    hasSavedSecret: true,
    ...over,
  };
}

function deps(over: Partial<PaletteDeps> = {}): PaletteDeps {
  return {
    actions: {
      refresh: vi.fn(),
      focusPath: vi.fn(),
      download: vi.fn(),
      upload: vi.fn(),
      rename: vi.fn(),
      delete: vi.fn(),
      switchPane: vi.fn(),
      palette: vi.fn(),
      filter: vi.fn(),
      search: vi.fn(),
    },
    connected: true,
    canUpload: true,
    canDownload: true,
    onConnect: vi.fn(),
    onSettings: vi.fn(),
    onQueue: vi.fn(),
    onNewFolder: vi.fn(),
    bookmarks: [],
    onConnectBookmark: vi.fn(),
    canSearch: true,
    onSearch: vi.fn(),
    diffMode: false,
    onToggleDiff: vi.fn(),
    onTransferDifferences: vi.fn(),
    ...over,
  };
}

describe("fuzzyMatch", () => {
  it("matches subsequences case-insensitively and rejects non-subsequences", () => {
    expect(fuzzyMatch("rfp", "refresh active pane")).not.toBeNull();
    expect(fuzzyMatch("REFRESH", "refresh active pane")).not.toBeNull();
    expect(fuzzyMatch("xyz", "refresh active pane")).toBeNull();
  });

  it("returns 0 for the empty query", () => {
    expect(fuzzyMatch("", "anything")).toBe(0);
  });

  it("ranks a word-start match above a scattered one", () => {
    const start = fuzzyMatch("set", "settings…");
    const scattered = fuzzyMatch("set", "switch pane extra t");
    expect(start).not.toBeNull();
    expect(scattered).not.toBeNull();
    expect(start!).toBeGreaterThan(scattered!);
  });
});

describe("filterCommands", () => {
  const inventory = buildCommands(deps());

  it("returns everything in canonical order for an empty query", () => {
    expect(filterCommands(inventory, "")).toEqual(inventory);
    expect(filterCommands(inventory, "   ")).toEqual(inventory);
  });

  it("filters to matches and puts the best first", () => {
    const out = filterCommands(inventory, "settings");
    expect(out.length).toBeGreaterThan(0);
    expect(out[0].id).toBe("settings");
  });

  it("drops non-matching commands", () => {
    const out = filterCommands(inventory, "zzzz");
    expect(out).toEqual([]);
  });
});

describe("buildCommands", () => {
  it("covers every ShortcutAction except the palette itself", () => {
    const ids = new Set(buildCommands(deps()).map((c) => c.id));
    // Total record: this list is derived from the union, so a new action that
    // is not handled here fails to compile in deps() above.
    const expected: ShortcutAction[] = [
      "refresh",
      "focusPath",
      "download",
      "upload",
      "rename",
      "delete",
      "switchPane",
      "filter",
      "search",
    ];
    for (const action of expected) {
      expect(ids.has(action), `missing palette entry for ${action}`).toBe(true);
    }
    expect(ids.has("palette")).toBe(false);
  });

  it("omits transfer commands when they cannot run", () => {
    const ids = new Set(
      buildCommands(deps({ canUpload: false, canDownload: false })).map((c) => c.id),
    );
    expect(ids.has("upload")).toBe(false);
    expect(ids.has("download")).toBe(false);
  });

  it("flips the connect label with connection state", () => {
    const disconnected = buildCommands(deps({ connected: false }));
    const connected = buildCommands(deps({ connected: true }));
    expect(disconnected.find((c) => c.id === "connect")?.label).toContain("connect");
    expect(connected.find((c) => c.id === "connect")?.label).toContain("disconnect");
  });

  it("lists each bookmark as a connect command that dispatches it", () => {
    const onConnectBookmark = vi.fn();
    const b = bookmark({ name: "staging", host: "stage.example.com" });
    const commands = buildCommands(deps({ bookmarks: [b], onConnectBookmark }));
    const entry = commands.find((c) => c.id === "bookmark:b1");
    expect(entry).toBeDefined();
    expect(entry!.label).toBe("connect: staging");
    expect(entry!.hint).toBe("deploy@stage.example.com");
    entry!.run();
    expect(onConnectBookmark).toHaveBeenCalledWith(b);
  });

  it("omits search when there is no remote pane to search", () => {
    const ids = new Set(buildCommands(deps({ canSearch: false })).map((c) => c.id));
    expect(ids.has("search")).toBe(false);
  });

  it("offers diff mode only while connected", () => {
    const offline = new Set(buildCommands(deps({ connected: false })).map((c) => c.id));
    const online = new Set(buildCommands(deps({ connected: true })).map((c) => c.id));
    expect(offline.has("diff")).toBe(false);
    expect(online.has("diff")).toBe(true);
  });

  it("offers 'transfer differences' only once the marks are on screen", () => {
    const off = buildCommands(deps({ diffMode: false }));
    const on = buildCommands(deps({ diffMode: true }));
    expect(off.some((c) => c.id === "transferDifferences")).toBe(false);
    expect(on.some((c) => c.id === "transferDifferences")).toBe(true);
    // The toggle's own label flips so the palette never offers "compare" twice.
    expect(off.find((c) => c.id === "diff")!.label).toContain("compare");
    expect(on.find((c) => c.id === "diff")!.label).toContain("hide");
  });

  it("wires each action id to its handler", () => {
    const d = deps();
    const commands = buildCommands(d);
    commands.find((c) => c.id === "refresh")!.run();
    expect(d.actions.refresh).toHaveBeenCalled();
    commands.find((c) => c.id === "delete")!.run();
    expect(d.actions.delete).toHaveBeenCalled();
  });
});
