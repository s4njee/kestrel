<!--
  Breadcrumbs.svelte — Terminal command header for a pane.

  Renders a shell-style prompt line — `local:~$ ls -la <path>` — where the path
  is an inline editable field: pressing Enter navigates, Escape reverts. The
  field keeps a stable id (`path-input-<kind>`) so the shell can focus it for the
  Cmd/Ctrl+L shortcut. Up-navigation is via the `..` row in the file grid.

  Props:
  - path: string                 — current directory path.
  - kind: PaneKind               — identifies the pane (label + input id).
  - onNavigate: (path) => void   — called with the target path.
-->
<script lang="ts">
  import type { PaneKind } from "$lib/types";

  interface Props {
    path: string;
    kind: PaneKind;
    onNavigate: (path: string) => void;
  }

  let { path, kind, onNavigate }: Props = $props();

  // Writable derived: resets to `path` when navigation changes it, but the user
  // can still type into it (reassignments hold until the next path change).
  let draft = $derived(path);

  /**
   * Handle path-field keys: Enter navigates to the typed path, Escape reverts.
   *
   * @param event - the keydown event.
   */
  function onKeyDown(event: KeyboardEvent): void {
    if (event.key === "Enter" && draft.trim()) onNavigate(draft.trim());
    else if (event.key === "Escape") draft = path;
  }
</script>

<div class="cmd">
  <span class="prompt">{kind}:~$</span>
  <span class="verb">ls -la</span>
  <input
    id={`path-input-${kind}`}
    class="path"
    bind:value={draft}
    onkeydown={onKeyDown}
    aria-label={`${kind} path`}
    spellcheck="false"
    autocomplete="off"
  />
</div>

<style>
  .cmd {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: var(--surface);
    border-bottom: 1px solid var(--grid);
    font-size: 12px;
    min-width: 0;
  }
  .prompt {
    color: var(--dim);
    flex: 0 0 auto;
  }
  .verb {
    color: var(--muted);
    flex: 0 0 auto;
  }
  .path {
    flex: 1 1 auto;
    min-width: 0;
    background: none;
    border: none;
    outline: none;
    padding: 1px 2px;
    color: var(--bright);
    font-size: 12px;
  }
  .path:focus {
    border-bottom: 1px solid var(--accent);
  }
</style>
