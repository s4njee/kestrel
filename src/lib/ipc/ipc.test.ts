// ipc.test.ts — Tests for the command wrappers and event routing.

import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock the Tauri core module before importing anything that uses it. The mock
// fn is created via vi.hoisted so it exists when the hoisted vi.mock factory runs.
const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
  Channel: class {
    onmessage: ((msg: unknown) => void) | null = null;
  },
}));

import { connect, type SessionInfo } from "./commands";
import { routeSessionEvent, routeTransferEvent, subscribeShell } from "./events";
import { sessions } from "$lib/stores/sessions.svelte";
import { prompts } from "$lib/stores/prompts.svelte";
import { toasts } from "$lib/stores/toasts.svelte";
import { transfers } from "$lib/stores/transfers.svelte";
import { edits } from "$lib/stores/edits.svelte";

beforeEach(() => {
  invokeMock.mockReset();
  // Reset store state between tests.
  for (const e of [...sessions.entries]) sessions.remove(e.info.id);
  for (const toast of [...toasts.items]) toasts.dismiss(toast.id);
  transfers.clearCompleted();
  edits.replace([]);
  prompts.clearHostKey();
});

describe("routeTransferEvent", () => {
  it("surfaces a checksum mismatch with a verification-specific toast", () => {
    routeTransferEvent({
      type: "state",
      id: "t-integrity",
      state: "failedVerification",
      error: "local and remote checksums differ",
      name: "payload.bin",
      size: 100,
      bytes: 100,
      direction: "upload",
    });

    expect(transfers.list.at(-1)?.state).toBe("failedVerification");
    expect(toasts.items.at(-1)?.message).toBe("Integrity verification failed: payload.bin");
  });
});

describe("commands.connect", () => {
  it("invokes the connect command and returns session info", async () => {
    const info: SessionInfo = { id: "abc", host: "h", port: 22, username: "u" };
    invokeMock.mockResolvedValueOnce(info);

    const result = await connect({
      host: "h",
      port: 22,
      username: "u",
      auth: { method: "password", password: "pw" },
    });

    expect(invokeMock).toHaveBeenCalledWith("connect", {
      request: { host: "h", port: 22, username: "u", auth: { method: "password", password: "pw" } },
    });
    expect(result).toEqual(info);
    // The connect flow tracks the session.
    sessions.add(result);
    expect(sessions.active?.info.id).toBe("abc");
  });
});

describe("routeSessionEvent", () => {
  it("routes edit-session changes and closes into the edits store", () => {
    routeSessionEvent({
      type: "editSessionChanged",
      session: {
        id: "e1",
        sessionId: "s1",
        remotePath: "/note.txt",
        localPath: "/tmp/note.txt",
        state: "watching",
        error: null,
      },
    });
    expect(edits.list[0].remotePath).toBe("/note.txt");

    routeSessionEvent({ type: "editSessionClosed", editId: "e1" });
    expect(edits.count).toBe(0);
  });

  it("routes connectionState disconnected by removing the session", () => {
    sessions.add({ id: "s1", host: "h", port: 22, username: "u" });
    expect(sessions.entries).toHaveLength(1);

    routeSessionEvent({
      type: "connectionState",
      sessionId: "s1",
      state: "disconnected",
      reason: null,
    });
    expect(sessions.entries).toHaveLength(0);
  });

  it("routes hostKeyPrompt into the prompts store", () => {
    routeSessionEvent({
      type: "hostKeyPrompt",
      promptId: "p1",
      host: "h",
      port: 22,
      keyType: "ssh-ed25519",
      fingerprintSha256: "SHA256:abc",
      status: "unknown",
      existingFingerprint: null,
    });
    expect(prompts.hostKey?.promptId).toBe("p1");
    expect(prompts.hostKey?.status).toBe("unknown");
  });
});

describe("shell event routing (E8-S9)", () => {
  /**
   * Emit one shell-output event.
   *
   * @param shellId - the shell the bytes came from.
   * @param text - the payload, base64-encoded onto the wire as the backend does.
   */
  function emitOutput(shellId: string, text: string): void {
    routeSessionEvent({ type: "shellData", shellId, data: btoa(text) } as never);
  }

  it("delivers each event to every mounted terminal, not just the last one", () => {
    // With one session per tab, several terminals are mounted at once. A
    // single-handler design silently routed every shell's output to whichever
    // terminal mounted last, so only one tab's shell would have worked.
    const first: string[] = [];
    const second: string[] = [];
    const offA = subscribeShell({
      onData: (id) => first.push(id),
      onClosed: () => {},
    });
    const offB = subscribeShell({
      onData: (id) => second.push(id),
      onClosed: () => {},
    });

    emitOutput("shell-a", "hello");
    emitOutput("shell-b", "world");

    expect(first).toEqual(["shell-a", "shell-b"]);
    expect(second).toEqual(["shell-a", "shell-b"]);
    offA();
    offB();
  });

  it("decodes the payload once and hands the same bytes to each subscriber", () => {
    const seen: Uint8Array[] = [];
    const off = subscribeShell({ onData: (_id, data) => seen.push(data), onClosed: () => {} });
    emitOutput("shell-a", "hi");
    off();
    expect(new TextDecoder().decode(seen[0])).toBe("hi");
  });

  it("stops delivering to an unsubscribed terminal", () => {
    const seen: string[] = [];
    const off = subscribeShell({ onData: (id) => seen.push(id), onClosed: () => {} });
    emitOutput("shell-a", "x");
    off();
    emitOutput("shell-a", "y");
    expect(seen).toEqual(["shell-a"]);
  });

  it("routes closure to every subscriber so each can check its own shell", () => {
    const closed: string[] = [];
    const off = subscribeShell({ onData: () => {}, onClosed: (id) => closed.push(id) });
    routeSessionEvent({ type: "shellClosed", shellId: "shell-b" } as never);
    off();
    expect(closed).toEqual(["shell-b"]);
  });
});
