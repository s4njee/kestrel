<!--
  CommandPalette.svelte — Terminal-grid command palette (Cmd/Ctrl+K).

  A `>` prompt over a fuzzy-filtered list of every runnable action (built by
  $lib/palette; the shell supplies the handlers). Keyboard-first: type to
  filter, ArrowUp/Down to move, Enter runs the highlighted command and closes,
  Escape closes without running. Clicking a row runs it; clicking the backdrop
  closes. The input autofocuses on open.

  Props:
  - commands: PaletteCommand[]  — the inventory (canonical order).
  - onClose: () => void          — dismiss the palette (run or cancelled).
-->
<script lang="ts">
  import { filterCommands, type PaletteCommand } from "$lib/palette";

  interface Props {
    commands: PaletteCommand[];
    onClose: () => void;
  }

  let { commands, onClose }: Props = $props();

  let query = $state("");
  let selected = $state(0);
  let inputEl = $state<HTMLInputElement | null>(null);

  let matches = $derived(filterCommands(commands, query));

  // Keep the highlight on a real row as the filter narrows.
  $effect(() => {
    if (selected >= matches.length) selected = Math.max(0, matches.length - 1);
  });

  // Autofocus the prompt when the palette opens.
  $effect(() => {
    inputEl?.focus();
  });

  /**
   * Run a command and close the palette.
   *
   * @param command - the chosen command.
   */
  function run(command: PaletteCommand): void {
    onClose();
    command.run();
  }

  /**
   * Palette keyboard handling: navigation, dispatch, dismissal.
   *
   * @param event - the keydown event from the prompt input.
   */
  function onKeyDown(event: KeyboardEvent): void {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      if (matches.length > 0) selected = (selected + 1) % matches.length;
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      if (matches.length > 0) selected = (selected - 1 + matches.length) % matches.length;
    } else if (event.key === "Enter") {
      event.preventDefault();
      const command = matches[selected];
      if (command) run(command);
    } else if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }
</script>

<div
  class="overlay"
  role="presentation"
  onpointerdown={(e) => {
    if (e.target === e.currentTarget) onClose();
  }}
>
  <div class="palette" role="dialog" aria-modal="true" aria-label="Command palette">
    <div class="prompt-row">
      <span class="prompt">&gt;</span>
      <input
        bind:this={inputEl}
        bind:value={query}
        onkeydown={onKeyDown}
        placeholder="type a command…"
        aria-label="Command"
        spellcheck="false"
        autocomplete="off"
      />
    </div>
    {#if matches.length === 0}
      <p class="empty">— no matching command —</p>
    {:else}
      <ul role="listbox" aria-label="Commands">
        {#each matches as command, i (command.id)}
          <li role="presentation">
            <button
              type="button"
              role="option"
              aria-selected={i === selected}
              class="row"
              class:selected={i === selected}
              onpointerenter={() => (selected = i)}
              onclick={() => run(command)}
            >
              <span class="label">{command.label}</span>
              {#if command.hint}<span class="hint">{command.hint}</span>{/if}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    z-index: 60;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 14vh;
  }
  .palette {
    width: min(560px, 90vw);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
    box-shadow: 0 24px 60px -24px rgba(0, 0, 0, 0.8);
  }
  .prompt-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 12px;
    border-bottom: 1px solid var(--grid);
  }
  .prompt {
    color: var(--accent);
    font-weight: 600;
  }
  input {
    flex: 1 1 auto;
    background: none;
    border: none;
    outline: none;
    font-size: 13px;
    color: var(--bright);
  }
  input::placeholder {
    color: var(--dim);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 4px 0;
    max-height: 46vh;
    overflow: auto;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    padding: 5px 12px;
    background: none;
    border: none;
    font-size: 12px;
    color: var(--text);
    cursor: pointer;
    text-align: left;
  }
  .row.selected {
    background: rgba(74, 222, 128, 0.1);
    box-shadow: inset 2px 0 0 var(--accent);
    color: var(--bright);
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hint {
    flex: 0 0 auto;
    color: var(--dim);
    font-size: 11px;
  }
  .empty {
    margin: 0;
    padding: 12px;
    color: var(--dim);
    font-size: 12px;
  }
</style>
