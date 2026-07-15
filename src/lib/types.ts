// types.ts — Shared UI-facing types.
//
// Lightweight view models used across the frontend. The authoritative
// backend-mirrored DTOs (matching src-tauri/dto.rs) arrive with the IPC layer
// in E1-S8; until then the shell renders mock data shaped like `FileEntry`.

/** Which of the two panes an entry or action belongs to. */
export type PaneKind = "local" | "remote";

/** The kind of a filesystem entry as shown in a pane. */
export type EntryKind = "file" | "dir" | "symlink";

/**
 * A single row in a file pane. Sizes are bytes; `mtime` is Unix epoch seconds.
 * `permissions` is a Unix mode (e.g. 0o644) or null when unknown/unsupported.
 */
export interface FileEntry {
  name: string;
  kind: EntryKind;
  size: number;
  mtime: number | null;
  permissions: number | null;
  /** Symlink target, when `kind === "symlink"`. */
  linkTarget?: string | null;
}
