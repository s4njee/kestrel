<!--
  TransferPanel.svelte — Bottom transfer dock.

  Collapsible panel that lists active/queued transfers. E0-S4 renders the
  collapsed header with a count and an empty state when expanded; real rows fed
  by the transfers store arrive in E2-S3. Expanded state lives in the ui store.

  Props:
  - count: number   — number of transfers to show in the header badge.
-->
<script lang="ts">
  import { ui } from "$lib/stores/ui.svelte";
  import Badge from "$lib/components/common/Badge.svelte";

  interface Props {
    count: number;
  }

  let { count }: Props = $props();
</script>

<section class="transfer-panel" class:expanded={ui.transferPanelExpanded}>
  <button class="dock-header" onclick={() => ui.toggleTransferPanel()}>
    <span class="chevron" class:open={ui.transferPanelExpanded}>▸</span>
    <span class="label">Transfers</span>
    <Badge {count} />
  </button>

  {#if ui.transferPanelExpanded}
    <div class="dock-body">
      {#if count === 0}
        <p class="empty">No transfers yet.</p>
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
    gap: 8px;
    width: 100%;
    padding: 6px 12px;
    background: none;
    border: none;
    cursor: pointer;
    font-size: 0.8rem;
    text-align: left;
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
  .dock-body {
    max-height: 180px;
    overflow: auto;
    padding: 6px 12px 12px;
  }
  .empty {
    color: var(--muted, #888);
    font-size: 0.8rem;
    margin: 8px 0;
  }
</style>
