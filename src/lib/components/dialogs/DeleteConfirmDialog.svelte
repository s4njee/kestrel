<!--
  DeleteConfirmDialog.svelte — Confirm deleting entries.

  Lists the targets and, when any is a directory, warns that the delete is
  recursive. Confirming calls back.

  Props:
  - names: string[]          — the entry names being deleted.
  - hasDir: boolean          — whether any target is a directory (recursive).
  - onConfirm: () => void    — proceed with deletion.
  - onCancel: () => void     — dismiss.
-->
<script lang="ts">
  interface Props {
    names: string[];
    hasDir: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  }

  let { names, hasDir, onConfirm, onCancel }: Props = $props();

  import Modal from "$lib/components/common/Modal.svelte";
</script>

<Modal title={`Delete ${names.length} item${names.length === 1 ? "" : "s"}?`} onClose={onCancel}>
  <div class="content">
    <ul class="targets">
      {#each names.slice(0, 8) as name (name)}
        <li>{name}</li>
      {/each}
      {#if names.length > 8}
        <li class="more">…and {names.length - 8} more</li>
      {/if}
    </ul>

    {#if hasDir}
      <p class="warn" role="alert">
        This includes folders — their contents will be deleted recursively.
      </p>
    {/if}

    <div class="actions">
      <button type="button" onclick={onCancel}>Cancel</button>
      <button type="button" class="danger" onclick={onConfirm}>Delete</button>
    </div>
  </div>
</Modal>

<style>
  .content {
    display: flex;
    flex-direction: column;
    gap: 12px;
    font-size: 0.88rem;
  }
  .targets {
    margin: 0;
    padding-left: 18px;
    max-height: 180px;
    overflow: auto;
  }
  .more {
    list-style: none;
    color: var(--muted, #888);
  }
  .warn {
    margin: 0;
    color: #8a1c12;
    background: #fdecea;
    border: 1px solid #f5b7b1;
    border-radius: 6px;
    padding: 8px 10px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  button {
    padding: 6px 14px;
    border-radius: 6px;
    border: 1px solid var(--border, #c4c4c4);
    background: var(--surface, #fff);
    cursor: pointer;
  }
  button.danger {
    background: #c0392b;
    border-color: #c0392b;
    color: #fff;
  }
</style>
