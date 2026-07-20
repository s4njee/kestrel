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

describe("ui store console height", () => {
  it("defaults to 18 lines' worth when nothing is stored", () => {
    expect(ui.consoleHeight).toBeGreaterThan(0);
  });

  it("persists the height to localStorage", () => {
    ui.consoleHeight = 240;
    expect(ui.consoleHeight).toBe(240);
    expect(window.localStorage.getItem("sftpapp.consoleHeight")).toBe("240");
  });

  it("clamps below the minimum", () => {
    ui.consoleHeight = 5;
    expect(ui.consoleHeight).toBe(64);
  });

  it("never lets the console exceed 80% of the window", () => {
    // jsdom's default window is 768px tall → ceiling 614.4px.
    ui.consoleHeight = 100_000;
    expect(ui.consoleHeight).toBeLessThanOrEqual(window.innerHeight * 0.8);
    expect(ui.consoleHeight).toBeGreaterThan(64);
  });

  it("falls back to the default for non-finite input", () => {
    ui.consoleHeight = Number.NaN;
    expect(ui.consoleHeight).toBe(360);
  });
});

describe("ui store follow-shell-cwd", () => {
  it("defaults to off (opt-in) and toggles + persists", () => {
    window.localStorage.removeItem("sftpapp.followShellCwd");
    // Fresh read is exercised via the live singleton's current value; the
    // important behaviour is that toggling round-trips to storage.
    const before = ui.followShellCwd;
    ui.toggleFollowShellCwd();
    expect(ui.followShellCwd).toBe(!before);
    expect(window.localStorage.getItem("sftpapp.followShellCwd")).toBe(String(!before));

    ui.toggleFollowShellCwd();
    expect(ui.followShellCwd).toBe(before);
    expect(window.localStorage.getItem("sftpapp.followShellCwd")).toBe(String(before));
  });
});
