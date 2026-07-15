<!--
  BookmarkManager.svelte — Saved-connection list and CRUD.

  Doubles as the remote pane's "not connected" content: lists saved bookmarks,
  connects on double-click (or Enter), and offers add/edit/delete. Add and edit
  are delegated to the parent (which opens the ConnectDialog prefilled); delete
  is handled here via the bookmarks store.

  Props:
  - onConnect: (b: Bookmark) => void — connect using a bookmark.
  - onEdit: (b: Bookmark) => void    — edit a bookmark (opens the dialog).
  - onAdd: () => void                — start a new bookmark (opens the dialog).
-->
<script lang="ts">
  import { bookmarks } from "$lib/stores/bookmarks.svelte";
  import type { Bookmark } from "$lib/ipc/commands";

  interface Props {
    onConnect: (b: Bookmark) => void;
    onEdit: (b: Bookmark) => void;
    onAdd: () => void;
  }

  let { onConnect, onEdit, onAdd }: Props = $props();

  /** Human label for a bookmark's auth method. */
  function methodLabel(b: Bookmark): string {
    switch (b.authMethod) {
      case "password":
        return "Password";
      case "key":
        return "Key";
      case "agent":
        return "Agent";
      case "keyboardInteractive":
        return "Interactive";
    }
  }

  /** Delete a bookmark after confirmation. */
  async function remove(b: Bookmark): Promise<void> {
    if (confirm(`Delete bookmark "${b.name}"?`)) await bookmarks.remove(b.id);
  }
</script>

<section class="bookmarks" aria-label="Bookmarks">
  <header>
    <span class="title">Bookmarks</span>
    <button type="button" class="add" onclick={onAdd}>Add…</button>
  </header>

  {#if bookmarks.items.length === 0}
    <p class="empty">No saved connections. Use <strong>Add…</strong> to create one.</p>
  {:else}
    <ul>
      {#each bookmarks.items as b (b.id)}
        <li>
          <button
            type="button"
            class="row"
            ondblclick={() => onConnect(b)}
            onkeydown={(e) => {
              if (e.key === "Enter") onConnect(b);
            }}
          >
            <span class="name">{b.name}</span>
            <span class="detail">{b.username}@{b.host}:{b.port}</span>
            <span class="method">{methodLabel(b)}</span>
          </button>
          <div class="actions">
            <button type="button" onclick={() => onConnect(b)}>Connect</button>
            <button type="button" onclick={() => onEdit(b)}>Edit</button>
            <button type="button" class="danger" onclick={() => remove(b)}>Delete</button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .bookmarks {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
    overflow: auto;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border, #d0d0d0);
  }
  .title {
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--muted, #666);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .empty {
    padding: 24px;
    text-align: center;
    color: var(--muted, #888);
    font-size: 0.85rem;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border, #eee);
  }
  .row {
    flex: 1 1 auto;
    display: grid;
    grid-template-columns: 1fr auto;
    grid-template-areas: "name method" "detail method";
    gap: 0 10px;
    text-align: left;
    background: none;
    border: none;
    padding: 4px 6px;
    border-radius: 6px;
    cursor: pointer;
    color: inherit;
  }
  .row:hover {
    background: var(--surface-2, #f2f2f2);
  }
  .name {
    grid-area: name;
    font-weight: 600;
    font-size: 0.9rem;
  }
  .detail {
    grid-area: detail;
    font-size: 0.78rem;
    color: var(--muted, #777);
  }
  .method {
    grid-area: method;
    align-self: center;
    font-size: 0.72rem;
    color: var(--muted, #777);
    border: 1px solid var(--border, #d0d0d0);
    border-radius: 10px;
    padding: 1px 8px;
  }
  .actions {
    display: flex;
    gap: 4px;
    flex: 0 0 auto;
  }
  .actions button {
    font-size: 0.75rem;
    padding: 3px 8px;
    border: 1px solid var(--border, #c4c4c4);
    border-radius: 6px;
    background: var(--surface, #fff);
    cursor: pointer;
    color: inherit;
  }
  .add {
    font-size: 0.75rem;
    padding: 3px 10px;
    border: 1px solid var(--border, #c4c4c4);
    border-radius: 6px;
    background: var(--surface, #fff);
    cursor: pointer;
    color: inherit;
  }
  .danger {
    color: #c0392b;
  }
</style>
