<!--
  StatusBar.svelte — Bottom console region: interactive shell + session log.

  Two tabs over one resizable region:
  - **shell** — a real interactive SSH shell (PTY) on the active session, via
    Terminal.svelte. This is a genuine terminal, not a transcript.
  - **log** — the tagged session stream (Status/Command/Response) from the logs
    store, with a blinking prompt line. Read-only.

  Both stay mounted while connected so switching tabs never kills the running
  shell or loses scrollback; the inactive one is just hidden.

  The region is drag-resizable by its top edge (the height lives in the ui store
  and is persisted); the terminal reflows automatically because Terminal.svelte
  observes its container.

  Props:
  - onCwd?: (cwd) => void     — forwarded from Terminal: the shell announced a
    working directory. The `[follow]` toggle beside the tabs decides whether the
    shell drives the remote pane; the shell itself is never written to.
  - connectionLabel: string   — e.g. "not connected" or "user@host".
  - transferCount: number     — active transfers (shown on the log's prompt).
  - sessionId: string | null  — session the shell attaches to; null = no shell.
-->
<script lang="ts">
  import { logs } from "$lib/stores/logs.svelte";
  import { ui } from "$lib/stores/ui.svelte";
  import Terminal from "./Terminal.svelte";

  interface Props {
    connectionLabel: string;
    transferCount: number;
    sessionId: string | null;
    onCwd?: (cwd: string) => void;
  }

  let { connectionLabel, transferCount, sessionId, onCwd }: Props = $props();

  /** Which tab is showing. */
  let tab = $state<"shell" | "log">("shell");

  let logEl = $state<HTMLDivElement | null>(null);

  // The log's prompt reflects the active connection (or a local shell when idle).
  let prompt = $derived(connectionLabel === "not connected" ? "local" : connectionLabel);

  // Auto-scroll the log to the newest line whenever it grows.
  $effect(() => {
    // Touch the length so the effect re-runs on append.
    void logs.lines.length;
    if (logEl) logEl.scrollTop = logEl.scrollHeight;
  });

  /** True while the top edge is being dragged. */
  let resizing = $state(false);
  /** Pointer y and region height captured at drag start. */
  let dragStartY = 0;
  let dragStartHeight = 0;

  /**
   * Begin a resize drag: capture the pointer and the starting geometry.
   *
   * @param event - the pointerdown event on the grip.
   */
  function onGripPointerDown(event: PointerEvent): void {
    event.preventDefault();
    resizing = true;
    dragStartY = event.clientY;
    dragStartHeight = ui.consoleHeight;
    (event.target as HTMLElement).setPointerCapture(event.pointerId);
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
  }

  /**
   * Resize while dragging: moving the grip up (negative dy) grows the console.
   *
   * @param event - the pointermove event.
   */
  function onPointerMove(event: PointerEvent): void {
    if (!resizing) return;
    ui.consoleHeight = dragStartHeight + (dragStartY - event.clientY);
  }

  /** End a resize drag and detach the temporary listeners. */
  function onPointerUp(): void {
    resizing = false;
    window.removeEventListener("pointermove", onPointerMove);
    window.removeEventListener("pointerup", onPointerUp);
  }

  /**
   * Keyboard resize for accessibility: Up/Down nudge the console taller/shorter.
   *
   * @param event - the keydown event on the grip.
   */
  function onGripKeyDown(event: KeyboardEvent): void {
    const step = event.shiftKey ? 40 : 12;
    if (event.key === "ArrowUp") ui.consoleHeight = ui.consoleHeight + step;
    else if (event.key === "ArrowDown") ui.consoleHeight = ui.consoleHeight - step;
    else return;
    event.preventDefault();
  }
</script>

<section class="console-region" class:resizing style:height="{ui.consoleHeight}px">
  <!-- Drag handle: a horizontal separator, focusable and arrow-key operable.
       Svelte's a11y lint does not model separators as interactive. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="grip"
    role="separator"
    aria-orientation="horizontal"
    aria-label="Resize console"
    aria-valuenow={Math.round(ui.consoleHeight)}
    tabindex="0"
    onpointerdown={onGripPointerDown}
    onkeydown={onGripKeyDown}
  ></div>
  <div class="tabs">
    <button class="tab" class:active={tab === "shell"} onclick={() => (tab = "shell")}>
      [shell]
    </button>
    <button class="tab" class:active={tab === "log"} onclick={() => (tab = "log")}>[log]</button>
    {#if sessionId}
      <button
        class="tab follow"
        class:active={ui.followShellCwd}
        title="Follow the shell's directory in the remote pane"
        onclick={() => ui.toggleFollowShellCwd()}
      >
        [{ui.followShellCwd ? "✓" : ""}follow]
      </button>
    {/if}
    <span class="spacer"></span>
    {#if transferCount > 0}<span class="jobs">{transferCount} jobs</span>{/if}
  </div>

  <div class="pane" class:hidden={tab !== "shell"}>
    {#if sessionId}
      <Terminal {sessionId} {onCwd} />
    {:else}
      <p class="hint">not connected — use [connect] to open a shell</p>
    {/if}
  </div>

  <div class="pane log" class:hidden={tab !== "log"} bind:this={logEl}>
    {#each logs.lines as line (line.id)}
      <div class="log-line {line.cls}"><span class="t">{line.tag}</span> {line.text}</div>
    {/each}
    <div class="prompt-line">
      <span class="who">{prompt}</span>:~$
      <span class="caret">&nbsp;</span>
    </div>
  </div>
</section>

<style>
  .console-region {
    /* Height is user-adjustable (dragged by .grip) and lives in the ui store;
       the store clamps it so the file panes always keep room. */
    --console-font: 11.5px;
    --console-leading: 1.55;
    --console-pad: 8px;
    --tabs-h: 22px;

    position: relative;
    display: flex;
    flex-direction: column;
    flex: 0 0 auto;
    background: var(--con);
    border-top: 1px solid var(--border);
    font-size: var(--console-font);
    line-height: var(--console-leading);
  }
  /* A slim grab strip sitting on the top border. */
  .grip {
    position: absolute;
    top: -3px;
    left: 0;
    right: 0;
    height: 6px;
    cursor: ns-resize;
    z-index: 2;
  }
  .grip:hover,
  .grip:focus-visible,
  .console-region.resizing .grip {
    background: var(--accent);
    outline: none;
  }
  /* Don't let a fast drag select text or fight the pointer. */
  .console-region.resizing {
    user-select: none;
  }
  .tabs {
    display: flex;
    align-items: center;
    gap: 10px;
    height: var(--tabs-h);
    padding: 0 14px;
    border-bottom: 1px solid var(--grid);
    flex: 0 0 auto;
  }
  .tab {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    font-size: 11px;
    color: var(--dim);
  }
  .tab:hover {
    color: var(--muted);
  }
  .tab.active {
    color: var(--accent);
  }
  .spacer {
    flex: 1 1 auto;
  }
  .jobs {
    color: var(--dim);
    font-size: 11px;
  }
  .pane {
    flex: 1 1 auto;
    min-height: 0;
  }
  .pane.hidden {
    display: none;
  }
  .log {
    overflow: auto;
    padding: var(--console-pad) 14px;
  }
  .hint {
    margin: 0;
    padding: var(--console-pad) 14px;
    color: var(--dim);
  }
  .log-line .t {
    color: var(--dim);
    margin-right: 4px;
  }
  .log-line.info {
    color: var(--muted);
  }
  /* Success is the only accented log class. */
  .log-line.ok {
    color: var(--accent);
  }
  .log-line.cmd {
    color: var(--text);
  }
  .log-line.resp {
    color: var(--muted);
  }
  .log-line.err {
    color: var(--danger);
  }
  .prompt-line {
    color: var(--dim);
    margin-top: 2px;
  }
  .who {
    color: var(--bright);
  }
  .caret {
    background: var(--accent);
    color: var(--con);
    animation: blink 1.1s steps(1) infinite;
  }
  @keyframes blink {
    50% {
      background: transparent;
    }
  }
</style>
