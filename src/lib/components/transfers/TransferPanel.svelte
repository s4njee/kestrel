<!--
  TransferPanel.svelte — Transfer queue stream.

  Terminal-grid `── transfer queue ──` region listing all transfers from the
  store. The header rule toggles expansion and shows the active count; expanding
  reveals the rows (or an empty note) plus [pause all] / [clear] text actions.
  Expanded state lives in the ui store.
-->
<script lang="ts">
  import { ui } from "$lib/stores/ui.svelte";
  import { transfers } from "$lib/stores/transfers.svelte";
  import {
    cancelTransfer,
    clearCompleted,
    pauseTransfer,
    resumeTransfer,
    pauseAllTransfers,
  } from "$lib/ipc/commands";
  import TransferRow from "./TransferRow.svelte";

  /**
   * Cancel a transfer via the backend.
   *
   * @param id - the transfer to cancel.
   */
  function onCancel(id: string): void {
    void cancelTransfer(id);
  }

  /**
   * Pause a transfer.
   *
   * @param id - the transfer to pause.
   */
  function onPause(id: string): void {
    void pauseTransfer(id);
  }

  /**
   * Resume a paused transfer.
   *
   * @param id - the transfer to resume.
   */
  function onResume(id: string): void {
    void resumeTransfer(id);
  }

  /** Pause all active transfers. */
  function onPauseAll(): void {
    void pauseAllTransfers();
  }

  /** Clear finished transfers from the queue and store. */
  function onClear(): void {
    void clearCompleted();
    transfers.clearCompleted();
  }
</script>

<section class="stream" class:expanded={ui.transferPanelExpanded}>
  <div class="hr">
    <button class="hr-toggle" onclick={() => ui.toggleTransferPanel()}>
      <span class="chevron" class:open={ui.transferPanelExpanded}>▸</span>
      ── transfer queue ──
      {#if transfers.activeCount > 0}<span class="count">{transfers.activeCount} active</span>{/if}
      ──────────
    </button>
    {#if ui.transferPanelExpanded && transfers.list.length > 0}
      <span class="actions">
        {#if transfers.activeCount > 0}
          <button class="link" onclick={onPauseAll}>[pause all]</button>
        {/if}
        <button class="link" onclick={onClear}>[clear]</button>
      </span>
    {/if}
  </div>

  {#if ui.transferPanelExpanded}
    <div class="body">
      {#if transfers.list.length === 0}
        <p class="empty">— no transfers —</p>
      {:else}
        {#each transfers.list as transfer (transfer.id)}
          <TransferRow {transfer} {onCancel} {onPause} {onResume} />
        {/each}
      {/if}
    </div>
  {/if}
</section>

<style>
  .stream {
    border-top: 1px solid var(--border);
    background: var(--stream);
    padding: 8px 14px;
  }
  .hr {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .hr-toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    font-size: 11px;
    color: var(--dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chevron {
    display: inline-block;
    transition: transform 0.15s;
    color: var(--muted);
  }
  .chevron.open {
    transform: rotate(90deg);
  }
  .count {
    color: var(--accent);
  }
  .actions {
    display: flex;
    gap: 10px;
    flex: 0 0 auto;
  }
  .link {
    background: none;
    border: none;
    cursor: pointer;
    font-size: 11px;
    color: var(--muted);
    padding: 0;
  }
  .link:hover {
    color: var(--accent);
  }
  .body {
    margin-top: 6px;
    max-height: 168px;
    overflow: auto;
  }
  .empty {
    color: var(--dim);
    font-size: 12px;
    margin: 4px 0;
  }
</style>
