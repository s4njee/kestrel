// format.ts — Presentation helpers for sizes, rates, and dates.
//
// Pure functions shared by the file panes, transfer rows, and status bar. No
// DOM or IPC access here — safe to unit-test in a node environment.

const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

/**
 * Format a byte count as a human-readable string using binary (1024) steps.
 *
 * @param bytes - the number of bytes (non-negative; negatives are clamped to 0).
 * @param fractionDigits - decimal places for values >= 1 KB (default 1).
 * @returns a string like "0 B", "512 B", "1.5 KB", "2.0 GB".
 */
export function formatBytes(bytes: number, fractionDigits = 1): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), UNITS.length - 1);
  const value = bytes / Math.pow(1024, exponent);
  // Bytes are always whole; larger units get fixed decimals.
  const text = exponent === 0 ? String(Math.round(value)) : value.toFixed(fractionDigits);
  return `${text} ${UNITS[exponent]}`;
}

/**
 * Format a transfer rate in bytes-per-second as a human-readable string.
 *
 * @param bytesPerSecond - the current rate; non-positive yields "0 B/s".
 * @returns a string like "1.5 MB/s".
 */
export function formatRate(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`;
}

/**
 * Format a Unix epoch timestamp as a locale date-time string.
 *
 * @param epochSeconds - seconds since the Unix epoch.
 * @returns a locale-formatted "YYYY-MM-DD, HH:MM"-style string, or "—" when the
 *   input is missing or invalid.
 */
export function formatMtime(epochSeconds: number | null | undefined): string {
  if (epochSeconds == null || !Number.isFinite(epochSeconds)) return "—";
  const d = new Date(epochSeconds * 1000);
  if (Number.isNaN(d.getTime())) return "—";
  return d.toLocaleString(undefined, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}
