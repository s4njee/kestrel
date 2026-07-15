<!--
  InputDialog.svelte — Single-line text prompt (rename, new folder).

  Props:
  - title: string                — dialog heading.
  - label: string                — field label.
  - initial?: string             — initial value (selected on open).
  - confirmLabel?: string        — confirm button text (default "OK").
  - onSubmit: (value) => void    — called with the trimmed value if non-empty.
  - onCancel: () => void         — dismiss.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import Modal from "$lib/components/common/Modal.svelte";

  interface Props {
    title: string;
    label: string;
    initial?: string;
    confirmLabel?: string;
    onSubmit: (value: string) => void;
    onCancel: () => void;
  }

  let { title, label, initial = "", confirmLabel = "OK", onSubmit, onCancel }: Props = $props();

  let value = $state(untrack(() => initial));

  function submit(event: Event): void {
    event.preventDefault();
    const trimmed = value.trim();
    if (trimmed) onSubmit(trimmed);
  }
</script>

<Modal {title} onClose={onCancel}>
  <form onsubmit={submit} class="form">
    <label>
      <span>{label}</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input bind:value autocomplete="off" spellcheck="false" autofocus />
    </label>
    <div class="actions">
      <button type="button" onclick={onCancel}>Cancel</button>
      <button type="submit" class="primary">{confirmLabel}</button>
    </div>
  </form>
</Modal>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 0.85rem;
  }
  label span {
    color: var(--muted, #666);
  }
  input {
    padding: 6px 8px;
    border: 1px solid var(--border, #c4c4c4);
    border-radius: 6px;
    background: var(--surface, #fff);
    color: inherit;
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
  button.primary {
    background: var(--accent, #396cd8);
    border-color: var(--accent, #396cd8);
    color: #fff;
  }
</style>
