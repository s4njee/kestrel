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
  import { prompts } from "$lib/stores/prompts.svelte";
  import { bookmarks } from "$lib/stores/bookmarks.svelte";
  import { localPane, remotePane } from "$lib/stores/panes.svelte";
  import { initSessionEvents, setLocalDirChangedHandler } from "$lib/ipc/events";
  import { respondPrompt } from "$lib/ipc/commands";
  import {
    disconnect as disconnectCmd,
    connectBookmark,
    localHomeDir,
    localListDir,
    watchLocalDir,
    listDir,
    enqueueTransfers,
    enqueueDirectory,
    renameEntry,
    deleteEntries,
    makeDir,
    setPermissions,
    type Bookmark,
    type SessionInfo,
    type TransferDirection,
  } from "$lib/ipc/commands";
  import type { DirEntry } from "$lib/ipc/commands";
  import type { PaneKind } from "$lib/types";
  import { buildTransferRequests, dropDirection, uploadRequestsForPaths } from "$lib/transfer";
  import { parentPath, joinPath } from "$lib/utils/path";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import ContextMenu, { type MenuItem } from "$lib/components/common/ContextMenu.svelte";
  import PermissionsDialog from "$lib/components/dialogs/PermissionsDialog.svelte";
  import DeleteConfirmDialog from "$lib/components/dialogs/DeleteConfirmDialog.svelte";
  import InputDialog from "$lib/components/dialogs/InputDialog.svelte";
  import Toolbar from "$lib/components/layout/Toolbar.svelte";
  import StatusBar from "$lib/components/layout/StatusBar.svelte";
  import SplitPane from "$lib/components/layout/SplitPane.svelte";
  import FilePane from "$lib/components/pane/FilePane.svelte";
  import TransferPanel from "$lib/components/transfers/TransferPanel.svelte";
  import ConnectDialog from "$lib/components/dialogs/ConnectDialog.svelte";
  import BookmarkManager from "$lib/components/dialogs/BookmarkManager.svelte";
  import HostKeyDialog from "$lib/components/dialogs/HostKeyDialog.svelte";
  import ConflictDialog from "$lib/components/dialogs/ConflictDialog.svelte";
  import PromptDialog from "$lib/components/dialogs/PromptDialog.svelte";

  let showConnect = $state(false);
  // The bookmark prefilling the connect dialog: null = a fresh connection.
  let connectSeed = $state<Bookmark | null>(null);

  /** Open the connect dialog, optionally prefilled from a bookmark. */
  function openConnect(seed: Bookmark | null = null): void {
    connectSeed = seed;
    showConnect = true;
  }

  let active = $derived(sessions.active);
  let connected = $derived(active !== null);
  let connectionLabel = $derived(
    active ? `${active.info.username}@${active.info.host}` : "Not connected",
  );
  let remoteBanner = $derived(
    active?.state === "reconnecting" ? "Connection lost — reconnecting…" : null,
  );

  /** Load a local directory into the local pane, and watch it for changes. */
  async function loadLocal(path: string): Promise<void> {
    localPane.startLoad(path);
    try {
      localPane.setEntries(await localListDir(path));
      // Retarget the FS watcher onto the now-visible directory.
      void watchLocalDir(path).catch(() => {});
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

  let unlistenDrop: (() => void) | undefined;

  onMount(() => {
    // Guard: browser dev preview has no Tauri runtime.
    try {
      void initSessionEvents();
      // Auto-refresh the local pane when its directory changes externally.
      setLocalDirChangedHandler((path) => {
        if (localPane.path === path) void loadLocal(path);
      });
      bookmarks.load().catch(() => {});
      localHomeDir()
        .then(loadLocal)
        .catch(() => {});
      // OS file drops onto the window upload to the remote pane.
      getCurrentWebview()
        .onDragDropEvent((event) => {
          if (event.payload.type === "drop") void onOsDrop(event.payload.paths);
        })
        .then((un) => (unlistenDrop = un))
        .catch(() => {});
    } catch {
      /* no Tauri runtime */
    }
    return () => {
      setLocalDirChangedHandler(null);
      unlistenDrop?.();
    };
  });

  /** Enqueue uploads for OS-dropped local file paths (to the remote pane). */
  async function onOsDrop(paths: string[]): Promise<void> {
    const id = sessions.active?.info.id;
    if (!id || !remotePane.path || paths.length === 0) return;
    await enqueueTransfers(uploadRequestsForPaths(id, paths, remotePane.path));
    ui.setTransferPanelExpanded(true);
  }

  /** Handle a cross-pane drag drop. */
  async function onPaneDrop(source: PaneKind, target: PaneKind): Promise<void> {
    const id = sessions.active?.info.id;
    if (!id) return;
    const dir = dropDirection(source, target);
    if (!dir) return;
    const sourcePane = source === "local" ? localPane : remotePane;
    const targetPane = target === "local" ? localPane : remotePane;
    await runTransfers(dir, id, sourcePane.selectedEntries, targetPane.path);
  }

  /** Toolbar Connect/Disconnect action. */
  async function onConnect(): Promise<void> {
    if (active) {
      const id = active.info.id;
      sessions.remove(id);
      remotePane.reset();
      await disconnectCmd(id);
    } else {
      openConnect(null);
    }
  }

  /** Track a newly connected session and load its root. */
  function onConnected(info: SessionInfo): void {
    sessions.add(info);
    void loadRemote("/");
    ui.setActivePane("remote");
  }

  /**
   * Connect using a saved bookmark. If the backend can't (e.g. no saved
   * password), fall back to the connect dialog prefilled from the bookmark.
   */
  async function connectFromBookmark(bookmark: Bookmark): Promise<void> {
    try {
      onConnected(await connectBookmark(bookmark.id));
    } catch {
      openConnect(bookmark);
    }
  }

  let canUpload = $derived(connected && localPane.selected.size > 0);
  let canDownload = $derived(connected && remotePane.selected.size > 0);

  /** Download the remote pane's selection into the local pane's folder. */
  async function download(): Promise<void> {
    const id = sessions.active?.info.id;
    if (!id) return;
    await runTransfers("download", id, remotePane.selectedEntries, localPane.path);
  }

  /** Upload the local pane's selection into the remote pane's folder. */
  async function upload(): Promise<void> {
    const id = sessions.active?.info.id;
    if (!id || !remotePane.path) return;
    await runTransfers("upload", id, localPane.selectedEntries, remotePane.path);
  }

  /**
   * Enqueue transfers for a selection: files as direct transfers, directories
   * recursively. Rows appear via transfer state events (no seeding needed).
   */
  async function runTransfers(
    direction: TransferDirection,
    sessionId: string,
    entries: DirEntry[],
    destDir: string,
  ): Promise<void> {
    const fileRequests = buildTransferRequests(direction, sessionId, entries, destDir);
    const dirs = entries.filter((e) => e.kind === "dir");
    let any = false;
    if (fileRequests.length > 0) {
      await enqueueTransfers(fileRequests);
      any = true;
    }
    for (const dir of dirs) {
      await enqueueDirectory(sessionId, direction, dir.path, destDir);
      any = true;
    }
    if (any) ui.setTransferPanelExpanded(true);
  }

  // File-operation dialog/menu state.
  let contextMenu = $state<{ x: number; y: number; kind: PaneKind; entry: DirEntry } | null>(null);
  let permsTarget = $state<{ kind: PaneKind; path: string; mode: number } | null>(null);
  let deleteTarget = $state<{ kind: PaneKind; entries: DirEntry[] } | null>(null);
  let inputDialog = $state<{
    title: string;
    label: string;
    initial: string;
    onSubmit: (value: string) => void;
  } | null>(null);

  /** The pane store for a kind. */
  function paneOf(kind: PaneKind) {
    return kind === "local" ? localPane : remotePane;
  }

  /** The session id to use for a pane's ops (null = local filesystem). */
  function sessionIdFor(kind: PaneKind): string | null {
    return kind === "remote" ? (sessions.active?.info.id ?? null) : null;
  }

  /** Reload a pane after an operation. */
  function refresh(kind: PaneKind): void {
    if (kind === "local") void loadLocal(localPane.path);
    else if (remotePane.path) void loadRemote(remotePane.path);
  }

  /** Refresh the active pane. */
  function refreshActive(): void {
    refresh(ui.activePane);
  }

  /** Open an entry from the pane (directories navigate). */
  function openInPane(kind: PaneKind, entry: DirEntry): void {
    if (entry.kind !== "dir") return;
    if (kind === "local") void loadLocal(entry.path);
    else void loadRemote(entry.path);
  }

  /** Rename an entry via an input dialog. */
  function startRename(kind: PaneKind, entry: DirEntry): void {
    inputDialog = {
      title: "Rename",
      label: "New name",
      initial: entry.name,
      onSubmit: async (name) => {
        inputDialog = null;
        const dest = joinPath(parentPath(entry.path), name);
        await renameEntry(sessionIdFor(kind), entry.path, dest);
        refresh(kind);
      },
    };
  }

  /** Create a new folder in a pane via an input dialog. */
  function startNewFolder(kind: PaneKind): void {
    inputDialog = {
      title: "New folder",
      label: "Folder name",
      initial: "untitled",
      onSubmit: async (name) => {
        inputDialog = null;
        await makeDir(sessionIdFor(kind), joinPath(paneOf(kind).path, name));
        refresh(kind);
      },
    };
  }

  /** Delete the pane's selection (confirmed for directories). */
  function startDelete(kind: PaneKind): void {
    const entries = paneOf(kind).selectedEntries;
    if (entries.length > 0) deleteTarget = { kind, entries };
  }

  /** Confirm and run the pending delete. */
  async function confirmDelete(): Promise<void> {
    if (!deleteTarget) return;
    const { kind, entries } = deleteTarget;
    deleteTarget = null;
    const hasDir = entries.some((e) => e.kind === "dir");
    await deleteEntries(
      sessionIdFor(kind),
      entries.map((e) => e.path),
      hasDir,
    );
    refresh(kind);
  }

  /** Edit an entry's permissions. */
  function startPermissions(kind: PaneKind, entry: DirEntry): void {
    if (entry.permissions == null) return;
    permsTarget = { kind, path: entry.path, mode: entry.permissions };
  }

  /** Apply an edited permission mode. */
  async function applyPermissions(mode: number): Promise<void> {
    if (!permsTarget) return;
    const { kind, path } = permsTarget;
    permsTarget = null;
    await setPermissions(sessionIdFor(kind), path, mode);
    refresh(kind);
  }

  /** Open the context menu for a right-clicked entry. */
  function openContextMenu(kind: PaneKind, entry: DirEntry, event: MouseEvent): void {
    contextMenu = { x: event.clientX, y: event.clientY, kind, entry };
  }

  /** Build the context-menu items for an entry. */
  function menuItems(kind: PaneKind, entry: DirEntry): MenuItem[] {
    const transfer =
      kind === "remote"
        ? { label: "Download", action: download }
        : { label: "Upload", action: upload };
    return [
      { label: "Open", action: () => openInPane(kind, entry), disabled: entry.kind !== "dir" },
      { separator: true },
      { ...transfer, disabled: !connected },
      { separator: true },
      { label: "Rename…", action: () => startRename(kind, entry) },
      { label: "Delete", action: () => startDelete(kind) },
      { label: "New folder…", action: () => startNewFolder(kind) },
      {
        label: "Permissions…",
        action: () => startPermissions(kind, entry),
        disabled: entry.permissions == null,
      },
      { separator: true },
      { label: "Copy path", action: () => void navigator.clipboard?.writeText(entry.path) },
      { label: "Refresh", action: () => refresh(kind) },
    ];
  }

  /** Global keyboard shortcuts. */
  function onGlobalKey(event: KeyboardEvent): void {
    // Don't hijack typing in inputs.
    const target = event.target as HTMLElement | null;
    if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) return;

    const kind = ui.activePane;
    if (event.key === "F2") {
      const entry = paneOf(kind).selectedEntries[0];
      if (entry) {
        event.preventDefault();
        startRename(kind, entry);
      }
      return;
    }
    if (event.key === "Delete" || event.key === "Backspace") {
      if (paneOf(kind).selected.size > 0) {
        event.preventDefault();
        startDelete(kind);
      }
      return;
    }

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
          onDrop={(src) => onPaneDrop(src, "local")}
          onContextMenu={(entry, e) => openContextMenu("local", entry, e)}
        />
      {/snippet}
      {#snippet right()}
        {#if connected}
          <FilePane
            pane={remotePane}
            active={ui.activePane === "remote"}
            emptyMessage="Empty directory."
            onActivate={() => ui.setActivePane("remote")}
            onNavigate={loadRemote}
            onDrop={(src) => onPaneDrop(src, "remote")}
            onContextMenu={(entry, e) => openContextMenu("remote", entry, e)}
            banner={remoteBanner}
          />
        {:else}
          <section class="bookmark-pane" aria-label="remote pane">
            <BookmarkManager
              onConnect={connectFromBookmark}
              onEdit={(b) => openConnect(b)}
              onAdd={() => openConnect(null)}
            />
          </section>
        {/if}
      {/snippet}
    </SplitPane>
  </main>

  <TransferPanel />
  <StatusBar {connectionLabel} transferCount={transfers.activeCount} />
</div>

{#if showConnect}
  <ConnectDialog {onConnected} initial={connectSeed} onClose={() => (showConnect = false)} />
{/if}
<HostKeyDialog />
<ConflictDialog />

{#if prompts.auth}
  {@const authPrompt = prompts.auth}
  <PromptDialog
    title="Authentication"
    instructions={authPrompt.instructions}
    fields={authPrompt.fields}
    onSubmit={(responses) => {
      prompts.clearAuth();
      void respondPrompt(authPrompt.promptId, { type: "keyboardInteractive", responses });
    }}
    onCancel={() => {
      prompts.clearAuth();
      void respondPrompt(authPrompt.promptId, { type: "keyboardInteractive", responses: [] });
    }}
  />
{/if}

{#if contextMenu}
  <ContextMenu
    x={contextMenu.x}
    y={contextMenu.y}
    items={menuItems(contextMenu.kind, contextMenu.entry)}
    onClose={() => (contextMenu = null)}
  />
{/if}
{#if permsTarget}
  <PermissionsDialog
    path={permsTarget.path}
    mode={permsTarget.mode}
    onApply={applyPermissions}
    onCancel={() => (permsTarget = null)}
  />
{/if}
{#if deleteTarget}
  <DeleteConfirmDialog
    names={deleteTarget.entries.map((e) => e.name)}
    hasDir={deleteTarget.entries.some((e) => e.kind === "dir")}
    onConfirm={confirmDelete}
    onCancel={() => (deleteTarget = null)}
  />
{/if}
{#if inputDialog}
  <InputDialog
    title={inputDialog.title}
    label={inputDialog.label}
    initial={inputDialog.initial}
    onSubmit={inputDialog.onSubmit}
    onCancel={() => (inputDialog = null)}
  />
{/if}

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
  .bookmark-pane {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
    min-width: 0;
    border: 1px solid var(--border, #d0d0d0);
    border-radius: 6px;
    margin: 6px;
    overflow: hidden;
    background: var(--surface, #fff);
  }
</style>
