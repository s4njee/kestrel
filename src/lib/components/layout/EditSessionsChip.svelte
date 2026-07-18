<!--
  EditSessionsChip.svelte — Indicator and popover for live edit-and-sync sessions.

  The compact neutral chip expands into a list of managed remote files, their
  synchronization state, and a close action. Conflict/error states use the
  existing danger role; normal watching/uploading remain neutral.

  Props:
  - sessions: EditSession[]       — live engine snapshots.
  - onClose: (id: string) => void — stop an edit session.
-->
<script lang="ts">
  import type { EditSession } from "$lib/ipc/commands";

  interface Props {
    sessions: EditSession[];
    onClose: (id: string) => void;
  }

  let { sessions, onClose }: Props = $props();
  let open = $state(false);

  /**
   * Display only the final remote path component.
   *
   * @param path - remote POSIX path.
   * @returns the final non-empty component.
   */
  function baseName(path: string): string {
    return path.replace(/\/+$/, "").split("/").at(-1) ?? path;
  }
</script>

<span class="edit-wrap">
  <button
    type="button"
    class="edit-chip"
    aria-expanded={open}
    aria-label={`${sessions.length} live edit sessions`}
    onclick={() => (open = !open)}>[edit:{sessions.length}]</button
  >
  {#if open}
    <div class="edit-popover" aria-label="Live edit sessions">
      <div class="heading">remote edits</div>
      {#each sessions as session (session.id)}
        <div
          class="edit-row"
          class:problem={session.state === "conflict" || session.state === "error"}
        >
          <span class="file" title={session.remotePath}>{baseName(session.remotePath)}</span>
          <span class="state">{session.state}</span>
          <button
            type="button"
            class="close"
            aria-label={`Close edit session for ${baseName(session.remotePath)}`}
            onclick={() => onClose(session.id)}>[x]</button
          >
        </div>
        {#if session.error}<div class="detail">{session.error}</div>{/if}
      {/each}
    </div>
  {/if}
</span>

<style>
  .edit-wrap {
    position: relative;
  }
  .edit-chip,
  .close {
    border: 0;
    padding: 0;
    background: none;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
  }
  .edit-chip:hover,
  .close:hover {
    color: var(--bright);
  }
  .edit-popover {
    position: absolute;
    z-index: 220;
    top: calc(100% + 10px);
    right: 0;
    width: 300px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    background: var(--surface-2);
    box-shadow: 0 10px 24px rgba(0, 0, 0, 0.35);
  }
  .heading {
    margin-bottom: 6px;
    color: var(--dim);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .edit-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    gap: 10px;
    align-items: center;
    color: var(--text);
  }
  .edit-row.problem,
  .edit-row.problem .state {
    color: var(--danger);
  }
  .file {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .state {
    color: var(--dim);
  }
  .detail {
    margin: 2px 0 6px;
    color: var(--danger);
    font-size: 10px;
    white-space: normal;
  }
</style>
