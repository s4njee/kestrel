<!--
  Breadcrumbs.svelte — Path navigation for a pane.

  Shows clickable path segments plus an editable address bar. Clicking a segment
  or pressing Enter in the address bar navigates. The input carries a stable id
  (`path-input-<kind>`) so the shell can focus it for the Cmd/Ctrl+L shortcut.

  Props:
  - path: string                 — current directory path.
  - kind: PaneKind               — identifies the pane (for the input id).
  - onNavigate: (path) => void   — called with the target path.
-->
<script lang="ts">
  import type { PaneKind } from "$lib/types";
  import { pathSegments } from "$lib/utils/path";

  interface Props {
    path: string;
    kind: PaneKind;
    onNavigate: (path: string) => void;
  }

  let { path, kind, onNavigate }: Props = $props();

  // Writable derived: resets to `path` when navigation changes it, but the user
  // can still type into it (reassignments hold until the next path change).
  let draft = $derived(path);

  let segments = $derived(pathSegments(path));

  function onKeyDown(event: KeyboardEvent): void {
    if (event.key === "Enter" && draft.trim()) onNavigate(draft.trim());
    else if (event.key === "Escape") draft = path;
  }
</script>

<div class="breadcrumbs">
  <nav class="crumbs" aria-label="path">
    {#each segments as seg (seg.path)}
      <button class="crumb" onclick={() => onNavigate(seg.path)}>{seg.label}</button>
    {/each}
  </nav>
  <input
    id={`path-input-${kind}`}
    class="address"
    bind:value={draft}
    onkeydown={onKeyDown}
    aria-label={`${kind} path`}
    spellcheck="false"
    autocomplete="off"
  />
</div>

<style>
  .breadcrumbs {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border, #d0d0d0);
    background: var(--surface-2, #f2f2f2);
  }
  .crumbs {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 1px;
    font-size: 0.78rem;
  }
  .crumb {
    background: none;
    border: none;
    padding: 1px 4px;
    border-radius: 4px;
    cursor: pointer;
    color: var(--accent, #396cd8);
  }
  .crumb:hover {
    background: var(--surface, #fff);
  }
  .crumb:not(:last-child)::after {
    content: "›";
    margin-left: 4px;
    color: var(--muted, #999);
  }
  .address {
    font-size: 0.78rem;
    padding: 3px 6px;
    border: 1px solid var(--border, #c4c4c4);
    border-radius: 5px;
    background: var(--surface, #fff);
    color: inherit;
  }
</style>
