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
import { routeSessionEvent, routeTransferEvent } from "./events";
import { sessions } from "$lib/stores/sessions.svelte";
import { prompts } from "$lib/stores/prompts.svelte";
import { toasts } from "$lib/stores/toasts.svelte";
import { transfers } from "$lib/stores/transfers.svelte";

beforeEach(() => {
  invokeMock.mockReset();
  // Reset store state between tests.
  for (const e of [...sessions.entries]) sessions.remove(e.info.id);
  for (const toast of [...toasts.items]) toasts.dismiss(toast.id);
  transfers.clearCompleted();
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
