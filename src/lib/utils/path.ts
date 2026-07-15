// path.ts — Path helpers that tolerate both POSIX (/) and Windows (\)
// separators, since local paths arrive in the OS's own syntax and remote
// (SFTP) paths are always POSIX.

/**
 * Detect the separator used by a path (defaults to "/").
 *
 * @param path - the path to inspect.
 * @returns "\\" for a Windows-style path with no forward slashes, else "/".
 */
function sep(path: string): string {
  return path.includes("\\") && !path.includes("/") ? "\\" : "/";
}

/**
 * Join a directory and a child name using the directory's separator.
 *
 * @param dir - the directory path.
 * @param name - the child entry name.
 * @returns the joined path.
 */
export function joinPath(dir: string, name: string): string {
  const s = sep(dir);
  return dir.endsWith(s) ? `${dir}${name}` : `${dir}${s}${name}`;
}

/**
 * The final component of a path.
 *
 * @param path - a path.
 * @returns the last segment (the file/dir name).
 */
export function basename(path: string): string {
  const s = sep(path);
  const trimmed = path.endsWith(s) ? path.slice(0, -1) : path;
  const idx = trimmed.lastIndexOf(s);
  return idx >= 0 ? trimmed.slice(idx + 1) : trimmed;
}

/**
 * The parent directory of a path.
 *
 * @param path - an absolute path.
 * @returns the parent path; the root maps to itself.
 */
export function parentPath(path: string): string {
  const s = sep(path);
  const trimmed = path.endsWith(s) && path.length > 1 ? path.slice(0, -1) : path;
  const idx = trimmed.lastIndexOf(s);
  if (idx <= 0) return s === "/" ? "/" : trimmed.slice(0, idx + 1);
  return trimmed.slice(0, idx);
}

/** A breadcrumb segment: its label and the full path it navigates to. */
export interface Segment {
  label: string;
  path: string;
}

/**
 * Split a path into cumulative breadcrumb segments.
 *
 * @param path - an absolute path.
 * @returns segments from root to the leaf (root shown as the separator).
 */
export function pathSegments(path: string): Segment[] {
  const s = sep(path);
  const parts = path.split(s).filter((p) => p.length > 0);
  const segments: Segment[] = [
    { label: s === "/" ? "/" : (parts[0] ?? s), path: s === "/" ? "/" : `${parts[0] ?? ""}${s}` },
  ];
  let acc = s === "/" ? "" : (parts[0] ?? "");
  const start = s === "/" ? 0 : 1;
  for (let i = start; i < parts.length; i++) {
    acc = `${acc}${s}${parts[i]}`;
    segments.push({ label: parts[i], path: acc });
  }
  return segments;
}
