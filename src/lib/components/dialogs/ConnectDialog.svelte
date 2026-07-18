<!--
  ConnectDialog.svelte — New-connection form.

  Collects host/port/username and an auth method (password, key file, ssh-agent,
  or keyboard-interactive), then calls the `connect` command. While connecting a
  host-key prompt may appear (HostKeyDialog, driven by the prompts store); this
  dialog stays "Connecting…" until `connect` resolves or fails. On success it
  reports the SessionInfo and closes.

  Doubles as the bookmark editor: pass `initial` to prefill from a saved
  bookmark, and use the "Save as bookmark" toggle to persist details (plus any
  secret) via the bookmarks store — either standalone ("Save") or alongside a
  connect.

  Props:
  - onClose: () => void                      — dismiss the dialog.
  - onConnected: (info: SessionInfo) => void — called after a successful connect.
  - initial?: Bookmark | null                — prefill from an existing bookmark.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
  import {
    connect,
    NIL_UUID,
    type Bookmark,
    type BookmarkAuthMethod,
    type ConnectRequest,
    type SessionInfo,
  } from "$lib/ipc/commands";
  import { bookmarks } from "$lib/stores/bookmarks.svelte";
  import Modal from "$lib/components/common/Modal.svelte";

  interface Props {
    onClose: () => void;
    onConnected: (info: SessionInfo) => void;
    /** Prefill the form from an existing bookmark (edit / connect-with-prompt). */
    initial?: Bookmark | null;
  }

  let { onClose, onConnected, initial = null }: Props = $props();

  // Prefill from a bookmark once on mount; the form is edited freely afterward.
  const seed = untrack(() => initial);
  let host = $state(seed?.host ?? "");
  let port = $state(seed?.port ?? 22);
  let username = $state(seed?.username ?? "");
  let method = $state<BookmarkAuthMethod>(seed?.authMethod ?? "password");
  let password = $state("");
  let keyPath = $state(seed?.keyPath ?? "");
  let passphrase = $state("");

  // Save-as-bookmark: on by default when editing an existing bookmark.
  let saveAsBookmark = $state(seed != null);
  let bookmarkName = $state(seed?.name ?? "");

  let connecting = $state(false);
  let error = $state<string | null>(null);

  let methodReady = $derived(
    method === "password" ? password !== "" : method === "key" ? keyPath.trim() !== "" : true,
  );
  let canSubmit = $derived(
    host.trim() !== "" && username.trim() !== "" && methodReady && !connecting,
  );
  // Saving (without connecting) needs the connection fields but not a secret.
  let canSave = $derived(host.trim() !== "" && username.trim() !== "" && !connecting);

  /** Open a native file picker for the private key path. */
  async function browseKey(): Promise<void> {
    const selected = await openFileDialog({ multiple: false, directory: false });
    if (typeof selected === "string") keyPath = selected;
  }

  /**
   * Build the connect request from the current form state.
   *
   * @returns the request, with an auth payload shaped to the selected method and
   *   host/username trimmed.
   */
  function buildRequest(): ConnectRequest {
    const auth: ConnectRequest["auth"] =
      method === "password"
        ? { method: "password", password }
        : method === "key"
          ? { method: "key", path: keyPath, passphrase: passphrase || null }
          : method === "keyboardInteractive"
            ? { method: "keyboardInteractive" }
            : { method: "agent" };
    return { host: host.trim(), port, username: username.trim(), auth };
  }

  /**
   * The secret to persist for the current method, or undefined if none.
   *
   * @returns the password for password auth, the passphrase for key auth, or
   *   undefined when empty or when the method needs no secret.
   */
  function secretValue(): string | undefined {
    if (method === "password") return password || undefined;
    if (method === "key") return passphrase || undefined;
    return undefined;
  }

  /**
   * Build a Bookmark from the current form (id preserved when editing).
   *
   * @returns the bookmark, named after the host when no name was given and reusing
   *   the seed's id and directories when editing an existing entry.
   */
  function formBookmark(): Bookmark {
    return {
      id: seed?.id ?? NIL_UUID,
      name: bookmarkName.trim() || host.trim(),
      host: host.trim(),
      port,
      username: username.trim(),
      authMethod: method,
      keyPath: method === "key" ? keyPath.trim() || null : null,
      remoteDir: seed?.remoteDir ?? null,
      localDir: seed?.localDir ?? null,
      hasSavedSecret: seed?.hasSavedSecret ?? false,
    };
  }

  /** Persist the bookmark (and any secret) without connecting. */
  async function save(): Promise<void> {
    if (!canSave) return;
    error = null;
    try {
      await bookmarks.save(formBookmark(), secretValue());
      onClose();
    } catch (e) {
      error = String(e);
    }
  }

  /**
   * Submit the form: connect, optionally save a bookmark, report the result.
   *
   * @param event - the form submit event; its default is prevented.
   */
  async function submit(event: Event): Promise<void> {
    event.preventDefault();
    if (!canSubmit) return;
    connecting = true;
    error = null;
    try {
      const info = await connect(buildRequest());
      if (saveAsBookmark) {
        // A save failure must not undo a successful connection.
        try {
          await bookmarks.save(formBookmark(), secretValue());
        } catch (e) {
          console.error("failed to save bookmark", e);
        }
      }
      onConnected(info);
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      connecting = false;
    }
  }
</script>

<Modal
  title={seed ? "Edit bookmark" : "Connect to server"}
  onClose={connecting ? undefined : onClose}
>
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
      <label class="radio">
        <input type="radio" name="method" value="agent" bind:group={method} /> ssh-agent
      </label>
      <label class="radio">
        <input type="radio" name="method" value="keyboardInteractive" bind:group={method} /> Keyboard-interactive
      </label>
    </fieldset>

    {#if method === "agent"}
      <p class="agent-note">Authentication will use identities from your running ssh-agent.</p>
    {:else if method === "keyboardInteractive"}
      <p class="agent-note">The server will prompt for credentials during connection.</p>
    {:else if method === "password"}
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

    <label class="radio save-toggle">
      <input type="checkbox" bind:checked={saveAsBookmark} /> Save as bookmark
    </label>
    {#if saveAsBookmark}
      <label>
        <span>Bookmark name</span>
        <input bind:value={bookmarkName} placeholder={host || "My server"} autocomplete="off" />
      </label>
    {/if}

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}

    <div class="actions">
      <button type="button" onclick={onClose} disabled={connecting}>Cancel</button>
      {#if saveAsBookmark}
        <button type="button" onclick={save} disabled={!canSave}>Save</button>
      {/if}
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
  .agent-note {
    margin: 0;
    font-size: 0.8rem;
    color: var(--muted, #666);
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
