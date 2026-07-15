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
  | { method: "key"; path: string; passphrase: string | null };

/** A connection request (mirrors `ConnectRequest`). */
export interface ConnectRequest {
  host: string;
  port: number;
  username: string;
  auth: Auth;
}

/** A reply to a prompt (mirrors `PromptReplyDto`). */
export type PromptReply = { type: "hostKey"; accept: boolean };

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
