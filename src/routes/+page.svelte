<!--
  +page.svelte — Application shell (dual-pane layout).

  Composes the top toolbar, the resizable split of the local and remote file
  panes, the bottom transfer dock, and the status bar. E0-S4 wires these with
  MOCK data and no IPC: the local pane shows sample rows, the remote pane shows
  a "Not connected" empty state. Real connections/browsing arrive in Epic 1.
-->
<script lang="ts">
  import { ui } from "$lib/stores/ui.svelte";
  import type { FileEntry } from "$lib/types";
  import Toolbar from "$lib/components/layout/Toolbar.svelte";
  import StatusBar from "$lib/components/layout/StatusBar.svelte";
  import SplitPane from "$lib/components/layout/SplitPane.svelte";
  import FilePane from "$lib/components/pane/FilePane.svelte";
  import TransferPanel from "$lib/components/transfers/TransferPanel.svelte";

  // Mock local entries so the shell has something to render before IPC lands.
  const localEntries: FileEntry[] = [
    { name: "Documents", kind: "dir", size: 0, mtime: 1_720_000_000, permissions: 0o755 },
    { name: "Downloads", kind: "dir", size: 0, mtime: 1_721_000_000, permissions: 0o755 },
    { name: "notes.txt", kind: "file", size: 2048, mtime: 1_721_500_000, permissions: 0o644 },
    {
      name: "archive.tar.gz",
      kind: "file",
      size: 15_728_640,
      mtime: 1_719_000_000,
      permissions: 0o644,
    },
    {
      name: "link-to-docs",
      kind: "symlink",
      size: 0,
      mtime: 1_720_500_000,
      permissions: 0o777,
      linkTarget: "Documents",
    },
  ];

  const remoteEntries: FileEntry[] = [];

  // Placeholder connection state until Epic 1 wires real sessions.
  let connected = $state(false);

  /** Placeholder Connect handler; opens the connect dialog in E1-S9. */
  function onConnect(): void {
    // Intentionally a no-op in the skeleton.
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
          title="Remote — Not connected"
          entries={remoteEntries}
          active={ui.activePane === "remote"}
          emptyMessage="Not connected. Use Connect… to open a session."
          onActivate={() => ui.setActivePane("remote")}
        />
      {/snippet}
    </SplitPane>
  </main>

  <TransferPanel count={0} />
  <StatusBar connectionLabel={connected ? "Connected" : "Not connected"} transferCount={0} />
</div>

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
