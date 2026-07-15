<!--
  TransferRow.svelte — One transfer in the dock.

  Shows direction, name, a progress bar, transferred/total bytes, current rate,
  and state. A cancel button appears while the transfer is active.

  Props:
  - transfer: Transfer          — the transfer to render.
  - onCancel: (id) => void      — cancel this transfer.
-->
<script lang="ts">
  import type { Transfer } from "$lib/stores/transfers.svelte";
  import { formatBytes, formatRate } from "$lib/utils/format";

  interface Props {
    transfer: Transfer;
    onCancel: (id: string) => void;
    onPause?: (id: string) => void;
    onResume?: (id: string) => void;
  }

  let { transfer, onCancel, onPause, onResume }: Props = $props();

  let active = $derived(
    transfer.state === "queued" || transfer.state === "running" || transfer.state === "paused",
  );
  let canPause = $derived(transfer.state === "queued" || transfer.state === "running");
  let paused = $derived(transfer.state === "paused");
  let percent = $derived(
    transfer.size > 0 ? Math.min(100, (transfer.bytes / transfer.size) * 100) : 0,
  );
  let arrow = $derived(transfer.direction === "upload" ? "↑" : "↓");
</script>

<div class="row" data-state={transfer.state}>
  <span class="arrow" class:up={transfer.direction === "upload"}>{arrow}</span>
  <div class="main">
    <div class="line">
      <span class="name" title={transfer.name}>{transfer.name}</span>
      <span class="meta">
        {#if transfer.state === "running"}
          {formatBytes(transfer.bytes)} / {formatBytes(transfer.size)} · {formatRate(
            transfer.rateBps,
          )}
        {:else if transfer.state === "failed"}
          <span class="failed" title={transfer.error ?? ""}>Failed</span>
        {:else if transfer.state === "paused"}
          Paused · {formatBytes(transfer.bytes)} / {formatBytes(transfer.size)}
        {:else}
          {transfer.state}
        {/if}
      </span>
    </div>
    <div class="bar" role="progressbar" aria-valuenow={Math.round(percent)}>
      <div class="fill" data-state={transfer.state} style:width="{percent}%"></div>
    </div>
  </div>
  {#if paused}
    <button class="act" title="Resume" aria-label="Resume" onclick={() => onResume?.(transfer.id)}
      >▶</button
    >
  {:else if canPause}
    <button class="act" title="Pause" aria-label="Pause" onclick={() => onPause?.(transfer.id)}
      >⏸</button
    >
  {/if}
  {#if active}
    <button class="cancel" title="Cancel" aria-label="Cancel" onclick={() => onCancel(transfer.id)}
      >✕</button
    >
  {/if}
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    font-size: 0.8rem;
  }
  .arrow {
    color: var(--accent, #396cd8);
    font-weight: 700;
  }
  .arrow.up {
    color: #2e9e5b;
  }
  .main {
    flex: 1 1 auto;
    min-width: 0;
  }
  .line {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    color: var(--muted, #777);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .failed {
    color: #c0392b;
  }
  .bar {
    height: 5px;
    border-radius: 3px;
    background: var(--surface-2, #e6e6e6);
    overflow: hidden;
    margin-top: 3px;
  }
  .fill {
    height: 100%;
    background: var(--accent, #396cd8);
    transition: width 0.1s linear;
  }
  .fill[data-state="done"] {
    background: #2e9e5b;
  }
  .fill[data-state="failed"] {
    background: #c0392b;
  }
  .cancel,
  .act {
    border: none;
    background: none;
    cursor: pointer;
    color: var(--muted, #888);
    font-size: 0.85rem;
  }
  .act:hover {
    color: var(--accent, #396cd8);
  }
  .cancel:hover {
    color: #c0392b;
  }
</style>
