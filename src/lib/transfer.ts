// transfer.ts — Build transfer requests from a pane selection.
//
// Pure helper (no IPC/DOM) so the enqueue payloads are unit-testable. Directory
// entries are skipped for now — recursive directory transfers arrive in E3-S5.

import type { DirEntry, TransferDirection, TransferRequest } from "$lib/ipc/commands";
import { joinPath } from "$lib/utils/path";

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
