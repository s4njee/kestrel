// keymap.ts — Global keyboard-shortcut resolution.
//
// Pure mapping from a KeyboardEvent to an app action, so the shortcut map is
// unit-testable without rendering the shell. The shell (+page) owns dispatch and
// context checks (e.g. rename needs a selection); this module only decides which
// action a keystroke means. Typing in an input/textarea suppresses shortcuts.

/** An action a shortcut can trigger. */
export type ShortcutAction =
  "refresh" | "focusPath" | "download" | "upload" | "rename" | "delete" | "switchPane" | "palette";

/** Minimal shape of the key event fields we consult (eases testing). */
export interface KeyLike {
  key: string;
  metaKey?: boolean;
  ctrlKey?: boolean;
  target?: EventTarget | null;
}

/**
 * Whether the event originates from an editable field (don't hijack typing).
 *
 * @param target - the event target to inspect; may be null/undefined.
 * @returns true if the target is an input, textarea, or contenteditable element.
 */
function isEditable(target: EventTarget | null | undefined): boolean {
  const el = target as HTMLElement | null;
  return !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable);
}

/**
 * Resolve a keyboard event to a shortcut action.
 *
 * @param event - the key event (only key/meta/ctrl/target are read).
 * @returns the action to perform, or null if the keystroke is not a shortcut
 *   (or was typed into an input).
 */
export function resolveShortcut(event: KeyLike): ShortcutAction | null {
  if (isEditable(event.target)) return null;

  // Non-modifier shortcuts.
  switch (event.key) {
    case "F2":
      return "rename";
    case "Delete":
    case "Backspace":
      return "delete";
    case "Tab":
      return "switchPane";
  }

  // Cmd (macOS) / Ctrl (elsewhere) shortcuts.
  const meta = event.metaKey || event.ctrlKey;
  if (!meta) return null;
  switch (event.key) {
    case "k":
      return "palette";
    case "r":
      return "refresh";
    case "l":
      return "focusPath";
    case "d":
      return "download";
    case "u":
      return "upload";
    default:
      return null;
  }
}
