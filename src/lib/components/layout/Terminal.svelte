<!--
  Terminal.svelte — Interactive SSH shell (real PTY) via xterm.js.

  Opens a shell on the active session and wires it both ways: keystrokes go to
  `shell_write`, the server's raw output is fed straight into xterm (it is ANSI —
  colours, cursor moves, clears — so it must be rendered by a terminal emulator,
  not printed as text). Resizes send SSH `window-change` so the remote side
  reflows. The shell is opened when a session appears and torn down when it goes
  away or the component unmounts.

  Props:
  - sessionId: string | null — the session to run the shell on; null closes it.
  - onCwd?: (cwd) => void    — the shell announced a working directory (OSC
    7/1337), used by the remote pane's [follow] mode.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import { openShell, shellWrite, shellResize, closeShell } from "$lib/ipc/commands";
  import { setShellHandlers } from "$lib/ipc/events";
  import { toasts } from "$lib/stores/toasts.svelte";
  import { CwdScanner } from "$lib/osc";

  interface Props {
    sessionId: string | null;
    /** Called when the shell announces a new working directory (OSC 7/1337). */
    onCwd?: (cwd: string) => void;
  }

  let { sessionId, onCwd }: Props = $props();

  /** Pulls cwd announcements out of the shell's output stream. */
  const cwdScanner = new CwdScanner();

  /** The element xterm renders into. */
  let host = $state<HTMLDivElement | null>(null);
  let term: Terminal | undefined;
  let fit: FitAddon | undefined;
  /** The open shell's id, or null when no shell is running. */
  let shellId: string | null = null;
  /** Guards against two concurrent opens for the same session. */
  let opening = false;

  // Terminal colours track the app's neutral palette (read from CSS tokens so
  // there is one source of truth for the theme).
  /**
   * Read a CSS custom property from the document root.
   *
   * @param name - the custom property name, e.g. "--bg".
   * @param fallback - value to use if the property is unset.
   * @returns the trimmed property value, or `fallback`.
   */
  function token(name: string, fallback: string): string {
    if (typeof getComputedStyle !== "function") return fallback;
    const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    return v || fallback;
  }

  /**
   * Fit the terminal to its container and tell the remote side the new size.
   */
  function refit(): void {
    if (!term || !fit) return;
    try {
      fit.fit();
    } catch {
      /* not laid out yet */
    }
    if (shellId) void shellResize(shellId, term.cols, term.rows).catch(() => {});
  }

  /**
   * Open a shell for the current session and attach it to the terminal.
   *
   * @param id - the session to run the shell on.
   */
  async function open(id: string): Promise<void> {
    if (!term || opening || shellId) return;
    opening = true;
    try {
      shellId = await openShell(id, term.cols, term.rows);
    } catch (e) {
      toasts.error(`Could not open a shell: ${String(e)}`);
      term.writeln(`\r\n\x1b[31mshell unavailable: ${String(e)}\x1b[0m`);
    } finally {
      opening = false;
    }
  }

  /** Close the running shell, if any. */
  function teardown(): void {
    if (shellId) {
      void closeShell(shellId).catch(() => {});
      shellId = null;
    }
  }

  onMount(() => {
    term = new Terminal({
      convertEol: false,
      cursorBlink: true,
      fontFamily: token("--mono", "monospace"),
      fontSize: 12,
      theme: {
        background: token("--con", "#0a0a0a"),
        foreground: token("--text", "#c9c9c9"),
        cursor: token("--accent", "#4ade80"),
        selectionBackground: "rgba(74, 222, 128, 0.25)",
      },
    });
    fit = new FitAddon();
    term.loadAddon(fit);
    if (host) term.open(host);
    refit();

    // Keystrokes → the remote shell (UTF-8 bytes; base64 on the wire).
    term.onData((data) => {
      if (shellId) void shellWrite(shellId, new TextEncoder().encode(data)).catch(() => {});
    });

    // Server output → the terminal, verbatim.
    setShellHandlers(
      (id, data) => {
        if (id !== shellId) return;
        term?.write(data);
        // The same bytes carry the shell's cwd announcements; reading them here
        // costs one scan and needs no server-side configuration.
        const cwd = cwdScanner.push(data);
        if (cwd !== null) onCwd?.(cwd);
      },
      (id) => {
        if (id !== shellId) return;
        shellId = null;
        cwdScanner.reset();
        term?.writeln("\r\n\x1b[2m[shell closed]\x1b[0m");
      },
    );

    const observer = new ResizeObserver(() => refit());
    if (host) observer.observe(host);

    // Open a shell for whatever session is already active.
    const initial = untrack(() => sessionId);
    if (initial) void open(initial);

    return () => {
      observer.disconnect();
      setShellHandlers(null, null);
      teardown();
      term?.dispose();
      term = undefined;
    };
  });

  // Follow the active session: open a shell when one connects, drop it when it
  // goes away or is replaced.
  $effect(() => {
    const id = sessionId;
    if (!term) return;
    if (!id) {
      untrack(teardown);
      return;
    }
    untrack(() => {
      if (!shellId) void open(id);
    });
  });
</script>

<div class="terminal" bind:this={host}></div>

<style>
  .terminal {
    width: 100%;
    height: 100%;
    background: var(--con);
    padding: 4px 8px;
    overflow: hidden;
  }
  /* xterm renders into a child canvas/textarea; keep it flush with the region. */
  .terminal :global(.xterm) {
    height: 100%;
  }
  .terminal :global(.xterm-viewport) {
    background: transparent !important;
    scrollbar-width: thin;
  }
</style>
