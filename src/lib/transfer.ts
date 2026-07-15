// transfer.ts — Build transfer requests from a pane selection.
//
// Pure helper (no IPC/DOM) so the enqueue payloads are unit-testable. Directory
// entries are skipped for now — recursive directory transfers arrive in E3-S5.

import type { DirEntry, TransferDirection, TransferRequest } from "$lib/ipc/commands";
import type { PaneKind } from "$lib/types";
import { basename, joinPath } from "$lib/utils/path";

/**
 * The transfer direction implied by dragging from one pane to another.
 *
 * @param source - the pane the drag started in.
 * @param target - the pane it was dropped on.
 * @returns "upload" (local→remote), "download" (remote→local), or null for a
 *   same-pane drop (no transfer).
 */
export function dropDirection(source: PaneKind, target: PaneKind): TransferDirection | null {
  if (source === target) return null;
  return source === "local" ? "upload" : "download";
}

/**
 * Build upload requests for OS-dropped absolute file paths.
 *
 * @param sessionId - the session.
 * @param paths - absolute local paths dropped onto the window.
 * @param destDir - the remote destination directory.
 * @returns one upload request per path (sizes unknown, filled at transfer time).
 */
export function uploadRequestsForPaths(
  sessionId: string,
  paths: string[],
  destDir: string,
): TransferRequest[] {
  return paths.map((p) => ({
    sessionId,
    direction: "upload" as const,
    src: p,
    dest: joinPath(destDir, basename(p)),
    size: 0,
  }));
}

/**
 * Build one transfer request per selected file.
 *
 * @param direction - "upload" or "download".
 * @param sessionId - the session the transfers belong to.
 * @param entries - the selected source entries (directories are skipped).
 * @param destDir - the destination directory path.
 * @returns a request per file, sources kept in the input order.
 */
export function buildTransferRequests(
  direction: TransferDirection,
  sessionId: string,
  entries: DirEntry[],
  destDir: string,
): TransferRequest[] {
  return entries
    .filter((e) => e.kind === "file")
    .map((e) => ({
      sessionId,
      direction,
      src: e.path,
      dest: joinPath(destDir, e.name),
      size: e.size,
    }));
}
