<!--
  PromptDialog.svelte — Generic prompt (passphrase / keyboard-interactive).

  Renders one or more prompts, each masked or echoed, and returns the entered
  values. Reusable chrome for backend-driven auth prompts; it is wired to
  keyboard-interactive events in E4-S4. Presentational + callback based, so it
  is testable in isolation.

  Props:
  - title: string                              — dialog heading.
  - instructions?: string                      — optional helper text.
  - fields: { text: string; echo: boolean }[]  — prompts to render.
  - onSubmit: (values: string[]) => void       — called with entered values.
  - onCancel: () => void                        — dismiss without submitting.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import Modal from "$lib/components/common/Modal.svelte";

  interface Field {
    text: string;
    echo: boolean;
  }
  interface Props {
    title: string;
    instructions?: string;
    fields: Field[];
    onSubmit: (values: string[]) => void;
    onCancel: () => void;
  }

  let { title, instructions, fields, onSubmit, onCancel }: Props = $props();

  // Size the values array to the fields once; a prompt's fields don't change
  // during its lifetime, so capturing the initial value is intentional.
  let values = $state<string[]>(untrack(() => fields.map(() => "")));

  function submit(event: Event): void {
    event.preventDefault();
    onSubmit([...values]);
  }
</script>

<Modal {title} onClose={onCancel}>
  <form onsubmit={submit} class="form">
    {#if instructions}
      <p class="instructions">{instructions}</p>
    {/if}
    {#each fields as field, i (i)}
      <label>
        <span>{field.text}</span>
        <input type={field.echo ? "text" : "password"} bind:value={values[i]} autocomplete="off" />
      </label>
    {/each}
    <div class="actions">
      <button type="button" onclick={onCancel}>Cancel</button>
      <button type="submit" class="primary">OK</button>
    </div>
  </form>
</Modal>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .instructions {
    margin: 0;
    font-size: 0.85rem;
    color: var(--muted, #666);
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 0.85rem;
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
