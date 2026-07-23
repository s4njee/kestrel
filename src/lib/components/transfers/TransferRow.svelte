<!--
  TransferRow.svelte — One transfer in the queue stream.

  Terminal-grid row: direction arrow, filename, an ASCII progress bar
  (`[████░░░░]`), percent, and size, with the rate or state trailing. Completed
  transfers dim the arrow + percent; pause/resume/cancel appear as text actions
  while relevant.

  Props:
  - transfer: Transfer          — the transfer to render.
  - onCancel: (id) => void      — cancel this transfer.
  - onPause?/onResume?: (id) => void — pause/resume controls.
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
  let done = $derived(transfer.state === "done");
  let failed = $derived(transfer.state === "failed" || transfer.state === "failedVerification");
  let knownSize = $derived(transfer.size > 0);
  let indeterminate = $derived(!knownSize && active);
  let percent = $derived(
    knownSize ? Math.min(100, Math.round((transfer.bytes / transfer.size) * 100)) : done ? 100 : 0,
  );
  let arrow = $derived(transfer.direction === "upload" ? "↑" : "↓");
  let sizeLabel = $derived(
    knownSize ? formatBytes(transfer.size) : transfer.bytes > 0 ? formatBytes(transfer.bytes) : "—",
  );
  let progressText = $derived(
    knownSize
      ? `${percent}%`
      : done
        ? "Complete"
        : `${formatBytes(transfer.bytes)} transferred; total size unknown`,
  );

  // 20-cell ASCII progress bar (█ filled, ░ empty).
  let bar = $derived.by(() => {
    if (!knownSize) {
      if (done) return "█".repeat(20);
      // Tar-streamed folders do not have a known total. A striped bar, animated
      // below while running, communicates activity without inventing a percent.
      return "████░░░░".repeat(3).slice(0, 20);
    }
    const filled = Math.max(0, Math.min(20, Math.round(percent / 5)));
    return "█".repeat(filled) + "░".repeat(20 - filled);
  });
</script>

<div class="q-row" data-state={transfer.state}>
  <span class="q-arrow" class:done>{arrow}</span>
  <span class="q-file" title={transfer.name}>{transfer.name}</span>
  <span class="q-bracket">[</span><span
    class="q-bar"
    class:done
    class:indeterminate={indeterminate && transfer.state === "running"}
    role="progressbar"
    aria-label={`Transfer progress for ${transfer.name}`}
    aria-valuemin="0"
    aria-valuemax="100"
    aria-valuenow={knownSize || done ? percent : undefined}
    aria-valuetext={progressText}>{bar}</span
  ><span class="q-bracket">]</span>
  <span class="q-pct" class:done class:failed>{indeterminate ? "···" : `${percent}%`}</span>
  <span class="q-size" title={knownSize ? "Total size" : "Transferred so far"}>{sizeLabel}</span>
  <span class="q-meta">
    {#if transfer.state === "running"}
      {formatRate(transfer.rateBps)}
    {:else if transfer.state === "failed"}
      <span class="failed" title={transfer.error ?? ""}>failed</span>
    {:else if transfer.state === "failedVerification"}
      <span class="failed" title={transfer.error ?? ""}>verification failed</span>
    {:else if transfer.state !== "done"}
      {transfer.state}
    {/if}
  </span>
  <span class="q-actions">
    {#if paused}
      <button class="act" aria-label="Resume" onclick={() => onResume?.(transfer.id)}
        >[resume]</button
      >
    {:else if canPause}
      <button class="act" aria-label="Pause" onclick={() => onPause?.(transfer.id)}>[pause]</button>
    {/if}
    {#if active}
      <button class="act cancel" aria-label="Cancel" onclick={() => onCancel(transfer.id)}
        >[x]</button
      >
    {/if}
  </span>
</div>

<style>
  .q-row {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12px;
    line-height: 1.8;
    color: var(--text);
  }
  .q-arrow {
    width: 14px;
    text-align: center;
    color: var(--muted);
  }
  .q-arrow.done {
    color: var(--dim);
  }
  .q-file {
    width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .q-bracket {
    color: var(--dim);
  }
  /* The bar + percent are the one accented element: in-flight progress. */
  .q-bar {
    color: var(--accent);
    letter-spacing: 1px;
    font-size: 11px;
    white-space: pre;
  }
  .q-bar.done {
    color: var(--dim);
  }
  .q-bar.indeterminate {
    animation: transfer-pulse 0.8s ease-in-out infinite alternate;
  }
  @keyframes transfer-pulse {
    from {
      opacity: 0.35;
    }
    to {
      opacity: 1;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .q-bar.indeterminate {
      animation: none;
    }
  }
  .q-pct {
    width: 42px;
    text-align: right;
    color: var(--accent);
  }
  .q-pct.done {
    color: var(--dim);
  }
  .q-pct.failed {
    color: var(--danger);
  }
  .q-size {
    width: 78px;
    text-align: right;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .q-meta {
    flex: 1 1 auto;
    color: var(--dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .failed {
    color: var(--danger);
  }
  .q-actions {
    display: flex;
    gap: 8px;
    flex: 0 0 auto;
  }
  .act {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--muted);
    font-size: 11px;
    padding: 0;
  }
  .act:hover {
    color: var(--accent);
  }
  .cancel:hover {
    color: var(--danger);
  }
</style>
