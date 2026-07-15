<!--
  ConnectDialog.svelte — New-connection form.

  Collects host/port/username and an auth method (password or private key file),
  then calls the `connect` command. While connecting, a host-key prompt may
  appear (HostKeyDialog, driven by the prompts store); this dialog stays in a
  "Connecting…" state until `connect` resolves or fails. On success it reports
  the SessionInfo and closes.

  Props:
  - onClose: () => void                    — dismiss the dialog.
  - onConnected: (info: SessionInfo) => void — called after a successful connect.
-->
<script lang="ts">
  import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
  import { connect, type ConnectRequest, type SessionInfo } from "$lib/ipc/commands";
  import Modal from "$lib/components/common/Modal.svelte";

  interface Props {
    onClose: () => void;
    onConnected: (info: SessionInfo) => void;
  }

  let { onClose, onConnected }: Props = $props();

  let host = $state("");
  let port = $state(22);
  let username = $state("");
  let method = $state<"password" | "key">("password");
  let password = $state("");
  let keyPath = $state("");
  let passphrase = $state("");

  let connecting = $state(false);
  let error = $state<string | null>(null);

  let canSubmit = $derived(
    host.trim() !== "" &&
      username.trim() !== "" &&
      (method === "password" ? password !== "" : keyPath.trim() !== "") &&
      !connecting,
  );

  /** Open a native file picker for the private key path. */
  async function browseKey(): Promise<void> {
    const selected = await openFileDialog({ multiple: false, directory: false });
    if (typeof selected === "string") keyPath = selected;
  }

  /** Build the connect request from the current form state. */
  function buildRequest(): ConnectRequest {
    const auth =
      method === "password"
        ? ({ method: "password", password } as const)
        : ({ method: "key", path: keyPath, passphrase: passphrase || null } as const);
    return { host: host.trim(), port, username: username.trim(), auth };
  }

  /** Submit the form: connect, then report success or show the error. */
  async function submit(event: Event): Promise<void> {
    event.preventDefault();
    if (!canSubmit) return;
    connecting = true;
    error = null;
    try {
      const info = await connect(buildRequest());
      onConnected(info);
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      connecting = false;
    }
  }
</script>

<Modal title="Connect to server" onClose={connecting ? undefined : onClose}>
  <form onsubmit={submit} class="form">
    <label>
      <span>Host</span>
      <input bind:value={host} placeholder="example.com" autocomplete="off" />
    </label>
    <label class="port">
      <span>Port</span>
      <input type="number" bind:value={port} min="1" max="65535" />
    </label>
    <label>
      <span>Username</span>
      <input bind:value={username} autocomplete="off" />
    </label>

    <fieldset class="auth">
      <legend>Authentication</legend>
      <label class="radio">
        <input type="radio" name="method" value="password" bind:group={method} /> Password
      </label>
      <label class="radio">
        <input type="radio" name="method" value="key" bind:group={method} /> Private key
      </label>
    </fieldset>

    {#if method === "password"}
      <label>
        <span>Password</span>
        <input type="password" bind:value={password} autocomplete="off" />
      </label>
    {:else}
      <label>
        <span>Key file</span>
        <span class="key-row">
          <input bind:value={keyPath} placeholder="~/.ssh/id_ed25519" />
          <button type="button" onclick={browseKey}>Browse…</button>
        </span>
      </label>
      <label>
        <span>Passphrase (if any)</span>
        <input type="password" bind:value={passphrase} autocomplete="off" />
      </label>
    {/if}

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}

    <div class="actions">
      <button type="button" onclick={onClose} disabled={connecting}>Cancel</button>
      <button type="submit" class="primary" disabled={!canSubmit}>
        {connecting ? "Connecting…" : "Connect"}
      </button>
    </div>
  </form>
</Modal>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 0.85rem;
  }
  label > span:first-child {
    color: var(--muted, #666);
  }
  input {
    padding: 6px 8px;
    border: 1px solid var(--border, #c4c4c4);
    border-radius: 6px;
    background: var(--surface, #fff);
    color: inherit;
  }
  .key-row {
    display: flex;
    gap: 6px;
  }
  .key-row input {
    flex: 1 1 auto;
  }
  .auth {
    border: 1px solid var(--border, #d0d0d0);
    border-radius: 6px;
    padding: 8px;
    display: flex;
    gap: 16px;
  }
  .radio {
    flex-direction: row;
    align-items: center;
    gap: 6px;
  }
  .error {
    color: #c0392b;
    font-size: 0.8rem;
    margin: 0;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
  button {
    padding: 6px 14px;
    border-radius: 6px;
    border: 1px solid var(--border, #c4c4c4);
    background: var(--surface, #fff);
    cursor: pointer;
  }
  button.primary {
    background: var(--accent, #396cd8);
    border-color: var(--accent, #396cd8);
    color: #fff;
  }
  button:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
</style>
