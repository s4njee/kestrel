// keymap.test.ts — Tests for global shortcut resolution.

import { describe, it, expect } from "vitest";
import { resolveShortcut } from "./keymap";

describe("resolveShortcut", () => {
  it("maps non-modifier keys", () => {
    expect(resolveShortcut({ key: "F2" })).toBe("rename");
    expect(resolveShortcut({ key: "Delete" })).toBe("delete");
    expect(resolveShortcut({ key: "Backspace" })).toBe("delete");
    expect(resolveShortcut({ key: "Tab" })).toBe("switchPane");
  });

  it("maps Cmd/Ctrl chords", () => {
    expect(resolveShortcut({ key: "r", metaKey: true })).toBe("refresh");
    expect(resolveShortcut({ key: "l", ctrlKey: true })).toBe("focusPath");
    expect(resolveShortcut({ key: "d", metaKey: true })).toBe("download");
    expect(resolveShortcut({ key: "u", ctrlKey: true })).toBe("upload");
  });

  it("requires a modifier for letter shortcuts", () => {
    expect(resolveShortcut({ key: "r" })).toBeNull();
    expect(resolveShortcut({ key: "d" })).toBeNull();
  });

  it("ignores unknown keys", () => {
    expect(resolveShortcut({ key: "x", metaKey: true })).toBeNull();
    expect(resolveShortcut({ key: "Enter" })).toBeNull();
  });

  it("suppresses shortcuts while typing in a field", () => {
    const input = document.createElement("input");
    expect(resolveShortcut({ key: "F2", target: input })).toBeNull();
    expect(resolveShortcut({ key: "d", metaKey: true, target: input })).toBeNull();
    const textarea = document.createElement("textarea");
    expect(resolveShortcut({ key: "Delete", target: textarea })).toBeNull();
  });
});
