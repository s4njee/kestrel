// perms.ts — Unix permission-bit helpers for the chmod dialog.
//
// Works with the low 9 mode bits as a 9-element boolean array in the order
// [owner r,w,x, group r,w,x, other r,w,x].

/** Convert a mode to its 9 rwx bits (owner, group, other). */
export function modeToBits(mode: number): boolean[] {
  const bits: boolean[] = [];
  for (let i = 0; i < 9; i++) {
    bits.push(((mode >> (8 - i)) & 1) === 1);
  }
  return bits;
}

/** Convert 9 rwx bits back to a mode value. */
export function bitsToMode(bits: boolean[]): number {
  let mode = 0;
  for (let i = 0; i < 9; i++) {
    if (bits[i]) mode |= 1 << (8 - i);
  }
  return mode;
}

/** Format the low 9 bits of a mode as a 3-digit octal string (e.g. "644"). */
export function formatOctal(mode: number): string {
  return (mode & 0o777).toString(8).padStart(3, "0");
}

/**
 * Parse a 1–3 digit octal permission string.
 *
 * @param text - an octal string like "644" or "755".
 * @returns the mode value, or null if invalid.
 */
export function parseOctal(text: string): number | null {
  if (!/^[0-7]{1,4}$/.test(text.trim())) return null;
  const value = parseInt(text.trim(), 8);
  return value >= 0 && value <= 0o7777 ? value & 0o777 : null;
}

/** Format a mode as an rwx string (e.g. "rwxr-xr--"). */
export function formatRwx(mode: number): string {
  const chars = ["r", "w", "x"];
  return modeToBits(mode)
    .map((bit, i) => (bit ? chars[i % 3] : "-"))
    .join("");
}
