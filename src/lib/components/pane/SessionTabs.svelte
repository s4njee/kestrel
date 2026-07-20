<!--
  SessionTabs.svelte — Host tabs above the remote pane (E8-S9).

  One terminal-grid tab per connected session, plus a `[+]` that opens the
  connect dialog. Each tab shows `user@host` and carries its own `×` to close
  just that session — closing a tab disconnects one host, not the application.

  A reconnecting session is marked rather than removed, so a tab does not vanish
  under the user while the supervisor is retrying.

  The strip renders only when there is a session; a single session still gets a
  tab, because the `[+]` is how a second one is opened.

  Props:
  - entries: SessionEntry[]              — the tracked sessions, in connect order.
  - activeId: string | null              — which tab is current.
  - onSelect: (id: string) => void       — switch to a session.
  - onClose: (id: string) => void        — disconnect one session.
  - onNew: () => void                    — open the connect dialog.
-->
<script lang="ts">
  import type { SessionEntry } from "$lib/stores/sessions.svelte";

  interface Props {
    entries: SessionEntry[];
    activeId: string | null;
    onSelect: (id: string) => void;
    onClose: (id: string) => void;
    onNew: () => void;
  }

  let { entries, activeId, onSelect, onClose, onNew }: Props = $props();

  /**
   * The label for one tab.
   *
   * @param entry - the session entry.
   * @returns `user@host`, the shortest form that still distinguishes two
   *   sessions to the same host as different users.
   */
  function label(entry: SessionEntry): string {
    return `${entry.info.username}@${entry.info.host}`;
  }
</script>

{#if entries.length > 0}
  <div class="tabs" role="tablist" aria-label="Sessions">
    {#each entries as entry (entry.info.id)}
      {@const id = entry.info.id}
      <div class="tab" class:active={id === activeId} class:pending={entry.state !== "connected"}>
        <button
          type="button"
          role="tab"
          aria-selected={id === activeId}
          class="pick"
          title={`${label(entry)}:${entry.info.port}`}
          onclick={() => onSelect(id)}
        >
          {label(entry)}{#if entry.state === "reconnecting"}<span class="state"> ⟳</span>{/if}
        </button>
        <button
          type="button"
          class="close"
          aria-label={`Disconnect ${label(entry)}`}
          title="Disconnect this session"
          onclick={() => onClose(id)}>×</button
        >
      </div>
    {/each}
    <button type="button" class="new" aria-label="Connect to another host" onclick={onNew}>
      [+]
    </button>
  </div>
{/if}

<style>
  .tabs {
    display: flex;
    align-items: stretch;
    gap: 2px;
    padding: 0 6px;
    background: var(--surface);
    border-bottom: 1px solid var(--grid);
    overflow-x: auto;
    flex: 0 0 auto;
  }
  .tab {
    display: flex;
    align-items: center;
    border-bottom: 2px solid transparent;
    flex: 0 0 auto;
  }
  /* The current tab is the one place in the strip that takes the accent. */
  .tab.active {
    border-bottom-color: var(--accent);
  }
  .tab.pending {
    opacity: 0.6;
  }
  .pick {
    background: none;
    border: none;
    font-family: inherit;
    font-size: 11.5px;
    color: var(--muted);
    padding: 5px 4px 5px 8px;
    cursor: pointer;
    white-space: nowrap;
  }
  .tab.active .pick {
    color: var(--bright);
  }
  .pick:hover {
    color: var(--text);
  }
  .state {
    color: var(--warn);
  }
  .close {
    background: none;
    border: none;
    font-family: inherit;
    font-size: 12px;
    color: var(--dim);
    padding: 5px 8px 5px 4px;
    cursor: pointer;
  }
  .close:hover {
    color: var(--danger);
  }
  .new {
    background: none;
    border: none;
    font-family: inherit;
    font-size: 11.5px;
    color: var(--dim);
    padding: 5px 8px;
    cursor: pointer;
    flex: 0 0 auto;
  }
  .new:hover {
    color: var(--accent);
  }
</style>
