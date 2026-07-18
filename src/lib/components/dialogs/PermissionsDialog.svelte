<!--
  PermissionsDialog.svelte — chmod editor.

  An rwx checkbox grid (owner/group/other) kept in sync with an octal field:
  toggling a box updates the octal value and vice-versa. Applying calls back with
  the new mode.

  Props:
  - path: string                 — the entry being edited (shown in the title).
  - mode: number                 — the current Unix mode (low bits).
  - onApply: (mode) => void      — called with the chosen mode.
  - onCancel: () => void         — dismiss without changes.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { modeToBits, bitsToMode, formatOctal, parseOctal } from "$lib/utils/perms";
  import Modal from "$lib/components/common/Modal.svelte";

  interface Props {
    path: string;
    mode: number;
    onApply: (mode: number) => void;
    onCancel: () => void;
  }

  let { path, mode, onApply, onCancel }: Props = $props();

  // The dialog is mounted fresh per target, so capturing the initial mode once
  // is intentional (the grid/octal then drive each other).
  let bits = $state<boolean[]>(untrack(() => modeToBits(mode)));
  let octal = $state<string>(untrack(() => formatOctal(mode)));

  const classes = ["Owner", "Group", "Other"];
  const perms = ["Read", "Write", "Execute"];

  /**
   * Toggle a bit and refresh the octal field.
   *
   * @param index - position in the 9-element bit grid (owner/group/other x rwx).
   */
  function toggle(index: number): void {
    bits[index] = !bits[index];
    octal = formatOctal(bitsToMode(bits));
  }

  /** Sync the grid from the octal field when it holds a valid value. */
  function onOctalInput(): void {
    const parsed = parseOctal(octal);
    if (parsed !== null) bits = modeToBits(parsed);
  }

  let currentMode = $derived(bitsToMode(bits));
</script>

<Modal title="Permissions" onClose={onCancel}>
  <div class="content">
    <p class="path" title={path}>{path}</p>

    <table class="grid">
      <thead>
        <tr>
          <th></th>
          {#each perms as p (p)}<th>{p}</th>{/each}
        </tr>
      </thead>
      <tbody>
        {#each classes as cls, row (cls)}
          <tr>
            <th class="rowlabel">{cls}</th>
            {#each perms as p, col (p)}
              <td>
                <input
                  type="checkbox"
                  aria-label={`${cls} ${p}`}
                  checked={bits[row * 3 + col]}
                  onchange={() => toggle(row * 3 + col)}
                />
              </td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>

    <label class="octal">
      Octal
      <input bind:value={octal} oninput={onOctalInput} maxlength="3" inputmode="numeric" />
    </label>

    <div class="actions">
      <button type="button" onclick={onCancel}>Cancel</button>
      <button type="button" class="primary" onclick={() => onApply(currentMode)}>Apply</button>
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
  .path {
    margin: 0;
    color: var(--muted, #666);
    word-break: break-all;
    font-family: ui-monospace, monospace;
    font-size: 0.8rem;
  }
  .grid {
    border-collapse: collapse;
  }
  .grid th,
  .grid td {
    padding: 4px 10px;
    text-align: center;
    font-weight: 500;
  }
  .rowlabel {
    text-align: left;
    color: var(--muted, #666);
  }
  .octal {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .octal input {
    width: 4rem;
    padding: 4px 6px;
    border: 1px solid var(--border, #c4c4c4);
    border-radius: 5px;
    font-family: ui-monospace, monospace;
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
