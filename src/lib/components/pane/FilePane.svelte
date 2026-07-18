<!--
  FilePane.svelte — One file browser pane (local or remote).

  Terminal-grid pane: a shell command header (Breadcrumbs), a virtualized file
  grid (FileTable) with a `..` up row, and a footer showing the entry count and
  total size. Double-clicking a directory navigates into it; clicking anywhere
  activates the pane. Source-agnostic — the same component drives both panes.

  Props:
  - pane: PaneStore              — the pane's reactive state.
  - active: boolean              — whether this pane is active.
  - onActivate: () => void       — called when the pane is clicked/focused.
  - onNavigate: (path) => void   — called to change directory.
  - onToggleExpand?: (entry) => void — expand/collapse a directory in place.
  - emptyMessage?: string        — text shown when the listing is empty.
-->
<script lang="ts">
  import type { PaneStore } from "$lib/stores/panes.svelte";
  import type { DirEntry } from "$lib/ipc/commands";
  import type { PaneKind } from "$lib/types";
  import { parentPath } from "$lib/utils/path";
  import { formatBytes } from "$lib/utils/format";
  import Breadcrumbs from "./Breadcrumbs.svelte";
  import FileTable from "./FileTable.svelte";

  interface Props {
    pane: PaneStore;
    active: boolean;
    onActivate: () => void;
    onNavigate: (path: string) => void;
    onDrop?: (sourcePane: PaneKind) => void;
    onContextMenu?: (entry: DirEntry, event: MouseEvent) => void;
    onToggleExpand?: (entry: DirEntry) => void;
    emptyMessage?: string;
    banner?: string | null;
  }

  let {
    pane,
    active,
    onActivate,
    onNavigate,
    onDrop,
    onContextMenu,
    onToggleExpand,
    emptyMessage = "empty",
    banner = null,
  }: Props = $props();

  let dragOver = $state(false);

  // Footer summary: entry count + total bytes across files.
  let entryCount = $derived(pane.sortedEntries.length);
  let totalBytes = $derived(
    pane.sortedEntries.reduce((sum, e) => sum + (e.kind === "file" ? e.size : 0), 0),
  );
  // The parent directory to offer as the `..` row, or null at the root.
  let parent = $derived(pane.path ? parentPath(pane.path) : null);
  let hasParent = $derived(parent != null && parent !== pane.path);

  /**
   * Open an entry: directories navigate; files are a no-op for now.
   *
   * @param entry - the activated row's entry.
   */
  function onOpen(entry: DirEntry): void {
    if (entry.kind === "dir") onNavigate(entry.path);
  }

  /**
   * Allow dropping cross-pane drags onto this pane.
   *
   * @param event - the dragover event; its default is prevented (and the pane
   *   highlighted) only for cross-pane drags.
   */
  function onDragOver(event: DragEvent): void {
    if (event.dataTransfer?.types.includes("application/x-sftp-source")) {
      event.preventDefault();
      dragOver = true;
    }
  }

  /**
   * Handle a cross-pane drop: forward the source pane to the parent.
   *
   * @param event - the drop event; ignored unless it carries a local/remote source.
   */
  function onDropEvent(event: DragEvent): void {
    dragOver = false;
    const source = event.dataTransfer?.getData("application/x-sftp-source");
    if (source === "local" || source === "remote") {
      event.preventDefault();
      onDrop?.(source);
    }
  }
</script>

<section
  class="pane"
  class:active
  class:drag-over={dragOver}
  data-kind={pane.kind}
  aria-label={`${pane.kind} pane`}
  onpointerdown={onActivate}
  onfocusin={onActivate}
  ondragover={onDragOver}
  ondragleave={() => (dragOver = false)}
  ondrop={onDropEvent}
>
  {#if pane.path}
    <Breadcrumbs path={pane.path} kind={pane.kind} {onNavigate} />
  {:else}
    <header class="placeholder-header">
      <span class="prompt">{pane.kind}:~$</span> not connected
    </header>
  {/if}

  {#if banner}
    <div class="banner" role="status">{banner}</div>
  {/if}

  <div class="body">
    {#if pane.loading}
      <p class="status">loading…</p>
    {:else if pane.error}
      <p class="status error" role="alert">{pane.error}</p>
    {:else if pane.sortedEntries.length === 0}
      <FileTable
        rows={pane.rows}
        paneKind={pane.kind}
        sortKey={pane.sortKey}
        sortAsc={pane.sortAsc}
        selected={pane.selected}
        onSort={(k) => pane.setSort(k)}
        onSelect={(n, m) => pane.select(n, m)}
        {onOpen}
        {onContextMenu}
        {onToggleExpand}
        onParent={hasParent && parent ? () => onNavigate(parent) : undefined}
      />
      <p class="empty">{emptyMessage}</p>
    {:else}
      <FileTable
        rows={pane.rows}
        paneKind={pane.kind}
        sortKey={pane.sortKey}
        sortAsc={pane.sortAsc}
        selected={pane.selected}
        onSort={(k) => pane.setSort(k)}
        onSelect={(n, m) => pane.select(n, m)}
        {onOpen}
        {onContextMenu}
        {onToggleExpand}
        onParent={hasParent && parent ? () => onNavigate(parent) : undefined}
      />
    {/if}
  </div>

  <footer class="pane-foot">
    {entryCount}
    {entryCount === 1 ? "entry" : "entries"} · {formatBytes(totalBytes)}
  </footer>
</section>

<style>
  .pane {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
    min-width: 0;
    border-right: 1px solid var(--grid);
    overflow: hidden;
    background: var(--bg);
  }
  .pane:last-of-type {
    border-right: none;
  }
  .pane.active {
    box-shadow: inset 0 2px 0 var(--accent);
  }
  .pane.drag-over {
    background: color-mix(in srgb, var(--accent) 7%, var(--bg));
  }
  .placeholder-header {
    padding: 6px 12px;
    font-size: 12px;
    background: var(--surface);
    border-bottom: 1px solid var(--grid);
    color: var(--dim);
  }
  .placeholder-header .prompt {
    color: var(--dim);
  }
  .banner {
    padding: 4px 12px;
    font-size: 11px;
    background: rgba(224, 179, 65, 0.08);
    color: var(--warn);
    border-bottom: 1px solid rgba(224, 179, 65, 0.28);
  }
  .body {
    flex: 1 1 auto;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    position: relative;
  }
  .status {
    padding: 24px;
    text-align: center;
    color: var(--dim);
    font-size: 12px;
  }
  .status.error {
    color: var(--danger);
  }
  .empty {
    position: absolute;
    top: 44px;
    left: 0;
    right: 0;
    text-align: center;
    color: var(--dim);
    font-size: 12px;
    pointer-events: none;
    margin: 0;
  }
  .pane-foot {
    padding: 5px 12px;
    background: var(--surface);
    border-top: 1px solid var(--grid);
    font-size: 11px;
    color: var(--dim);
  }
</style>
