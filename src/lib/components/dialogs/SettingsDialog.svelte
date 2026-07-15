<!--
  SettingsDialog.svelte — Edit and persist user settings.

  Edits a working copy of the current settings and saves via the settings store
  (the backend applies concurrency + conflict policy live). Covers transfer
  concurrency, the default conflict policy, an optional default local directory,
  and the show-hidden-files toggle.

  Props:
  - onClose: () => void — dismiss the dialog.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
  import { settings } from "$lib/stores/settings.svelte";
  import type { DefaultConflict, Settings } from "$lib/ipc/commands";
  import Modal from "$lib/components/common/Modal.svelte";

  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  // Work on a copy so Cancel discards edits.
  const start = untrack(() => settings.value);
  let concurrency = $state(start.concurrency);
  let defaultConflict = $state<DefaultConflict>(start.defaultConflict);
  let defaultLocalDir = $state(start.defaultLocalDir ?? "");
  let showHidden = $state(start.showHidden);

  let saving = $state(false);
  let error = $state<string | null>(null);

  const conflictOptions: { value: DefaultConflict; label: string }[] = [
    { value: "ask", label: "Ask each time" },
    { value: "overwrite", label: "Overwrite" },
    { value: "skip", label: "Skip" },
    { value: "rename", label: "Keep both (rename)" },
    { value: "resume", label: "Resume" },
  ];

  /** Pick a default local directory with the native folder picker. */
  async function browseDir(): Promise<void> {
    const selected = await openFileDialog({ directory: true, multiple: false });
    if (typeof selected === "string") defaultLocalDir = selected;
  }

  /** Save settings and close, or surface an error. */
  async function save(): Promise<void> {
    saving = true;
    error = null;
    const next: Settings = {
      concurrency,
      defaultConflict,
      defaultLocalDir: defaultLocalDir.trim() === "" ? null : defaultLocalDir.trim(),
      showHidden,
    };
    try {
      await settings.save(next);
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal title="Settings" onClose={saving ? undefined : onClose}>
  <div class="form">
    <label>
      <span>Concurrent transfers: {concurrency}</span>
      <input type="range" min="1" max="8" step="1" bind:value={concurrency} />
    </label>

    <label>
      <span>On file conflict</span>
      <select bind:value={defaultConflict}>
        {#each conflictOptions as opt (opt.value)}
          <option value={opt.value}>{opt.label}</option>
        {/each}
      </select>
    </label>

    <label>
      <span>Default local folder</span>
      <span class="dir-row">
        <input bind:value={defaultLocalDir} placeholder="(home directory)" />
        <button type="button" onclick={browseDir}>Browse…</button>
      </span>
    </label>

    <label class="check">
      <input type="checkbox" bind:checked={showHidden} /> Show hidden files
    </label>

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}

    <div class="actions">
      <button type="button" onclick={onClose} disabled={saving}>Cancel</button>
      <button type="button" class="primary" onclick={save} disabled={saving}>
        {saving ? "Saving…" : "Save"}
      </button>
    </div>
  </div>
</Modal>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 320px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 0.85rem;
  }
  label > span:first-child {
    color: var(--muted, #666);
  }
  input[type="range"] {
    width: 100%;
  }
  input:not([type]),
  select {
    padding: 6px 8px;
    border: 1px solid var(--border, #c4c4c4);
    border-radius: 6px;
    background: var(--surface, #fff);
    color: inherit;
  }
  .dir-row {
    display: flex;
    gap: 6px;
  }
  .dir-row input {
    flex: 1 1 auto;
    padding: 6px 8px;
    border: 1px solid var(--border, #c4c4c4);
    border-radius: 6px;
    background: var(--surface, #fff);
    color: inherit;
  }
  .check {
    flex-direction: row;
    align-items: center;
    gap: 6px;
  }
  .error {
    color: #c0392b;
    font-size: 0.8rem;
    margin: 0;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
  button {
    padding: 6px 14px;
    border-radius: 6px;
    border: 1px solid var(--border, #c4c4c4);
    background: var(--surface, #fff);
    cursor: pointer;
    color: inherit;
  }
  button.primary {
    background: var(--accent, #396cd8);
    border-color: var(--accent, #396cd8);
    color: #fff;
  }
  button:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
</style>
