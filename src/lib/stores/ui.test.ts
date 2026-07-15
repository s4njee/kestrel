// ui.test.ts — Tests for the UI runes store (clamping, persistence, toggles).

import { describe, it, expect, beforeEach } from "vitest";
import { ui } from "./ui.svelte";

describe("ui store", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("clamps the split ratio into range", () => {
    ui.splitRatio = 0.99;
    expect(ui.splitRatio).toBeLessThanOrEqual(0.85);
    ui.splitRatio = 0.01;
    expect(ui.splitRatio).toBeGreaterThanOrEqual(0.15);
  });

  it("persists the split ratio to localStorage", () => {
    ui.splitRatio = 0.42;
    expect(window.localStorage.getItem("sftpapp.splitRatio")).toBe("0.42");
  });

  it("tracks the active pane", () => {
    ui.setActivePane("remote");
    expect(ui.activePane).toBe("remote");
    ui.setActivePane("local");
    expect(ui.activePane).toBe("local");
  });

  it("toggles the transfer panel", () => {
    ui.setTransferPanelExpanded(false);
    ui.toggleTransferPanel();
    expect(ui.transferPanelExpanded).toBe(true);
    ui.toggleTransferPanel();
    expect(ui.transferPanelExpanded).toBe(false);
  });
});
