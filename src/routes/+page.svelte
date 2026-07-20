<!--
  +page.svelte — Application shell (dual-pane layout).

  Composes the toolbar, the resizable split of the local and remote panes, the
  transfer dock, and the status bar, and mounts the connect + host-key dialogs.
  Drives real browsing: the local pane loads from the home directory on mount;
  the remote pane loads "/" on connect. Cmd/Ctrl+R refreshes the active pane and
  Cmd/Ctrl+L focuses its path field.
-->
<script lang="ts">
  import { onMount, tick } from "svelte";
  import { ui } from "$lib/stores/ui.svelte";
  import { sessions } from "$lib/stores/sessions.svelte";
  import { transfers } from "$lib/stores/transfers.svelte";
  import { prompts } from "$lib/stores/prompts.svelte";
  import { bookmarks } from "$lib/stores/bookmarks.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { logs } from "$lib/stores/logs.svelte";
  import { health, latencyLevel, sparkline } from "$lib/stores/health.svelte";
  import { edits } from "$lib/stores/edits.svelte";
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
    startEditSession,
    closeEditSession,
    listEditSessions,
    type Bookmark,
    type SessionInfo,
    type TransferDirection,
  } from "$lib/ipc/commands";
  import type { DirEntry } from "$lib/ipc/commands";
  import type { PaneKind } from "$lib/types";
  import { buildTransferRequests, dropDirection, uploadRequestsForPaths } from "$lib/transfer";
  import { resolveShortcut } from "$lib/keymap";
  import { buildCommands } from "$lib/palette";
  import { toasts } from "$lib/stores/toasts.svelte";
  import { parentPath, joinPath } from "$lib/utils/path";
  import { formatRate } from "$lib/utils/format";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { openPath } from "@tauri-apps/plugin-opener";
  import ContextMenu, { type MenuItem } from "$lib/components/common/ContextMenu.svelte";
  import CommandPalette from "$lib/components/common/CommandPalette.svelte";
  import PermissionsDialog from "$lib/components/dialogs/PermissionsDialog.svelte";
  import DeleteConfirmDialog from "$lib/components/dialogs/DeleteConfirmDialog.svelte";
  import InputDialog from "$lib/components/dialogs/InputDialog.svelte";
  import Toolbar from "$lib/components/layout/Toolbar.svelte";
  import StatusBar from "$lib/components/layout/StatusBar.svelte";
  import SplitPane from "$lib/components/layout/SplitPane.svelte";
  import FilePane from "$lib/components/pane/FilePane.svelte";
  import TransferPanel from "$lib/components/transfers/TransferPanel.svelte";
  import Toasts from "$lib/components/common/Toasts.svelte";
  import ConnectDialog from "$lib/components/dialogs/ConnectDialog.svelte";
  import BookmarkManager from "$lib/components/dialogs/BookmarkManager.svelte";
  import SettingsDialog from "$lib/components/dialogs/SettingsDialog.svelte";
  import HostKeyDialog from "$lib/components/dialogs/HostKeyDialog.svelte";
  import ConflictDialog from "$lib/components/dialogs/ConflictDialog.svelte";
  import PromptDialog from "$lib/components/dialogs/PromptDialog.svelte";

  let showConnect = $state(false);
  let showSettings = $state(false);
  let showPalette = $state(false);
  // The bookmark prefilling the connect dialog: null = a fresh connection.
  let connectSeed = $state<Bookmark | null>(null);

  /**
   * Open the connect dialog, optionally prefilled from a bookmark.
   *
   * @param seed - the bookmark to prefill from; null (the default) for a fresh
   *   connection.
   */
  function openConnect(seed: Bookmark | null = null): void {
    connectSeed = seed;
    showConnect = true;
  }

  let active = $derived(sessions.active);
  let connected = $derived(active !== null);
  let connectionLabel = $derived(
    active ? `${active.info.username}@${active.info.host}` : "not connected",
  );
  // Topbar host label (with port) and session-detail chip.
  let hostLabel = $derived(
    active ? `${active.info.username}@${active.info.host}:${active.info.port}` : "not connected",
  );
  let metaChip = $derived(active ? "sftp · ssh-2" : "sftp");
  // Health HUD: the active session's latency ring and the queue's summed rate.
  let rttHud = $derived.by(() => {
    const id = active?.info.id;
    if (!id) return null;
    const latest = health.latest(id);
    if (latest == null) return null;
    return { spark: sparkline(health.samples(id)), ms: latest, level: latencyLevel(latest) };
  });
  let throughputHud = $derived.by(() => {
    const total = transfers.list
      .filter((t) => t.state === "running")
      .reduce((sum, t) => sum + t.rateBps, 0);
    return total > 0 ? formatRate(total) : null;
  });
  let remoteBanner = $derived(
    active?.state === "reconnecting" ? "Connection lost — reconnecting…" : null,
  );

  // Reflect the active session in the window/document title.
  $effect(() => {
    const title = active ? `kestrel — ${active.info.username}@${active.info.host}` : "kestrel";
    document.title = title;
    try {
      void getCurrentWindow().setTitle(title);
    } catch {
      /* no Tauri runtime (dev preview) */
    }
  });

  /**
   * Load a local directory into the local pane, and watch it for changes.
   *
   * @param path - the local directory to list; failures land in the pane's error
   *   state.
   */
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

  /**
   * Load a remote directory into the remote pane (needs an active session).
   *
   * @param path - the remote directory to list; a no-op when no session is active,
   *   and failures land in the pane's error state.
   */
  async function loadRemote(path: string): Promise<void> {
    const id = sessions.active?.info.id;
    if (!id) return;
    remotePane.startLoad(path);
    logs.command(`cd "${path}"`);
    try {
      const entries = await listDir(id, path);
      remotePane.setEntries(entries);
      logs.status(`Directory listing successful — ${entries.length} entries`, true);
    } catch (e) {
      remotePane.setError(String(e));
      logs.error(`list "${path}": ${String(e)}`);
    }
  }

  let unlistenDrop: (() => void) | undefined;

  onMount(() => {
    // Guard: browser dev preview has no Tauri runtime.
    try {
      logs.status("kestrel ready — use [connect] to open a session", true);
      void initSessionEvents();
      listEditSessions()
        .then((live) => edits.replace(live))
        .catch(() => {});
      // Auto-refresh the local pane when its directory changes externally.
      setLocalDirChangedHandler((path) => {
        if (localPane.path === path) void loadLocal(path);
      });
      bookmarks.load().catch(() => {});
      // Load settings, then open the local pane at the pinned default dir (or
      // the home directory when none is set).
      settings
        .load()
        .then(() => settings.defaultLocalDir ?? localHomeDir())
        .catch(() => localHomeDir())
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

  /**
   * Enqueue uploads for OS-dropped local file paths (to the remote pane).
   *
   * @param paths - the dropped local paths; ignored when empty or when no session
   *   or remote directory is available.
   */
  async function onOsDrop(paths: string[]): Promise<void> {
    const id = sessions.active?.info.id;
    if (!id || !remotePane.path || paths.length === 0) return;
    try {
      await enqueueTransfers(uploadRequestsForPaths(id, paths, remotePane.path));
      ui.setTransferPanelExpanded(true);
    } catch (e) {
      toasts.error(`Could not queue upload: ${String(e)}`);
    }
  }

  /**
   * Handle a cross-pane drag drop.
   *
   * @param source - the pane the drag started in (supplies the selection).
   * @param target - the pane dropped onto (supplies the destination directory).
   */
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

  /**
   * Track a newly connected session and load its root.
   *
   * @param info - the new session; becomes active, and its `/` is loaded into the
   *   remote pane.
   */
  function onConnected(info: SessionInfo): void {
    sessions.add(info);
    logs.status(`Connected to ${info.host}:${info.port}`, true);
    logs.status(`Authenticated as ${info.username}`, true);
    void loadRemote("/");
    ui.setActivePane("remote");
  }

  /**
   * Connect using a saved bookmark. If the backend can't (e.g. no saved
   * password), fall back to the connect dialog prefilled from the bookmark.
   *
   * @param bookmark - the saved connection to open.
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
   *
   * @param direction - whether the entries are uploaded or downloaded.
   * @param sessionId - the session to transfer over.
   * @param entries - the selected entries; files and directories are handled
   *   separately.
   * @param destDir - the destination directory.
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
    try {
      if (fileRequests.length > 0) {
        await enqueueTransfers(fileRequests);
        any = true;
      }
      for (const dir of dirs) {
        await enqueueDirectory(sessionId, direction, dir.path, destDir);
        any = true;
      }
    } catch (e) {
      toasts.error(`Could not queue transfer: ${String(e)}`);
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

  /**
   * Expand or collapse a directory in place, lazily listing its children the
   * first time it is opened. Collapsing keeps the cached listing so re-opening
   * is instant; a navigate/refresh drops it.
   *
   * @param kind - which pane the directory lives in.
   * @param entry - the directory row being toggled.
   */
  async function toggleExpand(kind: PaneKind, entry: DirEntry): Promise<void> {
    const pane = paneOf(kind);
    if (pane.isExpanded(entry.path)) {
      pane.collapse(entry.path);
      return;
    }
    if (!pane.hasChildren(entry.path)) {
      const sessionId = sessionIdFor(kind);
      if (kind === "remote" && !sessionId) return;
      pane.setChildLoading(entry.path, true);
      try {
        const children = sessionId
          ? await listDir(sessionId, entry.path)
          : await localListDir(entry.path);
        pane.setChildren(entry.path, children);
      } catch (e) {
        toasts.error(`Could not open ${entry.name}: ${String(e)}`);
        return;
      } finally {
        pane.setChildLoading(entry.path, false);
      }
    }
    pane.expand(entry.path);
  }

  /**
   * The pane store for a kind.
   *
   * @param kind - which pane to look up.
   * @returns the local or remote pane store.
   */
  function paneOf(kind: PaneKind) {
    return kind === "local" ? localPane : remotePane;
  }

  /**
   * The session id to use for a pane's ops (null = local filesystem).
   *
   * @param kind - which pane the operation targets.
   * @returns the active session id for the remote pane, or null for the local pane
   *   (or when no session is active).
   */
  function sessionIdFor(kind: PaneKind): string | null {
    return kind === "remote" ? (sessions.active?.info.id ?? null) : null;
  }

  /**
   * Reload a pane after an operation.
   *
   * @param kind - which pane to reload, at its current path.
   */
  function refresh(kind: PaneKind): void {
    if (kind === "local") void loadLocal(localPane.path);
    else if (remotePane.path) void loadRemote(remotePane.path);
  }

  /** Refresh the active pane. */
  function refreshActive(): void {
    refresh(ui.activePane);
  }

  /**
   * Open an entry from the pane (directories navigate).
   *
   * @param kind - the pane the entry belongs to.
   * @param entry - the entry to open; non-directories are ignored.
   */
  function openInPane(kind: PaneKind, entry: DirEntry): void {
    if (entry.kind !== "dir") return;
    if (kind === "local") void loadLocal(entry.path);
    else void loadRemote(entry.path);
  }

  /**
   * Download a remote file into its managed edit session and open it with the
   * operating system's default editor/application.
   *
   * @param entry - regular remote file to edit.
   */
  async function editRemoteFile(entry: DirEntry): Promise<void> {
    const sessionId = sessions.active?.info.id;
    if (!sessionId || entry.kind !== "file") return;
    let startedId: string | null = null;
    try {
      const edit = await startEditSession(sessionId, entry.path);
      startedId = edit.id;
      edits.upsert(edit);
      await openPath(edit.localPath);
      logs.status(`Editing ${entry.path}; saves sync automatically`, true);
    } catch (error) {
      if (startedId) {
        void closeEditSession(startedId).catch(() => {});
        edits.remove(startedId);
      }
      toasts.error(`Could not edit ${entry.name}: ${String(error)}`);
    }
  }

  /**
   * Stop a managed edit session and remove it from the indicator immediately.
   *
   * @param id - edit session id.
   */
  async function closeManagedEdit(id: string): Promise<void> {
    try {
      await closeEditSession(id);
      edits.remove(id);
    } catch (error) {
      toasts.error(`Could not close edit session: ${String(error)}`);
    }
  }

  /**
   * Rename an entry via an input dialog.
   *
   * @param kind - the pane the entry belongs to.
   * @param entry - the entry to rename; its name seeds the dialog and the new name
   *   is applied in its parent directory.
   */
  function startRename(kind: PaneKind, entry: DirEntry): void {
    inputDialog = {
      title: "Rename",
      label: "New name",
      initial: entry.name,
      onSubmit: async (name) => {
        inputDialog = null;
        const dest = joinPath(parentPath(entry.path), name);
        try {
          await renameEntry(sessionIdFor(kind), entry.path, dest);
        } catch (e) {
          toasts.error(`Rename failed: ${String(e)}`);
        }
        refresh(kind);
      },
    };
  }

  /**
   * Create a new folder in a pane via an input dialog.
   *
   * @param kind - the pane whose current directory receives the new folder.
   */
  function startNewFolder(kind: PaneKind): void {
    inputDialog = {
      title: "New folder",
      label: "Folder name",
      initial: "untitled",
      onSubmit: async (name) => {
        inputDialog = null;
        try {
          await makeDir(sessionIdFor(kind), joinPath(paneOf(kind).path, name));
        } catch (e) {
          toasts.error(`Create folder failed: ${String(e)}`);
        }
        refresh(kind);
      },
    };
  }

  /**
   * Delete the pane's selection (confirmed for directories).
   *
   * @param kind - the pane whose selection is staged for deletion; a no-op when
   *   nothing is selected.
   */
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
    try {
      await deleteEntries(
        sessionIdFor(kind),
        entries.map((e) => e.path),
        hasDir,
      );
    } catch (e) {
      toasts.error(`Delete failed: ${String(e)}`);
    }
    refresh(kind);
  }

  /**
   * Edit an entry's permissions.
   *
   * @param kind - the pane the entry belongs to.
   * @param entry - the entry to edit; ignored when it has no permission bits.
   */
  function startPermissions(kind: PaneKind, entry: DirEntry): void {
    if (entry.permissions == null) return;
    permsTarget = { kind, path: entry.path, mode: entry.permissions };
  }

  /**
   * Apply an edited permission mode.
   *
   * @param mode - the Unix permission bits to write to the pending target.
   */
  async function applyPermissions(mode: number): Promise<void> {
    if (!permsTarget) return;
    const { kind, path } = permsTarget;
    permsTarget = null;
    try {
      await setPermissions(sessionIdFor(kind), path, mode);
    } catch (e) {
      toasts.error(`Permissions change failed: ${String(e)}`);
    }
    refresh(kind);
  }

  /**
   * Open the context menu for a right-clicked entry.
   *
   * @param kind - the pane the entry belongs to.
   * @param entry - the right-clicked entry.
   * @param event - the mouse event; its client coordinates position the menu.
   */
  function openContextMenu(kind: PaneKind, entry: DirEntry, event: MouseEvent): void {
    contextMenu = { x: event.clientX, y: event.clientY, kind, entry };
  }

  /**
   * Build the context-menu items for an entry.
   *
   * @param kind - the pane the entry belongs to; decides download vs upload.
   * @param entry - the entry the menu acts on.
   * @returns the menu items, with open/transfer/permissions disabled where they do
   *   not apply.
   */
  function menuItems(kind: PaneKind, entry: DirEntry): MenuItem[] {
    const transfer =
      kind === "remote"
        ? { label: "Download", action: download }
        : { label: "Upload", action: upload };
    return [
      { label: "Open", action: () => openInPane(kind, entry), disabled: entry.kind !== "dir" },
      {
        label: "Edit",
        action: () => void editRemoteFile(entry),
        disabled: kind !== "remote" || entry.kind !== "file" || !connected,
      },
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

  /**
   * Open (or refocus) the filter bar in a pane and put the caret in it.
   *
   * @param kind - which pane to filter.
   */
  function openFilter(kind: PaneKind): void {
    const pane = paneOf(kind);
    if (pane.filter === null) pane.setFilter("");
    void tick().then(() => document.getElementById(`filter-input-${kind}`)?.focus());
  }

  /**
   * Close the command palette and hand focus back to the active pane, so the
   * global shortcuts (which ignore keystrokes inside inputs) work immediately.
   */
  function closePalette(): void {
    showPalette = false;
    (
      document.querySelector(`section[data-kind="${ui.activePane}"]`) as HTMLElement | null
    )?.focus();
  }

  /**
   * Build the palette's command inventory from live state (called on open).
   *
   * The actions record is total over ShortcutAction (compile-checked), so a new
   * shortcut without a palette entry cannot slip through.
   *
   * @returns the commands for CommandPalette, in canonical order.
   */
  function paletteCommands() {
    return buildCommands({
      actions: {
        refresh: refreshActive,
        focusPath: () => document.getElementById(`path-input-${ui.activePane}`)?.focus(),
        download: () => {
          if (canDownload) void download();
        },
        upload: () => {
          if (canUpload) void upload();
        },
        rename: () => {
          const entry = paneOf(ui.activePane).selectedEntries[0];
          if (entry) startRename(ui.activePane, entry);
        },
        delete: () => startDelete(ui.activePane),
        switchPane: () => ui.setActivePane(ui.activePane === "local" ? "remote" : "local"),
        filter: () => openFilter(ui.activePane),
        // The palette does not list itself.
        palette: () => {},
      },
      connected,
      canUpload,
      canDownload,
      onConnect: () => void onConnect(),
      onSettings: () => (showSettings = true),
      onQueue: () => ui.toggleTransferPanel(),
      onNewFolder: () => startNewFolder(ui.activePane),
      bookmarks: bookmarks.items,
      onConnectBookmark: (b) => void connectFromBookmark(b),
    });
  }

  /**
   * Global keyboard shortcuts (mapping in $lib/keymap; dispatch here).
   *
   * @param event - the window keydown event; its default is prevented only when a
   *   shortcut actually runs.
   */
  function onGlobalKey(event: KeyboardEvent): void {
    const action = resolveShortcut(event);
    if (!action) return;
    const kind = ui.activePane;
    switch (action) {
      case "rename": {
        const entry = paneOf(kind).selectedEntries[0];
        if (entry) {
          event.preventDefault();
          startRename(kind, entry);
        }
        break;
      }
      case "delete":
        if (paneOf(kind).selected.size > 0) {
          event.preventDefault();
          startDelete(kind);
        }
        break;
      case "switchPane":
        event.preventDefault();
        ui.setActivePane(kind === "local" ? "remote" : "local");
        break;
      case "palette":
        event.preventDefault();
        if (showPalette) closePalette();
        else showPalette = true;
        break;
      case "filter":
        event.preventDefault();
        openFilter(kind);
        break;
      case "refresh":
        event.preventDefault();
        refreshActive();
        break;
      case "focusPath":
        event.preventDefault();
        document.getElementById(`path-input-${ui.activePane}`)?.focus();
        break;
      case "download":
        event.preventDefault();
        if (canDownload) void download();
        break;
      case "upload":
        event.preventDefault();
        if (canUpload) void upload();
        break;
    }
  }
</script>

<svelte:window onkeydown={onGlobalKey} />

<div class="app">
  <Toolbar
    {connected}
    host={hostLabel}
    meta={metaChip}
    {canUpload}
    {canDownload}
    editSessions={edits.list}
    onCloseEdit={(id) => void closeManagedEdit(id)}
    {onConnect}
    onUpload={upload}
    onDownload={download}
    onRefresh={refreshActive}
    onQueue={() => ui.toggleTransferPanel()}
    onSettings={() => (showSettings = true)}
    rtt={rttHud}
    throughput={throughputHud}
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
          onToggleExpand={(entry) => void toggleExpand("local", entry)}
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
            onToggleExpand={(entry) => void toggleExpand("remote", entry)}
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
  <StatusBar
    {connectionLabel}
    transferCount={transfers.activeCount}
    sessionId={active?.info.id ?? null}
  />
</div>

{#if showConnect}
  <ConnectDialog {onConnected} initial={connectSeed} onClose={() => (showConnect = false)} />
{/if}
{#if showSettings}
  <SettingsDialog onClose={() => (showSettings = false)} />
{/if}
{#if showPalette}
  <CommandPalette commands={paletteCommands()} onClose={closePalette} />
{/if}
<HostKeyDialog />
<ConflictDialog />
<Toasts />

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
    border-bottom: 1px solid var(--border);
  }
  .bookmark-pane {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
    min-width: 0;
    border-left: 1px solid var(--grid);
    overflow: hidden;
    background: var(--bg);
  }
</style>
