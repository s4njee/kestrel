// commands.ts — Typed wrappers around Tauri `invoke` commands.
//
// This is the SINGLE frontend mirror of `src-tauri/src/dto.rs` (and the command
// signatures in `src-tauri/src/commands/`). Keep the types and argument names
// here in sync with that file by hand — when you change one, change the other.
// Tauri maps camelCase JS argument keys to the snake_case Rust parameters.

import { invoke, Channel } from "@tauri-apps/api/core";

/** A directory entry (mirrors `DirEntryDto`). */
export interface DirEntry {
  name: string;
  path: string;
  kind: "file" | "dir" | "symlink";
  size: number;
  mtime: number | null;
  permissions: number | null;
  linkTarget: string | null;
}

/** Summary of a connected session (mirrors `SessionInfoDto`). */
export interface SessionInfo {
  id: string;
  host: string;
  port: number;
  username: string;
}

/** Lifecycle state of a managed remote-file edit session. */
export type EditSessionState = "watching" | "uploading" | "conflict" | "error";

/** Managed local copy of a remote file opened for editing. */
export interface EditSession {
  id: string;
  sessionId: string;
  remotePath: string;
  localPath: string;
  state: EditSessionState;
  error: string | null;
}

/** Auth choice sent with a connect request (mirrors `AuthDto`). */
export type Auth =
  | { method: "password"; password: string }
  | { method: "key"; path: string; passphrase: string | null }
  | { method: "agent" }
  | { method: "keyboardInteractive" };

/** A connection request (mirrors `ConnectRequest`). */
export interface ConnectRequest {
  host: string;
  port: number;
  username: string;
  auth: Auth;
}

/** Which auth method a bookmark uses (the persisted string form). */
export type BookmarkAuthMethod = "password" | "key" | "agent" | "keyboardInteractive";

/** A saved connection bookmark (mirrors `Bookmark`). Never carries a secret. */
export interface Bookmark {
  /** Stable id; nil (all-zero UUID) means "new" when saving. */
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  authMethod: BookmarkAuthMethod;
  keyPath: string | null;
  remoteDir: string | null;
  localDir: string | null;
  hasSavedSecret: boolean;
}

/** The nil UUID used to mark an unsaved (new) bookmark. */
export const NIL_UUID = "00000000-0000-0000-0000-000000000000";

/** Default conflict resolution stored in settings. */
export type DefaultConflict = "ask" | "overwrite" | "skip" | "rename" | "resume";

/** User settings (mirrors `Settings`). */
export interface Settings {
  /** Max concurrently-running transfers (1–8). */
  concurrency: number;
  defaultConflict: DefaultConflict;
  defaultLocalDir: string | null;
  showHidden: boolean;
  /**
   * Whether recursive directory transfers may stream as a single tar archive
   * when the remote host supports it. Falls back to per-file transfers when off
   * or unsupported.
   */
  tarAcceleration: boolean;
  /** Compare local and remote checksums after successful file transfers. */
  verifyAfterTransfer: boolean;
}

/** A reply to a prompt (mirrors `PromptReplyDto`). */
export type PromptReply =
  { type: "hostKey"; accept: boolean } | { type: "keyboardInteractive"; responses: string[] };

/** Transfer direction. */
export type TransferDirection = "upload" | "download";

/** A transfer to enqueue (mirrors `TransferRequestDto`). */
export interface TransferRequest {
  sessionId: string;
  direction: TransferDirection;
  src: string;
  dest: string;
  size: number;
}

/** Transfer lifecycle state string. */
export type TransferStateStr =
  | "queued"
  | "running"
  | "paused"
  | "awaitingUser"
  | "done"
  | "skipped"
  | "failed"
  | "failedVerification"
  | "canceled";

/** How to resolve a destination-exists conflict. */
export type ConflictResolution = "overwrite" | "skip" | "rename" | "resume";

/** Transfer events pushed over the transfer channel (mirrors `TransferEventDto`). */
export type TransferEvent =
  | { type: "progressBatch"; items: { id: string; bytes: number; rateBps: number }[] }
  | {
      type: "state";
      id: string;
      state: TransferStateStr;
      error: string | null;
      name: string;
      size: number;
      bytes: number;
      direction: TransferDirection;
    }
  | {
      type: "conflict";
      id: string;
      dest: string;
      existingSize: number;
      existingMtime: number | null;
      incomingSize: number;
      incomingMtime: number | null;
    };

/** Session/prompt events pushed over the channel (mirrors `SessionEventDto`). */
export type SessionEvent =
  | {
      type: "hostKeyPrompt";
      promptId: string;
      host: string;
      port: number;
      keyType: string;
      fingerprintSha256: string;
      status: "unknown" | "CHANGED";
      existingFingerprint: string | null;
    }
  | {
      type: "connectionState";
      sessionId: string;
      state: "connected" | "disconnected" | "reconnecting";
      reason: string | null;
    }
  | {
      type: "authPrompt";
      promptId: string;
      instructions: string;
      fields: { text: string; echo: boolean }[];
    }
  | { type: "localDirChanged"; path: string }
  | { type: "editSessionChanged"; session: EditSession }
  | { type: "editSessionClosed"; editId: string }
  /** Raw shell output; `data` is base64 (terminal bytes are not valid UTF-8). */
  | { type: "shellData"; shellId: string; data: string }
  | { type: "shellClosed"; shellId: string };

/**
 * Connect and authenticate a session.
 *
 * @param request - host/port/username and auth method.
 * @returns the new session's info. Rejects on auth/host-key/connection failure.
 */
export function connect(request: ConnectRequest): Promise<SessionInfo> {
  return invoke("connect", { request });
}

/**
 * Disconnect a session.
 *
 * @param sessionId - the session to close.
 */
export function disconnect(sessionId: string): Promise<void> {
  return invoke("disconnect", { sessionId });
}

/**
 * Download a remote file into a watched managed temp directory.
 *
 * @param sessionId - connected SSH session.
 * @param remotePath - regular remote file to edit.
 * @returns the ready edit session; open its localPath with the opener plugin.
 */
export function startEditSession(sessionId: string, remotePath: string): Promise<EditSession> {
  return invoke("start_edit_session", { sessionId, remotePath });
}

/**
 * Close an edit session and release its managed local copy.
 *
 * @param editId - edit session id.
 */
export function closeEditSession(editId: string): Promise<void> {
  return invoke("close_edit_session", { editId });
}

/** List the live managed edit sessions. */
export function listEditSessions(): Promise<EditSession[]> {
  return invoke("list_edit_sessions");
}

/**
 * Answer a pending prompt (e.g. host-key trust).
 *
 * @param promptId - the prompt id from the event.
 * @param reply - the user's decision.
 */
export function respondPrompt(promptId: string, reply: PromptReply): Promise<void> {
  return invoke("respond_prompt", { promptId, reply });
}

/**
 * Open an interactive shell (PTY) on a session.
 *
 * @param sessionId - the session to run the shell on.
 * @param cols - initial terminal width in characters.
 * @param rows - initial terminal height in rows.
 * @returns the new shell's id; output then arrives as `shellData` events.
 */
export function openShell(sessionId: string, cols: number, rows: number): Promise<string> {
  return invoke("open_shell", { sessionId, cols, rows });
}

/**
 * Send keystrokes to a shell.
 *
 * @param shellId - the shell.
 * @param data - the input bytes; base64-encoded for transport.
 */
export function shellWrite(shellId: string, data: Uint8Array): Promise<void> {
  return invoke("shell_write", { shellId, data: toBase64(data) });
}

/**
 * Tell a shell its terminal was resized (sends SSH `window-change`).
 *
 * @param shellId - the shell.
 * @param cols - the new width in characters.
 * @param rows - the new height in rows.
 */
export function shellResize(shellId: string, cols: number, rows: number): Promise<void> {
  return invoke("shell_resize", { shellId, cols, rows });
}

/**
 * Close a shell.
 *
 * @param shellId - the shell to close.
 */
export function closeShell(shellId: string): Promise<void> {
  return invoke("close_shell", { shellId });
}

/**
 * Encode bytes as base64 for transport to the backend.
 *
 * Chunked rather than spread into `String.fromCharCode(...)`, which blows the
 * argument limit on large pastes.
 *
 * @param bytes - the raw bytes.
 * @returns the base64 text.
 */
export function toBase64(bytes: Uint8Array): string {
  let binary = "";
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(binary);
}

/**
 * Decode base64 shell output into bytes.
 *
 * @param data - the base64 text from a `shellData` event.
 * @returns the raw bytes to feed the terminal.
 */
export function fromBase64(data: string): Uint8Array {
  const binary = atob(data);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i);
  return out;
}

/**
 * List all saved bookmarks.
 *
 * @returns every stored bookmark (no secrets).
 */
export function listBookmarks(): Promise<Bookmark[]> {
  return invoke("list_bookmarks");
}

/**
 * Create or update a bookmark, optionally saving a secret to the OS keychain.
 *
 * @param bookmark - the details to store (nil id ⇒ new).
 * @param secret - optional password/passphrase to persist in the keychain.
 * @returns the stored bookmark, with its assigned id and secret flag.
 */
export function saveBookmark(bookmark: Bookmark, secret?: string): Promise<Bookmark> {
  return invoke("save_bookmark", { bookmark, secret: secret ?? null });
}

/**
 * Delete a bookmark and any keychain secret it owns.
 *
 * @param id - the bookmark id.
 */
export function deleteBookmark(id: string): Promise<void> {
  return invoke("delete_bookmark", { id });
}

/**
 * Connect using a saved bookmark; the backend reads any saved secret.
 *
 * @param id - the bookmark id.
 * @returns the new session's info. Rejects when a required secret is not saved
 *   (the caller should then open the connect dialog to prompt for it).
 */
export function connectBookmark(id: string): Promise<SessionInfo> {
  return invoke("connect_bookmark", { id });
}

/**
 * List a remote directory.
 *
 * @param sessionId - the session.
 * @param path - remote directory path.
 * @returns the directory entries.
 */
export function listDir(sessionId: string, path: string): Promise<DirEntry[]> {
  return invoke("list_dir", { sessionId, path });
}

/**
 * Stat a single remote path.
 *
 * @param sessionId - the session.
 * @param path - remote path.
 * @returns the entry's metadata.
 */
export function statEntry(sessionId: string, path: string): Promise<DirEntry> {
  return invoke("stat_entry", { sessionId, path });
}

/**
 * Rename/move an entry (remote if `sessionId` is given, else local).
 *
 * @param sessionId - the session to act on, or null to rename on the local disk.
 * @param from - the current path.
 * @param to - the destination path.
 */
export function renameEntry(sessionId: string | null, from: string, to: string): Promise<void> {
  return sessionId
    ? invoke("rename_entry", { sessionId, from, to })
    : invoke("local_rename", { from, to });
}

/**
 * Delete entries (remote if `sessionId` is given, else local).
 *
 * @param sessionId - the session to act on, or null to delete on the local disk.
 * @param paths - the paths to delete.
 * @param recursive - whether to delete directories and their contents.
 */
export function deleteEntries(
  sessionId: string | null,
  paths: string[],
  recursive: boolean,
): Promise<void> {
  return sessionId
    ? invoke("delete_entries", { sessionId, paths, recursive })
    : invoke("local_delete", { paths, recursive });
}

/**
 * Create a directory (remote if `sessionId` is given, else local).
 *
 * @param sessionId - the session to act on, or null to create on the local disk.
 * @param path - the directory path to create.
 */
export function makeDir(sessionId: string | null, path: string): Promise<void> {
  return sessionId ? invoke("mkdir", { sessionId, path }) : invoke("local_mkdir", { path });
}

/**
 * Set Unix permission bits (remote if `sessionId` is given, else local).
 *
 * @param sessionId - the session to act on, or null to chmod on the local disk.
 * @param path - the path to modify.
 * @param mode - the Unix permission bits to apply.
 */
export function setPermissions(
  sessionId: string | null,
  path: string,
  mode: number,
): Promise<void> {
  return sessionId
    ? invoke("set_permissions", { sessionId, path, mode })
    : invoke("local_set_permissions", { path, mode });
}

/**
 * Get the user's home directory (local pane default).
 *
 * @returns the absolute home path.
 */
export function localHomeDir(): Promise<string> {
  return invoke("local_home_dir");
}

/**
 * List a local directory.
 *
 * @param path - the local directory to list.
 * @returns the directory entries.
 */
export function localListDir(path: string): Promise<DirEntry[]> {
  return invoke("local_list_dir", { path });
}

/**
 * Get the current user settings (defaults on first run).
 *
 * @returns the persisted settings.
 */
export function getSettings(): Promise<Settings> {
  return invoke("get_settings");
}

/**
 * Save settings; the backend applies concurrency + conflict policy live.
 *
 * @param settings - the new settings.
 * @returns the stored (normalized) settings.
 */
export function saveSettings(settings: Settings): Promise<Settings> {
  return invoke("save_settings", { settings });
}

/**
 * Watch a local directory for external changes (retargets on navigation).
 *
 * @param path - the directory the local pane is now showing.
 * @returns a promise that resolves once watching; changes then arrive as
 *   `localDirChanged` session events.
 */
export function watchLocalDir(path: string): Promise<void> {
  return invoke("watch_local_dir", { path });
}

/**
 * Subscribe to session/prompt events. Call once at app start.
 *
 * @param onEvent - handler invoked for each event.
 * @returns a promise that resolves once the subscription is registered.
 */
export function subscribeSessionEvents(onEvent: (event: SessionEvent) => void): Promise<void> {
  const channel = new Channel<SessionEvent>();
  channel.onmessage = onEvent;
  return invoke("subscribe_session_events", { channel });
}

/**
 * Enqueue transfers.
 *
 * @param requests - the transfers to queue.
 * @returns the new transfer ids in request order.
 */
export function enqueueTransfers(requests: TransferRequest[]): Promise<string[]> {
  return invoke("enqueue_transfers", { requests });
}

/**
 * Recursively enqueue a directory transfer.
 *
 * @param sessionId - the session.
 * @param direction - "upload" or "download".
 * @param src - the source directory.
 * @param destParent - the destination directory to create the tree under.
 * @returns the enumerated file transfer ids.
 */
export function enqueueDirectory(
  sessionId: string,
  direction: TransferDirection,
  src: string,
  destParent: string,
): Promise<string[]> {
  return invoke("enqueue_directory", { sessionId, direction, src, destParent });
}

/**
 * Cancel a transfer.
 *
 * @param transferId - the transfer to cancel.
 */
export function cancelTransfer(transferId: string): Promise<void> {
  return invoke("cancel_transfer", { transferId });
}

/**
 * Pause a transfer (resumable from its current offset).
 *
 * @param transferId - the transfer to pause.
 */
export function pauseTransfer(transferId: string): Promise<void> {
  return invoke("pause_transfer", { transferId });
}

/**
 * Resume a paused transfer.
 *
 * @param transferId - the transfer to resume.
 */
export function resumeTransfer(transferId: string): Promise<void> {
  return invoke("resume_transfer", { transferId });
}

/** Pause all active transfers. */
export function pauseAllTransfers(): Promise<void> {
  return invoke("pause_all_transfers");
}

/**
 * Resolve a destination-exists conflict.
 *
 * @param transferId - the conflicted transfer.
 * @param resolution - overwrite/skip/rename/resume.
 * @param applyToAll - apply this choice to the rest of the batch.
 */
export function resolveConflict(
  transferId: string,
  resolution: ConflictResolution,
  applyToAll: boolean,
): Promise<void> {
  return invoke("resolve_conflict", { transferId, resolution, applyToAll });
}

/** Remove completed/failed/canceled transfers from the queue. */
export function clearCompleted(): Promise<void> {
  return invoke("clear_completed");
}

/**
 * Subscribe to transfer progress/state events. Call once at app start.
 *
 * @param onEvent - handler invoked for each transfer event.
 * @returns a promise that resolves once the subscription is registered.
 */
export function subscribeTransferEvents(onEvent: (event: TransferEvent) => void): Promise<void> {
  const channel = new Channel<TransferEvent>();
  channel.onmessage = onEvent;
  return invoke("subscribe_transfer_events", { channel });
}
