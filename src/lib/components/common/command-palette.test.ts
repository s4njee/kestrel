// command-palette.test.ts — Component tests for CommandPalette keyboard flow.

import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import CommandPalette from "./CommandPalette.svelte";
import type { PaletteCommand } from "$lib/palette";

function commands(): { list: PaletteCommand[]; ran: string[] } {
  const ran: string[] = [];
  const make = (id: string, label: string): PaletteCommand => ({
    id,
    label,
    run: () => ran.push(id),
  });
  return {
    list: [
      make("refresh", "refresh active pane"),
      make("settings", "settings…"),
      make("upload", "upload selection"),
    ],
    ran,
  };
}

describe("CommandPalette", () => {
  it("renders every command and filters as the user types", async () => {
    const { list } = commands();
    render(CommandPalette, { props: { commands: list, onClose: vi.fn() } });

    expect(screen.getByText("refresh active pane")).toBeInTheDocument();
    expect(screen.getByText("settings…")).toBeInTheDocument();

    await fireEvent.input(screen.getByRole("textbox", { name: "Command" }), {
      target: { value: "upl" },
    });
    expect(screen.queryByText("refresh active pane")).toBeNull();
    expect(screen.getByText("upload selection")).toBeInTheDocument();
  });

  it("Enter runs the highlighted command and closes", async () => {
    const { list, ran } = commands();
    const onClose = vi.fn();
    render(CommandPalette, { props: { commands: list, onClose } });

    const input = screen.getByRole("textbox", { name: "Command" });
    await fireEvent.keyDown(input, { key: "ArrowDown" });
    await fireEvent.keyDown(input, { key: "Enter" });

    expect(ran).toEqual(["settings"]);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("ArrowUp wraps to the last command", async () => {
    const { list, ran } = commands();
    render(CommandPalette, { props: { commands: list, onClose: vi.fn() } });

    const input = screen.getByRole("textbox", { name: "Command" });
    await fireEvent.keyDown(input, { key: "ArrowUp" });
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(ran).toEqual(["upload"]);
  });

  it("Escape closes without running anything", async () => {
    const { list, ran } = commands();
    const onClose = vi.fn();
    render(CommandPalette, { props: { commands: list, onClose } });

    await fireEvent.keyDown(screen.getByRole("textbox", { name: "Command" }), { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(ran).toEqual([]);
  });

  it("clicking a row runs it and closes", async () => {
    const { list, ran } = commands();
    const onClose = vi.fn();
    render(CommandPalette, { props: { commands: list, onClose } });

    await fireEvent.click(screen.getByText("settings…"));
    expect(ran).toEqual(["settings"]);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("Enter on an empty result list does nothing", async () => {
    const { list, ran } = commands();
    const onClose = vi.fn();
    render(CommandPalette, { props: { commands: list, onClose } });

    const input = screen.getByRole("textbox", { name: "Command" });
    await fireEvent.input(input, { target: { value: "zzzz" } });
    expect(screen.getByText("— no matching command —")).toBeInTheDocument();
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(ran).toEqual([]);
    expect(onClose).not.toHaveBeenCalled();
  });
});
