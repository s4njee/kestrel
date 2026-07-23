<!-- EncodeDialog.svelte — Options for a remote FFmpeg clip/transcode. -->
<script module lang="ts">
  export interface EncodeSettings {
    outputPath: string;
    startTime: string;
    endTime: string | null;
    burnSubtitles: boolean;
  }
</script>

<script lang="ts">
  import { untrack } from "svelte";
  import Modal from "$lib/components/common/Modal.svelte";

  interface Props {
    inputPath: string;
    initialOutputPath: string;
    onSubmit: (settings: EncodeSettings) => void;
    onCancel: () => void;
  }

  let { inputPath, initialOutputPath, onSubmit, onCancel }: Props = $props();
  let outputPath = $state(untrack(() => initialOutputPath));
  let startTime = $state("00:00:00");
  let endTime = $state("");
  let burnSubtitles = $state(false);
  let error = $state<string | null>(null);

  /** Convert seconds or a colon-delimited timestamp to seconds for validation. */
  function timeInSeconds(value: string): number | null {
    if (!/^\d+(?::\d+){0,2}(?:\.\d+)?$/.test(value)) return null;
    const parts = value.split(":").map(Number);
    if (parts.some((part) => !Number.isFinite(part))) return null;
    if (parts.length > 1 && parts.at(-1)! >= 60) return null;
    if (parts.length === 3 && parts[1] >= 60) return null;
    return parts.reduce((total, part) => total * 60 + part, 0);
  }

  /** Validate the form and pass normalized values to the page. */
  function submit(event: Event): void {
    event.preventDefault();
    error = null;
    const output = outputPath.trim();
    const start = startTime.trim();
    const end = endTime.trim();
    const startSeconds = timeInSeconds(start);
    const endSeconds = end ? timeInSeconds(end) : null;

    if (!output) error = "Output path is required.";
    else if (output === inputPath) error = "Output path must differ from the input path.";
    else if (startSeconds === null) error = "Start time must be seconds or HH:MM:SS.";
    else if (end && endSeconds === null) error = "End time must be seconds or HH:MM:SS.";
    else if (endSeconds !== null && endSeconds <= startSeconds)
      error = "End time must be after the start time.";
    else {
      onSubmit({
        outputPath: output,
        startTime: start,
        endTime: end || null,
        burnSubtitles,
      });
    }
  }
</script>

<Modal title="Encode with FFmpeg" onClose={onCancel}>
  <form class="form" onsubmit={submit}>
    <p class="input-path" title={inputPath}>{inputPath}</p>

    <div class="times">
      <label>
        <span>Start time</span>
        <input
          bind:value={startTime}
          placeholder="00:00:00"
          autocomplete="off"
          spellcheck="false"
        />
      </label>
      <label>
        <span>End time <small>(optional)</small></span>
        <input
          bind:value={endTime}
          placeholder="end of video"
          autocomplete="off"
          spellcheck="false"
        />
      </label>
    </div>

    <label>
      <span>Remote output path</span>
      <input bind:value={outputPath} autocomplete="off" spellcheck="false" />
    </label>

    <label class="check">
      <input type="checkbox" bind:checked={burnSubtitles} />
      <span>Burn in first embedded subtitle track</span>
    </label>

    <p class="summary">
      H.265 (libx265), CRF 18 · Opus audio · existing outputs are not overwritten
    </p>
    {#if error}<p class="error" role="alert">{error}</p>{/if}

    <div class="actions">
      <button type="button" onclick={onCancel}>Cancel</button>
      <button type="submit" class="primary">Encode</button>
    </div>
  </form>
</Modal>

<style>
  .form,
  label {
    display: flex;
    flex-direction: column;
  }
  .form {
    gap: 12px;
  }
  label {
    gap: 4px;
    font-size: 0.85rem;
  }
  label > span,
  .summary,
  .input-path {
    color: var(--muted, #777);
  }
  .input-path {
    margin: 0;
    font:
      0.8rem ui-monospace,
      monospace;
    word-break: break-all;
  }
  .times {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  input:not([type="checkbox"]) {
    min-width: 0;
    padding: 7px 8px;
    border: 1px solid var(--border, #c4c4c4);
    border-radius: 6px;
    background: var(--surface, #fff);
    color: inherit;
    font-family: ui-monospace, monospace;
  }
  .check {
    flex-direction: row;
    align-items: center;
    gap: 8px;
  }
  .summary,
  .error {
    margin: 0;
    font-size: 0.78rem;
  }
  .error {
    color: var(--danger, #d84a4a);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  button {
    padding: 6px 14px;
    border: 1px solid var(--border, #c4c4c4);
    border-radius: 6px;
    background: var(--surface, #fff);
    color: inherit;
    cursor: pointer;
  }
  button.primary {
    background: var(--accent, #396cd8);
    border-color: var(--accent, #396cd8);
    color: #fff;
  }
</style>
