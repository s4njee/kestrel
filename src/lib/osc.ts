// osc.ts — Extract the shell's working directory from terminal output.
//
// Many shells announce their cwd to the terminal with an OSC escape sequence
// after every prompt, which is how tabs in iTerm/GNOME Terminal "remember" where
// you are. Kestrel reads the same announcements to follow the shell with the
// remote pane — no server-side configuration, and nothing typed into the user's
// session.
//
// Two encodings are recognised:
//   OSC 7    ESC ] 7 ; file://host/path        (BEL or ST terminated)
//   OSC 1337 ESC ] 1337 ; CurrentDir=/path     (iTerm2)
//
// The scanner is stateful because a sequence can straddle a read boundary — the
// PTY hands us arbitrary byte chunks, so `ESC ] 7 ; file://…` may arrive split
// down the middle. Incomplete tails are retained until the next chunk completes
// them (bounded, so a stray ESC ] can never grow the buffer without limit).

/** Longest partial escape sequence retained across chunks, in characters. */
const MAX_PENDING = 4096;

/** OSC introducer and the two accepted terminators. */
const OSC = "]";
const BEL = "";
const ST = "\\";

/**
 * Decode a `file://` URL (or bare path) into a filesystem path.
 *
 * OSC 7 payloads are URLs, so the path is percent-encoded and prefixed with an
 * optional hostname — `file://host/srv/my%20app` means `/srv/my app`.
 *
 * @param value - the raw payload after `7;`.
 * @returns the decoded absolute path, or null if it is not usable.
 */
function decodeFileUrl(value: string): string | null {
  let path = value;
  if (path.startsWith("file://")) {
    // Strip the scheme and the (possibly empty) authority up to the next slash.
    const afterScheme = path.slice("file://".length);
    const slash = afterScheme.indexOf("/");
    if (slash < 0) return null;
    path = afterScheme.slice(slash);
  }
  if (!path.startsWith("/")) return null;
  try {
    return decodeURIComponent(path);
  } catch {
    // Malformed percent-encoding: use it as-is rather than dropping the update.
    return path;
  }
}

/**
 * Interpret one OSC payload (the text between the introducer and terminator).
 *
 * @param payload - e.g. `7;file:///home/deploy` or `1337;CurrentDir=/srv`.
 * @returns the announced working directory, or null for any other OSC command.
 */
function parsePayload(payload: string): string | null {
  if (payload.startsWith("7;")) {
    return decodeFileUrl(payload.slice(2));
  }
  if (payload.startsWith("1337;")) {
    const body = payload.slice("1337;".length);
    if (body.startsWith("CurrentDir=")) {
      const path = body.slice("CurrentDir=".length);
      return path.startsWith("/") ? path : null;
    }
  }
  return null;
}

/**
 * Streaming scanner that pulls cwd announcements out of terminal output.
 *
 * Feed it every chunk the shell produces; it returns the newest directory the
 * shell announced in that chunk, or null when the chunk contained none.
 */
export class CwdScanner {
  /** Decoder kept across chunks so split UTF-8 codepoints survive. */
  #decoder = new TextDecoder("utf-8", { fatal: false });
  /** Unterminated escape-sequence tail carried into the next chunk. */
  #pending = "";

  /**
   * Consume a chunk of terminal output.
   *
   * @param bytes - raw bytes as received from the shell.
   * @returns the most recent working directory announced in this chunk, or null
   *   if it announced none. Ordinary output is ignored entirely.
   */
  push(bytes: Uint8Array): string | null {
    let buffer = this.#pending + this.#decoder.decode(bytes, { stream: true });
    this.#pending = "";
    let found: string | null = null;

    for (;;) {
      const start = buffer.indexOf(OSC);
      if (start < 0) {
        // No introducer in what remains. A chunk can still end *mid*-introducer
        // (a lone ESC, with the `]` arriving next time), so keep a trailing ESC
        // — dropping it loses sequences that split on that exact byte.
        if (buffer.endsWith("\x1b")) this.#pending = "\x1b";
        break;
      }

      const rest = buffer.slice(start + OSC.length);
      const bel = rest.indexOf(BEL);
      const st = rest.indexOf(ST);

      // Nearest terminator wins; each has its own length.
      let end: number;
      let terminatorLength: number;
      if (bel < 0 && st < 0) {
        end = -1;
        terminatorLength = 0;
      } else if (st < 0 || (bel >= 0 && bel < st)) {
        end = bel;
        terminatorLength = BEL.length;
      } else {
        end = st;
        terminatorLength = ST.length;
      }

      if (end < 0) {
        // Incomplete: keep the tail for the next chunk, bounded so a stray
        // introducer in binary output cannot grow this without limit.
        const tail = buffer.slice(start);
        this.#pending = tail.length <= MAX_PENDING ? tail : "";
        break;
      }

      const cwd = parsePayload(rest.slice(0, end));
      if (cwd !== null) found = cwd;

      buffer = rest.slice(end + terminatorLength);
    }

    return found;
  }

  /** Forget any partial sequence (e.g. when the shell is replaced). */
  reset(): void {
    this.#pending = "";
  }
}
