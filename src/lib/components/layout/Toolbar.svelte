<!--
  Toolbar.svelte — Top application toolbar.

  E0-S4 skeleton: renders the app name and placeholder action buttons (Connect,
  Upload, Download). The Connect button invokes an optional callback so a later
  story can open the connect dialog (E1-S9); transfer actions are disabled until
  a session exists (E2-S4). No IPC here.

  Props:
  - connected: boolean          — whether a remote session is active.
  - onConnect?: () => void      — invoked when Connect/Disconnect is clicked.
-->
<script lang="ts">
  interface Props {
    connected: boolean;
    canUpload?: boolean;
    canDownload?: boolean;
    onConnect?: () => void;
    onUpload?: () => void;
    onDownload?: () => void;
  }

  let {
    connected,
    canUpload = false,
    canDownload = false,
    onConnect,
    onUpload,
    onDownload,
  }: Props = $props();
</script>

<header class="toolbar">
  <span class="brand">sftpapp</span>

  <div class="actions">
    <button class="primary" onclick={() => onConnect?.()}>
      {connected ? "Disconnect" : "Connect…"}
    </button>
    <button
      disabled={!canUpload}
      title="Upload selected local files to the remote folder (Cmd/Ctrl+U)"
      onclick={() => onUpload?.()}>Upload ↑</button
    >
    <button
      disabled={!canDownload}
      title="Download selected remote files to the local folder (Cmd/Ctrl+D)"
      onclick={() => onDownload?.()}>Download ↓</button
    >
  </div>
</header>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border, #d0d0d0);
    background: var(--surface-2, #f2f2f2);
  }
  .brand {
    font-weight: 700;
    letter-spacing: 0.02em;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  button {
    padding: 4px 12px;
    font-size: 0.8rem;
    border-radius: 6px;
    border: 1px solid var(--border, #c4c4c4);
    background: var(--surface, #ffffff);
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  button.primary {
    background: var(--accent, #396cd8);
    border-color: var(--accent, #396cd8);
    color: #ffffff;
  }
</style>
