// logs.svelte.ts — Session log stream for the status console (Svelte 5 runes).
//
// A capped, append-only list of tagged log lines (Status/Command/Response/…)
// shown in the terminal-grid console. Fed by routed session events and shell
// actions (connect, navigate, errors) to mirror a real SFTP session log.

/** Severity/kind of a log line (drives its color). */
export type LogClass = "info" | "ok" | "cmd" | "resp" | "err";

/** A single console log line. */
export interface LogLine {
  id: number;
  /** Leading tag, e.g. "Status:" / "Command:" / "Response:". */
  tag: string;
  cls: LogClass;
  text: string;
}

/** How many lines to retain (older lines are dropped). */
const MAX_LINES = 200;

class LogsStore {
  #lines = $state<LogLine[]>([]);
  #nextId = 0;

  /**
   * The retained log lines, oldest first.
   *
   * @returns the live reactive array (not a copy); empty until the first push, and
   *   capped at MAX_LINES with older lines dropped.
   */
  get lines(): LogLine[] {
    return this.#lines;
  }

  /**
   * Append a log line (dropping the oldest past the cap).
   *
   * @param tag - the leading tag (e.g. "Status:").
   * @param cls - the line's kind/severity.
   * @param text - the message body.
   */
  push(tag: string, cls: LogClass, text: string): void {
    const line = { id: this.#nextId++, tag, cls, text };
    const next = [...this.#lines, line];
    this.#lines = next.length > MAX_LINES ? next.slice(next.length - MAX_LINES) : next;
  }

  /**
   * Append a `Status:` line.
   *
   * @param text - the message body.
   * @param ok - true to use the success ("ok") styling; defaults to false for neutral "info".
   */
  status(text: string, ok = false): void {
    this.push("Status:", ok ? "ok" : "info", text);
  }

  /**
   * Append a `Command:` line.
   *
   * @param text - the command text to display.
   */
  command(text: string): void {
    this.push("Command:", "cmd", text);
  }

  /**
   * Append a `Response:` line.
   *
   * @param text - the server response text to display.
   */
  response(text: string): void {
    this.push("Response:", "resp", text);
  }

  /**
   * Append an `Error:` line.
   *
   * @param text - the error message to display.
   */
  error(text: string): void {
    this.push("Error:", "err", text);
  }
}

/** Application-wide logs store singleton. */
export const logs = new LogsStore();
