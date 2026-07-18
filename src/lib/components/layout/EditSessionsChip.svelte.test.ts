// EditSessionsChip.svelte.test.ts — Live edit-session indicator tests.

import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import EditSessionsChip from "./EditSessionsChip.svelte";

describe("EditSessionsChip", () => {
  it("lists live sessions and closes the selected one", async () => {
    const onClose = vi.fn();
    render(EditSessionsChip, {
      props: {
        sessions: [
          {
            id: "e1",
            sessionId: "s1",
            remotePath: "/work/note.txt",
            localPath: "/tmp/note.txt",
            state: "conflict" as const,
            error: "remote file changed",
          },
        ],
        onClose,
      },
    });

    await fireEvent.click(screen.getByRole("button", { name: "1 live edit sessions" }));
    expect(screen.getByText("note.txt")).toBeInTheDocument();
    expect(screen.getByText("conflict")).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Close edit session for note.txt" }));
    expect(onClose).toHaveBeenCalledWith("e1");
  });
});
