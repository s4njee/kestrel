<!--
  +page.svelte — Application shell (dual-pane layout).

  Composes the toolbar, the resizable split of the local and remote panes, the
  transfer dock, and the status bar, and mounts the connect + host-key dialogs.
  Drives real browsing: the local pane loads from the home directory on mount;
  the remote pane loads "/" on connect. Cmd/Ctrl+R refreshes the active pane and
  Cmd/Ctrl+L focuses its path field.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { ui } from "$lib/stores/ui.svelte";
  import { sessions } from "$lib/stores/sessions.svelte";
  import { transfers } from "$lib/stores/transfers.svelte";
  import { localPane, remotePane } from "$lib/stores/panes.svelte";
  import { initSessionEvents } from "$lib/ipc/events";
  import {
    disconnect as disconnectCmd,
    localHomeDir,
    localListDir,
    listDir,
    enqueueTransfers,
    type SessionInfo,
    type TransferRequest,
  } from "$lib/ipc/commands";
  import { buildTransferRequests } from "$lib/transfer";
  import { basename } from "$lib/utils/path";
  import Toolbar from "$lib/components/layout/Toolbar.svelte";
  import StatusBar from "$lib/components/layout/StatusBar.svelte";
  import SplitPane from "$lib/components/layout/SplitPane.svelte";
  import FilePane from "$lib/components/pane/FilePane.svelte";
  import TransferPanel from "$lib/components/transfers/TransferPanel.svelte";
  import ConnectDialog from "$lib/components/dialogs/ConnectDialog.svelte";
  import HostKeyDialog from "$lib/components/dialogs/HostKeyDialog.svelte";

  let showConnect = $state(false);

  let active = $derived(sessions.active);
  let connected = $derived(active !== null);
  let connectionLabel = $derived(
    active ? `${active.info.username}@${active.info.host}` : "Not connected",
  );

  /** Load a local directory into the local pane. */
  async function loadLocal(path: string): Promise<void> {
    localPane.startLoad(path);
    try {
      localPane.setEntries(await localListDir(path));
    } catch (e) {
      localPane.setError(String(e));
    }
  }

  /** Load a remote directory into the remote pane (needs an active session). */
  async function loadRemote(path: string): Promise<void> {
    const id = sessions.active?.info.id;
    if (!id) return;
    remotePane.startLoad(path);
    try {
      remotePane.setEntries(await listDir(id, path));
    } catch (e) {
      remotePane.setError(String(e));
    }
  }

  onMount(() => {
    // Guard: browser dev preview has no Tauri runtime.
    try {
      void initSessionEvents();
      localHomeDir()
        .then(loadLocal)
        .catch(() => {});
    } catch {
      /* no Tauri runtime */
    }
  });

  /** Toolbar Connect/Disconnect action. */
  async function onConnect(): Promise<void> {
    if (active) {
      const id = active.info.id;
      sessions.remove(id);
      remotePane.reset();
      await disconnectCmd(id);
    } else {
      showConnect = true;
    }
  }

  /** Track a newly connected session and load its root. */
  function onConnected(info: SessionInfo): void {
    sessions.add(info);
    void loadRemote("/");
    ui.setActivePane("remote");
  }

  let canUpload = $derived(connected && localPane.selected.size > 0);
  let canDownload = $derived(connected && remotePane.selected.size > 0);

  /** Download the remote pane's selected files into the local pane's folder. */
  async function download(): Promise<void> {
    const id = sessions.active?.info.id;
    if (!id) return;
    const requests = buildTransferRequests(
      "download",
      id,
      remotePane.selectedEntries,
      localPane.path,
    );
    await startTransfers(requests, "download");
  }

  /** Upload the local pane's selected files into the remote pane's folder. */
  async function upload(): Promise<void> {
    const id = sessions.active?.info.id;
    if (!id || !remotePane.path) return;
    const requests = buildTransferRequests(
      "upload",
      id,
      localPane.selectedEntries,
      remotePane.path,
    );
    await startTransfers(requests, "upload");
  }

  /** Enqueue requests, seed the transfers store, and reveal the dock. */
  async function startTransfers(
    requests: TransferRequest[],
    direction: "upload" | "download",
  ): Promise<void> {
    if (requests.length === 0) return;
    const ids = await enqueueTransfers(requests);
    ids.forEach((tid, i) =>
      transfers.add({
        id: tid,
        direction,
        name: basename(requests[i].dest),
        size: requests[i].size,
      }),
    );
    ui.setTransferPanelExpanded(true);
  }

  /** Refresh the active pane. */
  function refreshActive(): void {
    if (ui.activePane === "local") void loadLocal(localPane.path);
    else if (remotePane.path) void loadRemote(remotePane.path);
  }

  /** Global keyboard shortcuts. */
  function onGlobalKey(event: KeyboardEvent): void {
    const meta = event.metaKey || event.ctrlKey;
    if (!meta) return;
    if (event.key === "r") {
      event.preventDefault();
      refreshActive();
    } else if (event.key === "l") {
      event.preventDefault();
      document.getElementById(`path-input-${ui.activePane}`)?.focus();
    } else if (event.key === "d") {
      event.preventDefault();
      if (canDownload) void download();
    } else if (event.key === "u") {
      event.preventDefault();
      if (canUpload) void upload();
    }
  }
</script>

<svelte:window onkeydown={onGlobalKey} />

<div class="app">
  <Toolbar
    {connected}
    {canUpload}
    {canDownload}
    {onConnect}
    onUpload={upload}
    onDownload={download}
  />

  <main class="workspace">
    <SplitPane ratio={ui.splitRatio} onRatioChange={(r) => (ui.splitRatio = r)}>
      {#snippet left()}
        <FilePane
          pane={localPane}
          active={ui.activePane === "local"}
          onActivate={() => ui.setActivePane("local")}
          onNavigate={loadLocal}
        />
      {/snippet}
      {#snippet right()}
        <FilePane
          pane={remotePane}
          active={ui.activePane === "remote"}
          emptyMessage={connected
            ? "Empty directory."
            : "Not connected. Use Connect… to open a session."}
          onActivate={() => ui.setActivePane("remote")}
          onNavigate={loadRemote}
        />
      {/snippet}
    </SplitPane>
  </main>

  <TransferPanel />
  <StatusBar {connectionLabel} transferCount={transfers.activeCount} />
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
