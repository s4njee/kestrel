<!--
  SearchDialog.svelte — Remote tree search (Cmd/Ctrl+F, E8-S7).

  A terminal-grid `find` prompt over the remote pane's current directory. Type a
  fragment, press Enter to run it; results are a flat list of absolute paths.
  Choosing one closes the dialog and jumps the pane to the *containing*
  directory, which is what makes a hit actionable — you land where the file is,
  with the normal pane operations available.

  Searching is explicit (Enter) rather than as-you-type: each search is a real
  round-trip to the server — and on a server without `find`, a directory walk —
  so firing one per keystroke would flood the connection. That is the opposite
  trade-off from the in-pane filter (`/`), which narrows already-loaded rows and
  so is free.

  While a search runs, Escape cancels it rather than closing, so an expensive
  walk started by mistake can be stopped without losing what is typed.

  Props:
  - root: string                         — the directory being searched.
  - search: SearchState                  — the store's live state. (Not named
    `state`: a binding called `state` makes Svelte read `$state(…)` in this
    component as a store subscription rather than the rune.)
  - onSearch: (query: string) => void    — run a search.
  - onCancel: () => void                 — cancel the running search.
  - onOpen: (hit: SearchHit) => void     — reveal a hit in the pane.
  - onClose: () => void                  — dismiss.
-->
<script lang="ts">
  import type { SearchHit } from "$lib/ipc/commands";
  import type { SearchState } from "$lib/stores/search.svelte";

  interface Props {
    root: string;
    search: SearchState;
    onSearch: (query: string) => void;
    onCancel: () => void;
    onOpen: (hit: SearchHit) => void;
    onClose: () => void;
  }

  let { root, search, onSearch, onCancel, onOpen, onClose }: Props = $props();

  let query = $state("");
  let selected = $state(0);
  let inputEl = $state<HTMLInputElement | null>(null);

  let hits = $derived(search.result?.hits ?? []);

  // Keep the highlight on a real row as results arrive or are replaced.
  $effect(() => {
    if (selected >= hits.length) selected = Math.max(0, hits.length - 1);
  });

  $effect(() => {
    inputEl?.focus();
  });

  /**
   * Reveal a hit and close.
   *
   * @param hit - the chosen match.
   */
  function open(hit: SearchHit): void {
    onClose();
    onOpen(hit);
  }

  /**
   * Dialog keyboard handling.
   *
   * Escape is context-sensitive: while a search is running it cancels the
   * search (keeping the dialog and the typed query), and only closes the dialog
   * once nothing is in flight.
   *
   * @param event - the keydown event from the prompt input.
   */
  function onKeyDown(event: KeyboardEvent): void {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      if (hits.length > 0) selected = (selected + 1) % hits.length;
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      if (hits.length > 0) selected = (selected - 1 + hits.length) % hits.length;
    } else if (event.key === "Enter") {
      event.preventDefault();
      // Enter picks the highlighted hit once results are showing, and otherwise
      // runs the search.
      const hit = hits[selected];
      if (hit && search.query === query.trim()) open(hit);
      else if (query.trim()) onSearch(query.trim());
    } else if (event.key === "Escape") {
      event.preventDefault();
      if (search.running) onCancel();
      else onClose();
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
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Search remote files">
    <div class="prompt-row">
      <span class="prompt">find</span>
      <span class="root" title={root}>{root}</span>
      <span class="prompt">-iname</span>
      <input
        bind:this={inputEl}
        bind:value={query}
        onkeydown={onKeyDown}
        placeholder="name fragment…"
        aria-label="Search query"
        spellcheck="false"
        autocomplete="off"
      />
    </div>

    {#if search.running}
      <p class="status" role="status">searching… <span class="dim">esc to cancel</span></p>
    {:else if search.error}
      <p class="status error" role="alert">{search.error}</p>
    {:else if search.result}
      {#if hits.length === 0}
        <p class="status">— no matches —</p>
      {:else}
        <ul role="listbox" aria-label="Search results">
          {#each hits as hit, i (hit.path)}
            <li role="presentation">
              <button
                type="button"
                role="option"
                aria-selected={i === selected}
                class="row"
                class:selected={i === selected}
                onpointerenter={() => (selected = i)}
                onclick={() => open(hit)}
              >
                <span class="name">{hit.name}</span>
                <span class="path">{hit.path}</span>
              </button>
            </li>
          {/each}
        </ul>
        <!-- Bounds are always stated: a capped list must never read as a
             complete answer. -->
        <p class="status foot">
          {hits.length}
          {hits.length === 1 ? "match" : "matches"}
          {#if search.result.truncated}<span class="warn">· truncated at the search limit</span
            >{/if}
          {#if search.result.strategy === "walk"}<span class="dim"
              >· server has no find; walked over sftp</span
            >{/if}
        </p>
      {/if}
    {:else}
      <p class="status">enter a name fragment, then press return</p>
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
  .dialog {
    width: min(680px, 92vw);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
    box-shadow: 0 24px 60px -24px rgba(0, 0, 0, 0.8);
  }
  .prompt-row {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 9px 12px;
    border-bottom: 1px solid var(--grid);
    font-size: 13px;
  }
  .prompt {
    color: var(--dim);
    flex: 0 0 auto;
  }
  .root {
    color: var(--muted);
    max-width: 34%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    flex: 0 1 auto;
  }
  input {
    flex: 1 1 auto;
    min-width: 0;
    background: none;
    border: none;
    outline: none;
    font-size: 13px;
    font-family: inherit;
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
    align-items: baseline;
    gap: 12px;
    width: 100%;
    padding: 5px 12px;
    background: none;
    border: none;
    font-size: 12px;
    font-family: inherit;
    color: var(--text);
    cursor: pointer;
    text-align: left;
  }
  .row.selected {
    background: rgba(74, 222, 128, 0.1);
    box-shadow: inset 2px 0 0 var(--accent);
    color: var(--bright);
  }
  .name {
    flex: 0 0 auto;
  }
  .path {
    flex: 1 1 auto;
    color: var(--dim);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }
  .status {
    margin: 0;
    padding: 10px 12px;
    color: var(--muted);
    font-size: 12px;
  }
  .status.foot {
    border-top: 1px solid var(--grid);
    color: var(--dim);
  }
  .status.error {
    color: var(--danger);
  }
  .dim {
    color: var(--dim);
  }
  .warn {
    color: var(--warn);
  }
</style>
