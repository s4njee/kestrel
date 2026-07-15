// dialogs.test.ts — Component tests for the connect and prompt dialogs.

import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";

// Mock the IPC command wrappers and the native file dialog.
const { connectMock, respondPromptMock, openDialogMock } = vi.hoisted(() => ({
  connectMock: vi.fn(),
  respondPromptMock: vi.fn(),
  openDialogMock: vi.fn(),
}));
vi.mock("$lib/ipc/commands", () => ({
  connect: (...a: unknown[]) => connectMock(...a),
  respondPrompt: (...a: unknown[]) => respondPromptMock(...a),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...a: unknown[]) => openDialogMock(...a),
}));

import ConnectDialog from "./ConnectDialog.svelte";
import HostKeyDialog from "./HostKeyDialog.svelte";
import PromptDialog from "./PromptDialog.svelte";
import { prompts } from "$lib/stores/prompts.svelte";

beforeEach(() => {
  connectMock.mockReset();
  respondPromptMock.mockReset();
  openDialogMock.mockReset();
  prompts.clearHostKey();
});

describe("ConnectDialog", () => {
  it("submits a password connect and reports the session", async () => {
    const info = { id: "s1", host: "h", port: 22, username: "u" };
    connectMock.mockResolvedValueOnce(info);
    const onConnected = vi.fn();
    const onClose = vi.fn();

    render(ConnectDialog, { props: { onConnected, onClose } });

    await fireEvent.input(screen.getByPlaceholderText("example.com"), {
      target: { value: "h" },
    });
    // Username is the third text input; grab by label proximity via role.
    const inputs = screen.getAllByRole("textbox");
    await fireEvent.input(inputs[inputs.length - 1], { target: { value: "u" } });
    // Password field (type=password has no role); query by selector.
    const pw = document.querySelector('input[type="password"]') as HTMLInputElement;
    await fireEvent.input(pw, { target: { value: "pw" } });

    await fireEvent.click(screen.getByRole("button", { name: "Connect" }));

    expect(connectMock).toHaveBeenCalledTimes(1);
    const arg = connectMock.mock.calls[0][0];
    expect(arg.host).toBe("h");
    expect(arg.auth).toEqual({ method: "password", password: "pw" });
    expect(onConnected).toHaveBeenCalledWith(info);
  });
});

describe("HostKeyDialog", () => {
  it("shows the fingerprint and accepts an unknown key", async () => {
    prompts.setHostKeyPrompt({
      type: "hostKeyPrompt",
      promptId: "p1",
      host: "example.com",
      port: 22,
      keyType: "ssh-ed25519",
      fingerprintSha256: "SHA256:abc",
      status: "unknown",
      existingFingerprint: null,
    });
    respondPromptMock.mockResolvedValue(undefined);

    render(HostKeyDialog);
    expect(screen.getByText("SHA256:abc")).toBeInTheDocument();

    await fireEvent.click(screen.getByRole("button", { name: "Trust & connect" }));
    expect(respondPromptMock).toHaveBeenCalledWith("p1", { type: "hostKey", accept: true });
    expect(prompts.hostKey).toBeNull();
  });

  it("gates a CHANGED key behind the acknowledge checkbox", async () => {
    prompts.setHostKeyPrompt({
      type: "hostKeyPrompt",
      promptId: "p2",
      host: "example.com",
      port: 22,
      keyType: "ssh-ed25519",
      fingerprintSha256: "SHA256:new",
      status: "CHANGED",
      existingFingerprint: "SHA256:old",
    });

    render(HostKeyDialog);
    const accept = screen.getByRole("button", { name: "Replace & connect" }) as HTMLButtonElement;
    expect(accept.disabled).toBe(true);

    await fireEvent.click(screen.getByRole("checkbox"));
    expect(accept.disabled).toBe(false);
  });
});

describe("PromptDialog", () => {
  it("collects values and submits them", async () => {
    const onSubmit = vi.fn();
    render(PromptDialog, {
      props: {
        title: "Passphrase",
        fields: [{ text: "Passphrase:", echo: false }],
        onSubmit,
        onCancel: vi.fn(),
      },
    });

    const input = document.querySelector('input[type="password"]') as HTMLInputElement;
    await fireEvent.input(input, { target: { value: "secret" } });
    await fireEvent.click(screen.getByRole("button", { name: "OK" }));
    expect(onSubmit).toHaveBeenCalledWith(["secret"]);
  });
});
