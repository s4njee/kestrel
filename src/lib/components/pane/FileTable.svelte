<!--
  FileTable.svelte — Virtualized, sortable, multi-selectable file grid.

  Terminal-grid styled `ls -la` listing: columns Perms · Name · Size · Modified,
  glyph icons (`↰` parent, `▸`/`▾` folder disclosure, `•` file), directories
  brighter than files. Directories expand in place — clicking anywhere on a
  folder's label (the glyph or its name) toggles it open and its children render
  indented beneath it; the caller supplies the children. Hand-rolled windowed
  virtualization over the already-flattened rows (fixed row height; only visible
  rows are rendered) so 10k+ entry directories still scroll smoothly.

  Props:
  - rows: PaneRow[]                      — the flattened tree, sorted by the caller.
  - sortKey / sortAsc                    — current sort, for header indicators.
  - selected: Set<string>                — selected entry PATHS.
  - onSort: (key) => void                — header click.
  - onSelect: (path, mods) => void       — row click with ctrl/shift modifiers.
                                           (a plain click on a dir's label also expands it)
  - onOpen: (entry) => void              — double-click (navigate/open).
  - onToggleExpand?: (entry) => void     — a directory's label was clicked.
  - onParent?: () => void                — navigate up (renders the `..` row).
-->
<script lang="ts">
  import type { SvelteSet } from "svelte/reactivity";
  import type { DirEntry } from "$lib/ipc/commands";
  import type { PaneKind } from "$lib/types";
  import type { PaneRow, SortKey } from "$lib/stores/panes.svelte";
  import { formatBytes, formatMtime } from "$lib/utils/format";

  interface Props {
    rows: PaneRow[];
    paneKind: PaneKind;
    sortKey: SortKey;
    sortAsc: boolean;
    selected: SvelteSet<string>;
    onSort: (key: SortKey) => void;
    onSelect: (path: string, mods: { ctrl: boolean; shift: boolean }) => void;
    onOpen: (entry: DirEntry) => void;
    onContextMenu?: (entry: DirEntry, event: MouseEvent) => void;
    onToggleExpand?: (entry: DirEntry) => void;
    onParent?: () => void;
  }

  let {
    rows,
    paneKind,
    sortKey,
    sortAsc,
    selected,
    onSort,
    onSelect,
    onOpen,
    onContextMenu,
    onToggleExpand,
    onParent,
  }: Props = $props();

  /**
   * Right-click a row: ensure it is selected, then open the menu.
   *
   * @param entry - the right-clicked row's entry.
   * @param event - the contextmenu event; its default is prevented.
   */
  function onRowContextMenu(entry: DirEntry, event: MouseEvent): void {
    event.preventDefault();
    if (!selected.has(entry.path)) onSelect(entry.path, { ctrl: false, shift: false });
    onContextMenu?.(entry, event);
  }

  /**
   * Start a cross-pane drag: ensure the row is selected, mark the source.
   *
   * @param entry - the dragged row's entry.
   * @param event - the dragstart event; its dataTransfer carries the source pane.
   */
  function onDragStart(entry: DirEntry, event: DragEvent): void {
    if (!selected.has(entry.path)) onSelect(entry.path, { ctrl: false, shift: false });
    event.dataTransfer?.setData("application/x-sftp-source", paneKind);
    if (event.dataTransfer) event.dataTransfer.effectAllowed = "copy";
  }

  const ROW = 26;
  const OVERSCAN = 6;
  /** Horizontal indent applied per nesting level, in pixels. */
  const INDENT = 14;

  let scrollTop = $state(0);
  let viewportHeight = $state(0);

  let totalHeight = $derived(rows.length * ROW);
  let startIndex = $derived(Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN));
  let visibleCount = $derived(Math.ceil(viewportHeight / ROW) + OVERSCAN * 2);
  let visible = $derived(rows.slice(startIndex, startIndex + visibleCount));
  let offsetY = $derived(startIndex * ROW);

  /**
   * Format a Unix mode as a 10-character `drwxr-xr-x`-style string.
   *
   * @param mode - the permission bits, or null when unknown.
   * @param kind - the entry kind (sets the leading type character).
   * @returns the symbolic permission string, or "—" when `mode` is null.
   */
  function permString(mode: number | null, kind: DirEntry["kind"]): string {
    if (mode == null) return "—";
    const type = kind === "dir" ? "d" : kind === "symlink" ? "l" : "-";
    const bits = ["r", "w", "x"];
    let out = type;
    for (let group = 2; group >= 0; group--) {
      for (let bit = 2; bit >= 0; bit--) {
        out += (mode >> (group * 3 + bit)) & 1 ? bits[2 - bit] : "-";
      }
    }
    return out;
  }

  /**
   * Pick the leading glyph for a row: a disclosure arrow for directories,
   * otherwise a type glyph.
   *
   * @param row - the row being rendered.
   * @returns `▾`/`▸` for an expanded/collapsed dir, `↳` for a symlink, `•` for a file.
   */
  function icon(row: PaneRow): string {
    if (row.entry.kind === "dir") return row.expanded ? "▾" : "▸";
    if (row.entry.kind === "symlink") return "↳";
    return "•";
  }

  /**
   * Handle a row click. The row always takes the selection; additionally, a
   * plain click anywhere on a **directory's label** (the glyph *and* its name —
   * the whole `.col-name` cell, not just the tiny arrow) toggles it open.
   *
   * Two guards keep that from fighting other gestures:
   * - modifier clicks select only, so ctrl/shift range-select over folders does
   *   not flap them open and shut;
   * - `detail > 1` means this is the second click of a double-click, which is
   *   the navigate gesture — let `ondblclick` own it rather than toggling back.
   *
   * The glyph is a plain span (not a nested button, which would be invalid
   * inside the row's button), so the target is matched here.
   *
   * @param row - the clicked row.
   * @param event - the mouse event (ctrl/cmd, shift, and click count are read).
   */
  function onRowClick(row: PaneRow, event: MouseEvent): void {
    const ctrl = event.metaKey || event.ctrlKey;
    onSelect(row.entry.path, { ctrl, shift: event.shiftKey });

    if (row.entry.kind !== "dir" || ctrl || event.shiftKey || event.detail > 1) return;
    if ((event.target as HTMLElement | null)?.closest(".col-name")) {
      onToggleExpand?.(row.entry);
    }
  }

  /**
   * Keyboard disclosure on a focused directory row: Right expands, Left collapses.
   *
   * @param row - the focused row.
   * @param event - the keydown event.
   */
  function onRowKeyDown(row: PaneRow, event: KeyboardEvent): void {
    if (row.entry.kind !== "dir") return;
    if (event.key === "ArrowRight" && !row.expanded) {
      event.preventDefault();
      onToggleExpand?.(row.entry);
    } else if (event.key === "ArrowLeft" && row.expanded) {
      event.preventDefault();
      onToggleExpand?.(row.entry);
    }
  }

  /**
   * Track the viewport scroll offset that drives virtualization.
   *
   * @param event - the scroll event from the viewport element.
   */
  function onScroll(event: Event): void {
    scrollTop = (event.currentTarget as HTMLDivElement).scrollTop;
  }

  /**
   * The sort-direction arrow to show on a column header.
   *
   * @param key - the column being rendered.
   * @returns " ▲"/" ▼" for the active sort column, else "".
   */
  function sortArrow(key: SortKey): string {
    if (sortKey !== key) return "";
    return sortAsc ? " ▲" : " ▼";
  }
</script>

<div class="table">
  <div class="head">
    <button class="col-perms" onclick={() => onSort("permissions")}
      >Perms{sortArrow("permissions")}</button
    >
    <button class="col-name" onclick={() => onSort("name")}>Name{sortArrow("name")}</button>
    <button class="col-size" onclick={() => onSort("size")}>Size{sortArrow("size")}</button>
    <button class="col-mtime" onclick={() => onSort("mtime")}>Modified{sortArrow("mtime")}</button>
  </div>

  <div class="viewport" bind:clientHeight={viewportHeight} onscroll={onScroll}>
    {#if onParent}
      <button type="button" class="row up" style:height="{ROW}px" ondblclick={() => onParent?.()}>
        <span class="col-perms"></span>
        <span class="col-name"><span class="icon">↰</span>..</span>
        <span class="col-size"></span>
        <span class="col-mtime"></span>
      </button>
    {/if}
    <div class="spacer" style:height="{totalHeight}px">
      <div class="rows" style:transform="translateY({offsetY}px)">
        {#each visible as row (row.entry.path)}
          <button
            type="button"
            class="row"
            data-row-kind={row.entry.kind}
            data-depth={row.depth}
            draggable="true"
            aria-pressed={selected.has(row.entry.path)}
            aria-expanded={row.entry.kind === "dir" ? row.expanded : undefined}
            class:selected={selected.has(row.entry.path)}
            style:height="{ROW}px"
            onclick={(e) => onRowClick(row, e)}
            ondblclick={() => onOpen(row.entry)}
            onkeydown={(e) => onRowKeyDown(row, e)}
            ondragstart={(e) => onDragStart(row.entry, e)}
            oncontextmenu={(e) => onRowContextMenu(row.entry, e)}
          >
            <span class="col-perms">{permString(row.entry.permissions, row.entry.kind)}</span>
            <span class="col-name" style:padding-left="{row.depth * INDENT}px">
              <span
                class="icon"
                class:twisty={row.entry.kind === "dir"}
                class:busy={row.loading}
                title={row.entry.kind === "dir" ? "Expand/collapse" : undefined}>{icon(row)}</span
              >{row.entry.name}
            </span>
            <span class="col-size">
              {row.entry.kind === "dir" ? "" : formatBytes(row.entry.size)}
            </span>
            <span class="col-mtime">{formatMtime(row.entry.mtime)}</span>
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
    grid-template-columns: 96px 1fr 74px 148px;
    border-bottom: 1px solid var(--grid);
    background: var(--surface);
  }
  .head button {
    text-align: left;
    background: none;
    border: none;
    padding: 4px 12px;
    font-size: 10.5px;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--dim);
    cursor: pointer;
  }
  .head button:hover {
    color: var(--muted);
  }
  .head .col-size {
    text-align: right;
  }
  .viewport {
    flex: 1 1 auto;
    overflow-y: auto;
    min-height: 0;
    outline: none;
    background: var(--bg);
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
    grid-template-columns: 96px 1fr 74px 148px;
    align-items: center;
    padding: 0 12px;
    font-size: 12px;
    cursor: default;
    width: 100%;
    border: none;
    background: none;
    color: var(--muted);
    font-family: inherit;
    text-align: left;
  }
  .row:hover {
    background: #171717;
  }
  .row.selected {
    background: rgba(74, 222, 128, 0.1);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .row.up {
    color: var(--dim);
  }
  /* Directories read brighter than files (brightness, not hue). */
  .row[data-row-kind="dir"] {
    color: var(--bright);
  }
  .row[data-row-kind="symlink"] {
    color: var(--text);
  }
  .col-perms {
    color: var(--dim);
    white-space: nowrap;
  }
  .col-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: flex;
    gap: 8px;
    min-width: 0;
  }
  .col-size {
    text-align: right;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .col-mtime {
    padding-left: 12px;
    color: var(--dim);
    white-space: nowrap;
  }
  .icon {
    flex: 0 0 auto;
    width: 10px;
  }
  /* A directory's whole label — glyph and name — is the disclosure target, so
     it advertises itself as clickable; files keep a plain text cursor. */
  .row[data-row-kind="dir"] .col-name {
    cursor: pointer;
  }
  .row[data-row-kind="dir"] .col-name:hover .twisty {
    color: var(--accent);
  }
  .twisty {
    border-radius: 2px;
  }
  .twisty.busy {
    opacity: 0.5;
  }
  .row .col-name {
    color: inherit;
  }
</style>
