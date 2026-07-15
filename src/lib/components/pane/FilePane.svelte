<!--
  FilePane.svelte — One file browser pane (source-agnostic).

  Used for both the local and remote panes; the parent supplies the title,
  entries, and connection state. E0-S4 renders a static header + column headers
  + rows (or an empty state) from mock data; navigation, sorting, selection and
  virtualization arrive in E1-S10. Clicking anywhere activates the pane.

  Props:
  - kind: PaneKind             — "local" or "remote" (identifies the pane).
  - title: string              — header text (e.g. a path or "Not connected").
  - entries: FileEntry[]       — rows to display (may be empty).
  - active: boolean            — whether this pane is currently active.
  - emptyMessage?: string      — text shown when there are no entries.
  - onActivate?: () => void    — called when the pane is clicked/focused.
-->
<script lang="ts">
  import type { FileEntry, PaneKind } from "$lib/types";
  import { formatBytes, formatMtime } from "$lib/utils/format";

  interface Props {
    kind: PaneKind;
    title: string;
    entries: FileEntry[];
    active: boolean;
    emptyMessage?: string;
    onActivate?: () => void;
  }

  let { kind, title, entries, active, emptyMessage = "Empty", onActivate }: Props = $props();

  /**
   * Render a Unix mode as an rwx string, or a placeholder when unknown.
   *
   * @param mode - a Unix permission mode (e.g. 0o644) or null.
   * @returns a 9-char rwx string like "rw-r--r--", or "—" when null.
   */
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

  /**
   * Icon glyph for an entry kind.
   *
   * @param entry - the file entry.
   * @returns a short emoji/text glyph.
   */
  function icon(entry: FileEntry): string {
    if (entry.kind === "dir") return "📁";
    if (entry.kind === "symlink") return "🔗";
    return "📄";
  }
</script>

<section
  class="pane"
  class:active
  data-kind={kind}
  aria-label={`${kind} pane`}
  onpointerdown={() => onActivate?.()}
  onfocusin={() => onActivate?.()}
>
  <header class="pane-header" {title}>{title}</header>

  <div class="pane-columns" aria-hidden="true">
    <span class="col-name">Name</span>
    <span class="col-size">Size</span>
    <span class="col-mtime">Modified</span>
    <span class="col-perms">Perms</span>
  </div>

  <div class="pane-body">
    {#if entries.length === 0}
      <p class="empty">{emptyMessage}</p>
    {:else}
      <ul class="rows">
        {#each entries as entry (entry.name)}
          <li class="row">
            <span class="col-name"><span class="icon">{icon(entry)}</span>{entry.name}</span>
            <span class="col-size">{entry.kind === "dir" ? "—" : formatBytes(entry.size)}</span>
            <span class="col-mtime">{formatMtime(entry.mtime)}</span>
            <span class="col-perms">{permString(entry.permissions)}</span>
          </li>
        {/each}
      </ul>
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
    background: var(--surface, #ffffff);
  }
  .pane.active {
    outline: 2px solid var(--accent, #396cd8);
    outline-offset: -1px;
  }
  .pane-header {
    padding: 6px 10px;
    font-size: 0.8rem;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    background: var(--surface-2, #f2f2f2);
    border-bottom: 1px solid var(--border, #d0d0d0);
  }
  .pane-columns,
  .row {
    display: grid;
    grid-template-columns: 1fr 90px 150px 100px;
    gap: 8px;
    padding: 3px 10px;
    font-size: 0.8rem;
  }
  .pane-columns {
    font-weight: 600;
    color: var(--muted, #666);
    border-bottom: 1px solid var(--border, #d0d0d0);
  }
  .pane-body {
    flex: 1 1 auto;
    overflow: auto;
    min-height: 0;
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row:hover {
    background: var(--surface-2, #f2f2f2);
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
    color: var(--muted, #666);
  }
  .col-mtime {
    color: var(--muted, #666);
  }
  .icon {
    margin-right: 6px;
  }
  .empty {
    padding: 24px;
    text-align: center;
    color: var(--muted, #888);
    font-size: 0.85rem;
  }
</style>
