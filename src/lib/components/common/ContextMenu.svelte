<!--
  ContextMenu.svelte — Positioned right-click menu.

  Renders a list of items at (x, y). Clicking an item runs its action and
  closes; clicking elsewhere or pressing Escape closes without acting. A
  `separator` item renders a divider.

  Props:
  - x, y: number                 — viewport position.
  - items: MenuItem[]            — the menu entries.
  - onClose: () => void          — close the menu.
-->
<script lang="ts">
  export interface MenuItem {
    label?: string;
    action?: () => void;
    disabled?: boolean;
    separator?: boolean;
  }

  interface Props {
    x: number;
    y: number;
    items: MenuItem[];
    onClose: () => void;
  }

  let { x, y, items, onClose }: Props = $props();

  function choose(item: MenuItem): void {
    if (item.disabled || item.separator) return;
    onClose();
    item.action?.();
  }

  function onKey(event: KeyboardEvent): void {
    if (event.key === "Escape") onClose();
  }
</script>

<svelte:window onkeydown={onKey} />

<div
  class="overlay"
  role="presentation"
  onpointerdown={onClose}
  oncontextmenu={(e) => e.preventDefault()}
>
  <ul
    class="menu"
    role="menu"
    style:left="{x}px"
    style:top="{y}px"
    onpointerdown={(e) => e.stopPropagation()}
  >
    {#each items as item, i (i)}
      {#if item.separator}
        <li class="sep" role="separator"></li>
      {:else}
        <li role="none">
          <button role="menuitem" disabled={item.disabled} onclick={() => choose(item)}>
            {item.label}
          </button>
        </li>
      {/if}
    {/each}
  </ul>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 200;
  }
  .menu {
    position: fixed;
    min-width: 180px;
    margin: 0;
    padding: 4px;
    list-style: none;
    background: var(--surface, #fff);
    border: 1px solid var(--border, #d0d0d0);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.2);
  }
  .menu button {
    display: block;
    width: 100%;
    text-align: left;
    padding: 5px 10px;
    border: none;
    background: none;
    border-radius: 5px;
    font-size: 0.82rem;
    color: inherit;
    cursor: pointer;
  }
  .menu button:hover:not(:disabled) {
    background: var(--accent, #396cd8);
    color: #fff;
  }
  .menu button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .sep {
    height: 1px;
    margin: 4px 0;
    background: var(--border, #e0e0e0);
  }
</style>
