<!--
  FileTable.svelte — Virtualized, sortable, multi-selectable file list.

  Hand-rolled windowed virtualization (fixed row height; only visible rows are
  rendered) so 10k+ entry directories scroll smoothly without a third-party
  virtual-list dependency. Sortable column headers, click/ctrl/shift
  multi-select, double-click to open, and type-ahead selection.

  Props:
  - entries: DirEntry[]                 — already sorted by the caller.
  - sortKey / sortAsc                    — current sort, for header indicators.
  - selected: Set<string>                — selected entry names.
  - onSort: (key) => void                — header click.
  - onSelect: (name, mods) => void       — row click with ctrl/shift modifiers.
  - onOpen: (entry) => void              — double-click (navigate/open).
-->
<script lang="ts">
  import type { SvelteSet } from "svelte/reactivity";
  import type { DirEntry } from "$lib/ipc/commands";
  import type { PaneKind } from "$lib/types";
  import type { SortKey } from "$lib/stores/panes.svelte";
  import { formatBytes, formatMtime } from "$lib/utils/format";

  interface Props {
    entries: DirEntry[];
    paneKind: PaneKind;
    sortKey: SortKey;
    sortAsc: boolean;
    selected: SvelteSet<string>;
    onSort: (key: SortKey) => void;
    onSelect: (name: string, mods: { ctrl: boolean; shift: boolean }) => void;
    onOpen: (entry: DirEntry) => void;
    onContextMenu?: (entry: DirEntry, event: MouseEvent) => void;
  }

  let {
    entries,
    paneKind,
    sortKey,
    sortAsc,
    selected,
    onSort,
    onSelect,
    onOpen,
    onContextMenu,
  }: Props = $props();

  /** Right-click a row: ensure it is selected, then open the menu. */
  function onRowContextMenu(entry: DirEntry, event: MouseEvent): void {
    event.preventDefault();
    if (!selected.has(entry.name)) onSelect(entry.name, { ctrl: false, shift: false });
    onContextMenu?.(entry, event);
  }

  /** Start a cross-pane drag: ensure the row is selected, mark the source. */
  function onDragStart(entry: DirEntry, event: DragEvent): void {
    if (!selected.has(entry.name)) onSelect(entry.name, { ctrl: false, shift: false });
    event.dataTransfer?.setData("application/x-sftp-source", paneKind);
    if (event.dataTransfer) event.dataTransfer.effectAllowed = "copy";
  }

  const ROW = 24;
  const OVERSCAN = 6;

  let scrollTop = $state(0);
  let viewportHeight = $state(0);

  let totalHeight = $derived(entries.length * ROW);
  let startIndex = $derived(Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN));
  let visibleCount = $derived(Math.ceil(viewportHeight / ROW) + OVERSCAN * 2);
  let visible = $derived(entries.slice(startIndex, startIndex + visibleCount));
  let offsetY = $derived(startIndex * ROW);

  function permString(mode: number | null): string {
    if (mode == null) return "—";
    const bits = ["r", "w", "x"];
    let out = "";
    for (let group = 2; group >= 0; group--) {
      for (let bit = 2; bit >= 0; bit--) {
        out += (mode >> (group * 3 + bit)) & 1 ? bits[2 - bit] : "-";
      }
    }
    return out;
  }

  function icon(entry: DirEntry): string {
    if (entry.kind === "dir") return "📁";
    if (entry.kind === "symlink") return "🔗";
    return "📄";
  }

  function onRowClick(entry: DirEntry, event: MouseEvent): void {
    onSelect(entry.name, { ctrl: event.metaKey || event.ctrlKey, shift: event.shiftKey });
  }

  function onScroll(event: Event): void {
    scrollTop = (event.currentTarget as HTMLDivElement).scrollTop;
  }

  function sortArrow(key: SortKey): string {
    if (sortKey !== key) return "";
    return sortAsc ? " ▲" : " ▼";
  }
</script>

<div class="table">
  <div class="head">
    <button class="col-name" onclick={() => onSort("name")}>Name{sortArrow("name")}</button>
    <button class="col-size" onclick={() => onSort("size")}>Size{sortArrow("size")}</button>
    <button class="col-mtime" onclick={() => onSort("mtime")}>Modified{sortArrow("mtime")}</button>
    <button class="col-perms" onclick={() => onSort("permissions")}
      >Perms{sortArrow("permissions")}</button
    >
  </div>

  <div class="viewport" bind:clientHeight={viewportHeight} onscroll={onScroll}>
    <div class="spacer" style:height="{totalHeight}px">
      <div class="rows" style:transform="translateY({offsetY}px)">
        {#each visible as entry (entry.path)}
          <button
            type="button"
            class="row"
            draggable="true"
            aria-pressed={selected.has(entry.name)}
            class:selected={selected.has(entry.name)}
            style:height="{ROW}px"
            onclick={(e) => onRowClick(entry, e)}
            ondblclick={() => onOpen(entry)}
            ondragstart={(e) => onDragStart(entry, e)}
            oncontextmenu={(e) => onRowContextMenu(entry, e)}
          >
            <span class="col-name"><span class="icon">{icon(entry)}</span>{entry.name}</span>
            <span class="col-size">{entry.kind === "dir" ? "—" : formatBytes(entry.size)}</span>
            <span class="col-mtime">{formatMtime(entry.mtime)}</span>
            <span class="col-perms">{permString(entry.permissions)}</span>
          </button>
        {/each}
      </div>
    </div>
  </div>
</div>

<style>
  .table {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
  }
  .head {
    display: grid;
    grid-template-columns: 1fr 90px 150px 100px;
    border-bottom: 1px solid var(--border, #d0d0d0);
  }
  .head button {
    text-align: left;
    background: none;
    border: none;
    padding: 4px 10px;
    font-size: 0.78rem;
    font-weight: 600;
    color: var(--muted, #666);
    cursor: pointer;
  }
  .head .col-size,
  .head .col-perms {
    text-align: right;
  }
  .viewport {
    flex: 1 1 auto;
    overflow-y: auto;
    min-height: 0;
    outline: none;
  }
  .spacer {
    position: relative;
  }
  .rows {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 90px 150px 100px;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    font-size: 0.8rem;
    cursor: default;
    width: 100%;
    border: none;
    background: none;
    color: inherit;
    font-family: inherit;
    text-align: left;
  }
  .row:hover {
    background: var(--surface-2, #f2f2f2);
  }
  .row.selected {
    background: var(--accent, #396cd8);
    color: #fff;
  }
  .col-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .col-size,
  .col-perms {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .icon {
    margin-right: 6px;
  }
</style>
