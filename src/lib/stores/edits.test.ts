// edits.test.ts — Store tests for managed remote-file edit sessions.

import { beforeEach, describe, expect, it } from "vitest";
import type { EditSession } from "$lib/ipc/commands";
import { edits } from "./edits.svelte";

/** Build a test edit-session snapshot. */
function edit(overrides: Partial<EditSession> = {}): EditSession {
  return {
    id: "e1",
    sessionId: "s1",
    remotePath: "/note.txt",
    localPath: "/tmp/note.txt",
    state: "watching",
    error: null,
    ...overrides,
  };
}

beforeEach(() => edits.replace([]));

describe("edits store", () => {
  it("upserts lifecycle snapshots without duplicating sessions", () => {
    edits.upsert(edit());
    edits.upsert(edit({ state: "uploading" }));
    expect(edits.count).toBe(1);
    expect(edits.list[0].state).toBe("uploading");
  });

  it("removes closed sessions and all sessions for a disconnected host", () => {
    edits.replace([edit(), edit({ id: "e2" }), edit({ id: "e3", sessionId: "s2" })]);
    edits.remove("e2");
    expect(edits.list.map((item) => item.id)).toEqual(["e1", "e3"]);
    edits.removeForSession("s1");
    expect(edits.list.map((item) => item.id)).toEqual(["e3"]);
  });
});
