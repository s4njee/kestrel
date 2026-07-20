// osc.test.ts — Tests for the shell cwd announcement scanner (E8-S5).
//
// The chunk-splitting cases matter most: a PTY hands over arbitrary byte
// boundaries, so a sequence arriving in pieces is the normal case, not an edge
// case.

import { describe, it, expect } from "vitest";
import { CwdScanner } from "./osc";

const enc = new TextEncoder();

/** Feed a string to a scanner as a single chunk. */
function push(scanner: CwdScanner, text: string): string | null {
  return scanner.push(enc.encode(text));
}

describe("CwdScanner", () => {
  it("ignores ordinary output", () => {
    const s = new CwdScanner();
    expect(push(s, "total 24\r\ndrwxr-xr-x  5 deploy staff\r\n")).toBeNull();
  });

  it("reads an OSC 7 file URL terminated by BEL", () => {
    const s = new CwdScanner();
    expect(push(s, "\x1b]7;file://myhost/var/www\x07$ ")).toBe("/var/www");
  });

  it("reads an OSC 7 file URL terminated by ST", () => {
    const s = new CwdScanner();
    expect(push(s, "\x1b]7;file://myhost/srv/app\x1b\\")).toBe("/srv/app");
  });

  it("accepts an empty authority (file:///path)", () => {
    const s = new CwdScanner();
    expect(push(s, "\x1b]7;file:///home/deploy\x07")).toBe("/home/deploy");
  });

  it("percent-decodes paths", () => {
    const s = new CwdScanner();
    expect(push(s, "\x1b]7;file:///srv/my%20app%2Fv2\x07")).toBe("/srv/my app/v2");
  });

  it("reads iTerm2 OSC 1337 CurrentDir", () => {
    const s = new CwdScanner();
    expect(push(s, "\x1b]1337;CurrentDir=/opt/tools\x07")).toBe("/opt/tools");
  });

  it("returns the LAST announcement when a chunk holds several", () => {
    const s = new CwdScanner();
    const text = "\x1b]7;file:///first\x07ls\r\n\x1b]7;file:///second\x07";
    expect(push(s, text)).toBe("/second");
  });

  it("reassembles a sequence split across two chunks", () => {
    const s = new CwdScanner();
    expect(push(s, "output\x1b]7;file:///var")).toBeNull();
    expect(push(s, "/log\x07$ ")).toBe("/var/log");
  });

  it("reassembles a sequence split byte-by-byte", () => {
    const s = new CwdScanner();
    const text = "\x1b]7;file:///deep/path\x07";
    let last: string | null = null;
    for (const ch of text) {
      const got = push(s, ch);
      if (got !== null) last = got;
    }
    expect(last).toBe("/deep/path");
  });

  it("survives a UTF-8 codepoint split across chunks", () => {
    const s = new CwdScanner();
    const bytes = enc.encode("\x1b]1337;CurrentDir=/srv/café\x07");
    // Split inside the two-byte é.
    const cut = bytes.length - 3;
    expect(s.push(bytes.slice(0, cut))).toBeNull();
    expect(s.push(bytes.slice(cut))).toBe("/srv/café");
  });

  it("ignores unrelated OSC commands", () => {
    const s = new CwdScanner();
    // OSC 0 sets the window title — must not be mistaken for a cwd.
    expect(push(s, "\x1b]0;deploy@prod: /var/www\x07")).toBeNull();
    expect(push(s, "\x1b]2;another title\x07")).toBeNull();
  });

  it("ignores relative or malformed paths", () => {
    const s = new CwdScanner();
    expect(push(s, "\x1b]7;file://host\x07")).toBeNull();
    expect(push(s, "\x1b]1337;CurrentDir=relative/path\x07")).toBeNull();
    expect(push(s, "\x1b]7;notaurl\x07")).toBeNull();
  });

  it("keeps working after an unrelated OSC precedes a real one", () => {
    const s = new CwdScanner();
    const text = "\x1b]0;title\x07\x1b]7;file:///after/title\x07";
    expect(push(s, text)).toBe("/after/title");
  });

  it("does not grow its buffer without bound on a stray introducer", () => {
    const s = new CwdScanner();
    // A lone introducer followed by megabytes of binary-ish output (e.g. a user
    // cat-ing a binary) must not be retained forever.
    expect(push(s, "\x1b]" + "x".repeat(5000))).toBeNull();
    // The oversized tail was dropped, so a subsequent real sequence still works.
    expect(push(s, "\x1b]7;file:///recovered\x07")).toBe("/recovered");
  });

  it("reset() forgets a partial sequence", () => {
    const s = new CwdScanner();
    expect(push(s, "\x1b]7;file:///half")).toBeNull();
    s.reset();
    // Without the retained head, the completion alone is just ordinary output.
    expect(push(s, "/done\x07")).toBeNull();
  });
});
