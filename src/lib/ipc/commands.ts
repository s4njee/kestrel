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

/** Auth choice sent with a connect request (mirrors `AuthDto`). */
export type Auth =
  | { method: "password"; password: string }
  | { method: "key"; path: string; passphrase: string | null }
  | { method: "agent" };

/** A connection request (mirrors `ConnectRequest`). */
export interface ConnectRequest {
  host: string;
  port: number;
  username: string;
  auth: Auth;
}

/** A reply to a prompt (mirrors `PromptReplyDto`). */
export type PromptReply = { type: "hostKey"; accept: boolean };

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
  "queued" | "running" | "paused" | "awaitingUser" | "done" | "skipped" | "failed" | "canceled";

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
    };

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
 * Answer a pending prompt (e.g. host-key trust).
 *
 * @param promptId - the prompt id from the event.
 * @param reply - the user's decision.
 */
export function respondPrompt(promptId: string, reply: PromptReply): Promise<void> {
  return invoke("respond_prompt", { promptId, reply });
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

/** Rename/move an entry (remote if `sessionId` is given, else local). */
export function renameEntry(sessionId: string | null, from: string, to: string): Promise<void> {
  return sessionId
    ? invoke("rename_entry", { sessionId, from, to })
    : invoke("local_rename", { from, to });
}

/** Delete entries (remote if `sessionId` is given, else local). */
export function deleteEntries(
  sessionId: string | null,
  paths: string[],
  recursive: boolean,
): Promise<void> {
  return sessionId
    ? invoke("delete_entries", { sessionId, paths, recursive })
    : invoke("local_delete", { paths, recursive });
}

/** Create a directory (remote if `sessionId` is given, else local). */
export function makeDir(sessionId: string | null, path: string): Promise<void> {
  return sessionId ? invoke("mkdir", { sessionId, path }) : invoke("local_mkdir", { path });
}

/** Set Unix permission bits (remote if `sessionId` is given, else local). */
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
