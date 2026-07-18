<!--
  Toolbar.svelte — Terminal-grid top bar.

  The single command bar at the top of the shell: the `kestrel://` scheme, the
  active host, a session-detail chip, and the text action links
  ([connect]/[disconnect] [↑up] [↓down] [refresh] [queue] [settings]) plus a
  live-connection pill. Actions invoke optional callbacks; transfer actions are
  disabled until a session exists. No IPC here.

  Props:
  - connected: boolean          — whether a remote session is active.
  - host?: string               — active host label (e.g. "user@host:22").
  - meta?: string               — session-detail chip (e.g. "sftp · key").
  - canUpload/canDownload?: boolean — enable the transfer links.
  - onConnect/onUpload/onDownload/onRefresh/onQueue/onSettings?: () => void
-->
<script lang="ts">
  interface Props {
    connected: boolean;
    host?: string;
    meta?: string;
    canUpload?: boolean;
    canDownload?: boolean;
    onConnect?: () => void;
    onUpload?: () => void;
    onDownload?: () => void;
    onRefresh?: () => void;
    onQueue?: () => void;
    onSettings?: () => void;
  }

  let {
    connected,
    host = "not connected",
    meta = "sftp",
    canUpload = false,
    canDownload = false,
    onConnect,
    onUpload,
    onDownload,
    onRefresh,
    onQueue,
    onSettings,
  }: Props = $props();
</script>

<header class="topbar">
  <span class="scheme">kestrel://</span>
  <span class="who">{host}</span>
  <span class="chip">[{meta}]</span>

  <span class="tools">
    <button class="link" onclick={() => onConnect?.()}>
      [{connected ? "disconnect" : "connect"}]
    </button>
    <button class="link" disabled={!canUpload} onclick={() => onUpload?.()}>[↑up]</button>
    <button class="link" disabled={!canDownload} onclick={() => onDownload?.()}>[↓down]</button>
    <button class="link" onclick={() => onRefresh?.()}>[refresh]</button>
    <button class="link" onclick={() => onQueue?.()}>[queue]</button>
    <button class="link" onclick={() => onSettings?.()}>[settings]</button>
    <span class="live" class:on={connected}>● {connected ? "live" : "idle"}</span>
  </span>
</header>

<style>
  .topbar {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 7px 14px;
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
  }
  .scheme {
    font-weight: 600;
    color: var(--accent);
  }
  .who {
    color: var(--bright);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chip {
    color: var(--dim);
  }
  .tools {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .link {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    color: var(--text);
    font-size: 12px;
  }
  .link:hover:not(:disabled) {
    color: var(--accent);
  }
  .link:disabled {
    color: var(--dim);
    cursor: not-allowed;
  }
  .live {
    color: var(--dim);
    letter-spacing: 0.02em;
  }
  .live.on {
    color: var(--accent);
  }
</style>
