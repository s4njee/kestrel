// fileops.test.ts — Component tests for the file-op dialogs and context menu.

import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import PermissionsDialog from "./PermissionsDialog.svelte";
import DeleteConfirmDialog from "./DeleteConfirmDialog.svelte";
import ContextMenu from "$lib/components/common/ContextMenu.svelte";

describe("PermissionsDialog", () => {
  it("applies the current mode and syncs the octal field", async () => {
    const onApply = vi.fn();
    render(PermissionsDialog, {
      props: { path: "/f", mode: 0o644, onApply, onCancel: vi.fn() },
    });
    const octal = document.querySelector('input[inputmode="numeric"]') as HTMLInputElement;
    expect(octal.value).toBe("644");

    // Toggle "Owner Execute" → 744.
    await fireEvent.click(screen.getByRole("checkbox", { name: "Owner Execute" }));
    expect(octal.value).toBe("744");

    await fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    expect(onApply).toHaveBeenCalledWith(0o744);
  });

  it("syncs the grid from the octal field", async () => {
    render(PermissionsDialog, {
      props: { path: "/f", mode: 0o644, onApply: vi.fn(), onCancel: vi.fn() },
    });
    const octal = document.querySelector('input[inputmode="numeric"]') as HTMLInputElement;
    await fireEvent.input(octal, { target: { value: "700" } });
    const ownerExec = screen.getByRole("checkbox", { name: "Owner Execute" }) as HTMLInputElement;
    const groupRead = screen.getByRole("checkbox", { name: "Group Read" }) as HTMLInputElement;
    expect(ownerExec.checked).toBe(true);
    expect(groupRead.checked).toBe(false);
  });
});

describe("DeleteConfirmDialog", () => {
  it("warns about recursion and confirms", async () => {
    const onConfirm = vi.fn();
    render(DeleteConfirmDialog, {
      props: { names: ["a", "folder"], hasDir: true, onConfirm, onCancel: vi.fn() },
    });
    expect(screen.getByText(/recursively/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(onConfirm).toHaveBeenCalled();
  });
});

describe("ContextMenu", () => {
  it("runs an item's action and closes; skips disabled", async () => {
    const rename = vi.fn();
    const disabled = vi.fn();
    const onClose = vi.fn();
    render(ContextMenu, {
      props: {
        x: 10,
        y: 10,
        items: [
          { label: "Rename", action: rename },
          { label: "Nope", action: disabled, disabled: true },
        ],
        onClose,
      },
    });
    await fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    expect(rename).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();

    const nope = screen.getByRole("menuitem", { name: "Nope" }) as HTMLButtonElement;
    expect(nope.disabled).toBe(true);
  });
});
