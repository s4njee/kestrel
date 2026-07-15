<!--
  SplitPane.svelte — Horizontal two-pane split with a draggable divider.

  Renders the `left` and `right` snippets side by side, sized by `ratio` (the
  left pane's fraction of the width). Dragging the divider reports new ratios
  through `onRatioChange`; the parent owns the value (typically ui.splitRatio)
  and feeds it back via the `ratio` prop, so there is a single source of truth.

  Props:
  - ratio: number              — left-pane width fraction (0..1).
  - left: Snippet              — content for the left pane.
  - right: Snippet             — content for the right pane.
  - onRatioChange?: (r) => void — called with the new fraction during a drag.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    ratio: number;
    left: Snippet;
    right: Snippet;
    onRatioChange?: (ratio: number) => void;
  }

  let { ratio, left, right, onRatioChange }: Props = $props();

  // The flex container whose width defines the drag coordinate space.
  let container: HTMLDivElement | undefined = $state();
  let dragging = $state(false);

  /**
   * Convert a pointer x-position into a left-pane fraction and report it.
   *
   * @param clientX - the pointer's viewport x-coordinate.
   */
  function updateFromPointer(clientX: number): void {
    if (!container) return;
    const rect = container.getBoundingClientRect();
    if (rect.width === 0) return;
    const fraction = (clientX - rect.left) / rect.width;
    onRatioChange?.(fraction);
  }

  /**
   * Begin a divider drag: capture the pointer and attach move/up listeners.
   *
   * @param event - the pointerdown event on the divider.
   */
  function onPointerDown(event: PointerEvent): void {
    event.preventDefault();
    dragging = true;
    (event.target as HTMLElement).setPointerCapture(event.pointerId);
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
  }

  /**
   * Handle a divider drag move.
   *
   * @param event - the pointermove event.
   */
  function onPointerMove(event: PointerEvent): void {
    if (dragging) updateFromPointer(event.clientX);
  }

  /** End a divider drag and detach the temporary listeners. */
  function onPointerUp(): void {
    dragging = false;
    window.removeEventListener("pointermove", onPointerMove);
    window.removeEventListener("pointerup", onPointerUp);
  }

  /**
   * Keyboard resize for accessibility: arrow keys nudge the divider.
   *
   * @param event - the keydown event on the divider.
   */
  function onKeyDown(event: KeyboardEvent): void {
    const step = 0.02;
    if (event.key === "ArrowLeft") onRatioChange?.(ratio - step);
    else if (event.key === "ArrowRight") onRatioChange?.(ratio + step);
    else return;
    event.preventDefault();
  }

  // CSS flex-basis percentages for the two panes.
  let leftPercent = $derived(`${(ratio * 100).toFixed(3)}%`);
</script>

<div class="split" bind:this={container} class:dragging>
  <div class="pane" style:flex-basis={leftPercent}>
    {@render left()}
  </div>
  <!-- The divider is an interactive resize handle: focusable, arrow-key
       operable, and exposed as a separator with aria-valuenow. Svelte's a11y
       lint does not model separators as interactive, so suppress here. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="divider"
    role="separator"
    aria-orientation="vertical"
    aria-valuenow={Math.round(ratio * 100)}
    tabindex="0"
    onpointerdown={onPointerDown}
    onkeydown={onKeyDown}
  ></div>
  <div class="pane pane-right">
    {@render right()}
  </div>
</div>

<style>
  .split {
    display: flex;
    flex: 1 1 auto;
    min-height: 0;
    width: 100%;
  }
  .split.dragging {
    cursor: col-resize;
    user-select: none;
  }
  .pane {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .pane-right {
    flex: 1 1 auto;
  }
  .divider {
    flex: 0 0 6px;
    cursor: col-resize;
    background-color: var(--border, #d0d0d0);
    transition: background-color 0.15s;
  }
  .divider:hover,
  .divider:focus-visible {
    background-color: var(--accent, #396cd8);
    outline: none;
  }
</style>
