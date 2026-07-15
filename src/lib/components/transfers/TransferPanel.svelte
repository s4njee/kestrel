<!--
  TransferPanel.svelte — Bottom transfer dock.

  Collapsible panel listing all transfers from the transfers store. The header
  shows the active count; expanding reveals the rows (or an empty state) and a
  "Clear finished" action. Expanded state lives in the ui store.
-->
<script lang="ts">
  import { ui } from "$lib/stores/ui.svelte";
  import { transfers } from "$lib/stores/transfers.svelte";
  import { cancelTransfer, clearCompleted } from "$lib/ipc/commands";
  import Badge from "$lib/components/common/Badge.svelte";
  import TransferRow from "./TransferRow.svelte";

  /** Cancel a transfer via the backend. */
  function onCancel(id: string): void {
    void cancelTransfer(id);
  }

  /** Clear finished transfers from the queue and store. */
  function onClear(): void {
    void clearCompleted();
    transfers.clearCompleted();
  }
</script>

<section class="transfer-panel" class:expanded={ui.transferPanelExpanded}>
  <div class="dock-header">
    <button class="toggle" onclick={() => ui.toggleTransferPanel()}>
      <span class="chevron" class:open={ui.transferPanelExpanded}>▸</span>
      <span class="label">Transfers</span>
      <Badge count={transfers.activeCount} />
    </button>
    {#if ui.transferPanelExpanded && transfers.list.length > 0}
      <button class="clear" onclick={onClear}>Clear finished</button>
    {/if}
  </div>

  {#if ui.transferPanelExpanded}
    <div class="dock-body">
      {#if transfers.list.length === 0}
        <p class="empty">No transfers yet.</p>
      {:else}
        {#each transfers.list as transfer (transfer.id)}
          <TransferRow {transfer} {onCancel} />
        {/each}
      {/if}
    </div>
  {/if}
</section>

<style>
  .transfer-panel {
    border-top: 1px solid var(--border, #d0d0d0);
    background: var(--surface-2, #f2f2f2);
  }
  .dock-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-right: 12px;
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: none;
    border: none;
    cursor: pointer;
    font-size: 0.8rem;
    color: inherit;
  }
  .chevron {
    display: inline-block;
    transition: transform 0.15s;
  }
  .chevron.open {
    transform: rotate(90deg);
  }
  .label {
    font-weight: 600;
  }
  .clear {
    background: none;
    border: none;
    cursor: pointer;
    font-size: 0.75rem;
    color: var(--accent, #396cd8);
  }
  .dock-body {
    max-height: 220px;
    overflow: auto;
    padding: 6px 12px 12px;
  }
  .empty {
    color: var(--muted, #888);
    font-size: 0.8rem;
    margin: 8px 0;
  }
</style>
