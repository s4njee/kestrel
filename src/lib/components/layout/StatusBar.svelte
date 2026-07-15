<!--
  StatusBar.svelte — Bottom status strip.

  Shows the active pane, the remote connection status, and a live transfer
  count. Read-only; state comes from props and the ui store.

  Props:
  - connectionLabel: string  — e.g. "Not connected" or "user@host".
  - transferCount: number    — number of active transfers.
-->
<script lang="ts">
  import { ui } from "$lib/stores/ui.svelte";

  interface Props {
    connectionLabel: string;
    transferCount: number;
  }

  let { connectionLabel, transferCount }: Props = $props();
</script>

<footer class="status-bar">
  <span>Active: {ui.activePane}</span>
  <span class="sep">•</span>
  <span>{connectionLabel}</span>
  <span class="spacer"></span>
  <span>{transferCount} transfer{transferCount === 1 ? "" : "s"}</span>
</footer>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 12px;
    font-size: 0.75rem;
    color: var(--muted, #555);
    background: var(--surface-2, #ececec);
    border-top: 1px solid var(--border, #d0d0d0);
  }
  .sep {
    opacity: 0.5;
  }
  .spacer {
    flex: 1 1 auto;
  }
</style>
