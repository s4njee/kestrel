<!--
  +page.svelte — Application shell (dual-pane layout).

  Composes the toolbar, the resizable split of the local and remote panes, the
  transfer dock, and the status bar, and mounts the connect + host-key dialogs.
  Session events are initialized once on mount. The local pane still shows mock
  data and the remote pane is a placeholder until real browsing lands in E1-S10.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { ui } from "$lib/stores/ui.svelte";
  import { sessions } from "$lib/stores/sessions.svelte";
  import { initSessionEvents } from "$lib/ipc/events";
  import { disconnect as disconnectCmd, type SessionInfo } from "$lib/ipc/commands";
  import type { FileEntry } from "$lib/types";
  import Toolbar from "$lib/components/layout/Toolbar.svelte";
  import StatusBar from "$lib/components/layout/StatusBar.svelte";
  import SplitPane from "$lib/components/layout/SplitPane.svelte";
  import FilePane from "$lib/components/pane/FilePane.svelte";
  import TransferPanel from "$lib/components/transfers/TransferPanel.svelte";
  import ConnectDialog from "$lib/components/dialogs/ConnectDialog.svelte";
  import HostKeyDialog from "$lib/components/dialogs/HostKeyDialog.svelte";

  // Mock local entries so the shell has something to render before E1-S10.
  const localEntries: FileEntry[] = [
    { name: "Documents", kind: "dir", size: 0, mtime: 1_720_000_000, permissions: 0o755 },
    { name: "Downloads", kind: "dir", size: 0, mtime: 1_721_000_000, permissions: 0o755 },
    { name: "notes.txt", kind: "file", size: 2048, mtime: 1_721_500_000, permissions: 0o644 },
  ];

  let showConnect = $state(false);

  let active = $derived(sessions.active);
  let connected = $derived(active !== null);
  let connectionLabel = $derived(
    active ? `${active.info.username}@${active.info.host}` : "Not connected",
  );

  onMount(() => {
    // Guard: in a plain browser (dev preview without the Tauri runtime) the
    // channel setup throws; swallow it so the UI still renders.
    try {
      void initSessionEvents();
    } catch {
      /* no Tauri runtime */
    }
  });

  /** Toolbar Connect/Disconnect action. */
  async function onConnect(): Promise<void> {
    if (active) {
      const id = active.info.id;
      sessions.remove(id);
      await disconnectCmd(id);
    } else {
      showConnect = true;
    }
  }

  /** Track a newly connected session. */
  function onConnected(info: SessionInfo): void {
    sessions.add(info);
  }
</script>

<div class="app">
  <Toolbar {connected} {onConnect} />

  <main class="workspace">
    <SplitPane ratio={ui.splitRatio} onRatioChange={(r) => (ui.splitRatio = r)}>
      {#snippet left()}
        <FilePane
          kind="local"
          title="Local — ~/"
          entries={localEntries}
          active={ui.activePane === "local"}
          onActivate={() => ui.setActivePane("local")}
        />
      {/snippet}
      {#snippet right()}
        <FilePane
          kind="remote"
          title={active ? `Remote — ${active.info.host}` : "Remote — Not connected"}
          entries={[]}
          active={ui.activePane === "remote"}
          emptyMessage={active
            ? "Browsing arrives in the next step."
            : "Not connected. Use Connect… to open a session."}
          onActivate={() => ui.setActivePane("remote")}
        />
      {/snippet}
    </SplitPane>
  </main>

  <TransferPanel count={0} />
  <StatusBar {connectionLabel} transferCount={0} />
</div>

{#if showConnect}
  <ConnectDialog {onConnected} onClose={() => (showConnect = false)} />
{/if}
<HostKeyDialog />

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    width: 100vw;
    overflow: hidden;
  }
  .workspace {
    display: flex;
    flex: 1 1 auto;
    min-height: 0;
  }
</style>
