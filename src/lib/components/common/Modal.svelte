<!--
  Modal.svelte — Centered modal dialog chrome.

  Renders an overlay + titled panel around `children`. Clicking the backdrop or
  pressing Escape calls `onClose`. Presentational only.

  Props:
  - title: string           — dialog heading / aria-label.
  - onClose?: () => void     — invoked on backdrop click or Escape.
  - children: Snippet        — dialog body content.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    onClose?: () => void;
    children: Snippet;
  }

  let { title, onClose, children }: Props = $props();

  /**
   * Close when Escape is pressed.
   *
   * @param event - the window keydown event.
   */
  function onKeyDown(event: KeyboardEvent): void {
    if (event.key === "Escape") onClose?.();
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<div
  class="overlay"
  role="presentation"
  onpointerdown={(e) => {
    if (e.target === e.currentTarget) onClose?.();
  }}
>
  <div class="modal" role="dialog" aria-modal="true" aria-label={title}>
    <header class="modal-header">{title}</header>
    <div class="modal-body">
      {@render children()}
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.45);
    z-index: 100;
  }
  .modal {
    width: min(460px, 92vw);
    max-height: 90vh;
    overflow: auto;
    background: var(--surface, #fff);
    border: 1px solid var(--border, #d0d0d0);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.3);
  }
  .modal-header {
    padding: 12px 16px;
    font-weight: 700;
    border-bottom: 1px solid var(--border, #d0d0d0);
  }
  .modal-body {
    padding: 16px;
  }
</style>
