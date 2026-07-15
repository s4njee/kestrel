<!--
  FilePane.svelte — One file browser pane (local or remote).

  Renders breadcrumbs + a virtualized FileTable for a PaneStore, with loading,
  error, and empty states. Double-clicking a directory navigates into it;
  clicking anywhere activates the pane. Source-agnostic: the same component
  drives both panes.

  Props:
  - pane: PaneStore              — the pane's reactive state.
  - active: boolean              — whether this pane is active.
  - onActivate: () => void       — called when the pane is clicked/focused.
  - onNavigate: (path) => void   — called to change directory.
  - emptyMessage?: string        — text shown when the listing is empty.
-->
<script lang="ts">
  import type { PaneStore } from "$lib/stores/panes.svelte";
  import type { DirEntry } from "$lib/ipc/commands";
  import Breadcrumbs from "./Breadcrumbs.svelte";
  import FileTable from "./FileTable.svelte";

  interface Props {
    pane: PaneStore;
    active: boolean;
    onActivate: () => void;
    onNavigate: (path: string) => void;
    emptyMessage?: string;
  }

  let { pane, active, onActivate, onNavigate, emptyMessage = "Empty" }: Props = $props();

  /** Open an entry: directories navigate; files are a no-op for now. */
  function onOpen(entry: DirEntry): void {
    if (entry.kind === "dir") onNavigate(entry.path);
  }
</script>

<section
  class="pane"
  class:active
  data-kind={pane.kind}
  aria-label={`${pane.kind} pane`}
  onpointerdown={onActivate}
  onfocusin={onActivate}
>
  {#if pane.path}
    <Breadcrumbs path={pane.path} kind={pane.kind} {onNavigate} />
  {:else}
    <header class="placeholder-header">{pane.kind === "remote" ? "Not connected" : "Local"}</header>
  {/if}

  <div class="body">
    {#if pane.loading}
      <p class="status">Loading…</p>
    {:else if pane.error}
      <p class="status error" role="alert">{pane.error}</p>
    {:else if pane.sortedEntries.length === 0}
      <p class="status">{emptyMessage}</p>
    {:else}
      <FileTable
        entries={pane.sortedEntries}
        sortKey={pane.sortKey}
        sortAsc={pane.sortAsc}
        selected={pane.selected}
        onSort={(k) => pane.setSort(k)}
        onSelect={(n, m) => pane.select(n, m)}
        {onOpen}
      />
    {/if}
  </div>
</section>

<style>
  .pane {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
    min-width: 0;
    border: 1px solid var(--border, #d0d0d0);
    border-radius: 6px;
    margin: 6px;
    overflow: hidden;
    background: var(--surface, #fff);
  }
  .pane.active {
    outline: 2px solid var(--accent, #396cd8);
    outline-offset: -1px;
  }
  .placeholder-header {
    padding: 6px 10px;
    font-size: 0.8rem;
    font-weight: 600;
    background: var(--surface-2, #f2f2f2);
    border-bottom: 1px solid var(--border, #d0d0d0);
  }
  .body {
    flex: 1 1 auto;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }
  .status {
    padding: 24px;
    text-align: center;
    color: var(--muted, #888);
    font-size: 0.85rem;
  }
  .status.error {
    color: #c0392b;
  }
</style>
