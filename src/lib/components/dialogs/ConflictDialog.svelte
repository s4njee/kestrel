<!--
  ConflictDialog.svelte — Destination-exists conflict resolution.

  Shows the current pending conflict (from the conflicts store): the destination
  path plus the existing and incoming file sizes/times. The user picks
  Overwrite / Skip / Rename / Resume, optionally applying the choice to the rest
  of the batch. Answering resolves it via the backend and reveals the next
  pending conflict.
-->
<script lang="ts">
  import { conflicts } from "$lib/stores/conflicts.svelte";
  import { resolveConflict, type ConflictResolution } from "$lib/ipc/commands";
  import { formatBytes, formatMtime } from "$lib/utils/format";
  import Modal from "$lib/components/common/Modal.svelte";

  let conflict = $derived(conflicts.current);
  let applyToAll = $state(false);

  /**
   * Answer the current conflict and advance to the next.
   *
   * @param resolution - the choice to apply, to this conflict alone or to the whole
   *   batch when "apply to all" is checked.
   */
  async function choose(resolution: ConflictResolution): Promise<void> {
    if (!conflict) return;
    const id = conflict.id;
    const all = applyToAll;
    if (all) conflicts.clear();
    else conflicts.resolve(id);
    applyToAll = false;
    await resolveConflict(id, resolution, all);
  }
</script>

{#if conflict}
  <Modal title="File already exists" onClose={() => choose("skip")}>
    <div class="content">
      <p>
        <code class="dest">{conflict.dest}</code> already exists. What would you like to do?
      </p>

      <dl class="compare">
        <dt>Existing</dt>
        <dd>{formatBytes(conflict.existingSize)} · {formatMtime(conflict.existingMtime)}</dd>
        <dt>Incoming</dt>
        <dd>{formatBytes(conflict.incomingSize)} · {formatMtime(conflict.incomingMtime)}</dd>
      </dl>

      <label class="all">
        <input type="checkbox" bind:checked={applyToAll} />
        Apply to all conflicts in this batch
      </label>

      <div class="actions">
        <button onclick={() => choose("skip")}>Skip</button>
        <button onclick={() => choose("rename")}>Keep both</button>
        <button onclick={() => choose("resume")}>Resume</button>
        <button class="primary" onclick={() => choose("overwrite")}>Overwrite</button>
      </div>
    </div>
  </Modal>
{/if}

<style>
  .content {
    display: flex;
    flex-direction: column;
    gap: 12px;
    font-size: 0.88rem;
  }
  .dest {
    word-break: break-all;
    font-family: ui-monospace, monospace;
  }
  .compare {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 12px;
    margin: 0;
  }
  .compare dt {
    color: var(--muted, #666);
  }
  .compare dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
  }
  .all {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 0.85rem;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    flex-wrap: wrap;
  }
  button {
    padding: 6px 12px;
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
</style>
