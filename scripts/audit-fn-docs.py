#!/usr/bin/env python3
"""Audit function-header coverage across Rust / TS / Svelte sources.

For every function-like definition, extract the real signature via balanced-paren
scanning, then report whether the preceding doc comment documents its arguments
and its return value.

Covers, per language:
  Rust  — every `fn` (free functions, inherent + trait impl methods).
  TS/Svelte — `function` declarations, exported arrow-function consts, and
              **class methods and getters** (previously unaudited, which silently
              exempted every runes-store method).

Test files and `mod tests` blocks are excluded: assertions document themselves.
"""
import re
import subprocess

# Words that look like calls at the start of a line but are not definitions.
KEYWORDS = {
    "if", "for", "while", "switch", "catch", "return", "function", "class",
    "constructor", "await", "typeof", "new", "do", "else", "try", "throw",
    "super", "import", "export", "const", "let", "var", "case", "with", "yield",
}


def files(pattern, *roots):
    """List source files under `roots`, excluding tests.

    Arguments: `pattern` — a find(1) -name glob; `roots` — directories to search.
    Returns: sorted paths with `.test.` files removed.
    """
    out = subprocess.run(["find", *roots, "-name", pattern],
                         capture_output=True, text=True).stdout
    return sorted(p for p in out.splitlines() if p and ".test." not in p)


def signature(text, start, rust):
    """Return (params, ret, tail) by scanning balanced parens from the definition's '('.

    `tail` is the text following the closing paren, which lets callers reject
    *call sites*: a definition continues with `{`, `=>`, or a return type, while
    a call like `walk(a, 0);` continues with `;`. Without that check an indented
    call inside a class body is indistinguishable from a method.

    `start` MUST be the offset just after the function *name*. Scanning from the
    line start instead would latch onto the paren in a `pub(crate)` visibility
    qualifier, which both fabricates a phantom `crate` parameter and hides the
    real return type.
    """
    i = text.find("(", start)
    if i < 0:
        return "", "", ""
    depth, j = 0, i
    while j < len(text):
        if text[j] == "(":
            depth += 1
        elif text[j] == ")":
            depth -= 1
            if depth == 0:
                break
        j += 1
    params = text[i + 1:j]
    tail = text[j + 1:j + 300]
    # Rust: `-> T {`. TS/Svelte: `: T {` or `: T =>`.
    pat = r"\s*->\s*([^{;]+)" if rust else r"\s*:\s*([^{=]+)"
    m = re.match(pat, tail)
    ret = m.group(1).strip() if m else ""
    return params, ret, tail


def prev_doc(lines, i, rust):
    """Collect the doc comment immediately above line `i`.

    Arguments: `lines` — file lines; `i` — the definition's line index;
    `rust` — select `///` vs `/** */` comment syntax.
    Returns: the joined comment text ("" when the definition is undocumented).
    """
    doc, j = [], i - 1
    while j >= 0:
        s = lines[j].strip()
        if not s or s.startswith("#[") or s.startswith("//"):
            if rust and s.startswith("///"):
                doc.append(s)
            j -= 1
            continue
        if not rust and (s.startswith("/**") or s.startswith("*") or s.endswith("*/")):
            doc.append(s)
            j -= 1
            continue
        break
    return "\n".join(reversed(doc))


def real_params(params, rust):
    """Whether a parameter list has anything beyond `self`.

    Arguments: `params` — the raw text between the parens; `rust` — strip `self`.
    Returns: True when at least one documentable parameter remains.
    """
    p = params.strip()
    if rust:
        p = re.sub(r"^&?\s*(mut\s+)?self\s*,?", "", p).strip()
    return bool(p)


def check(report, path, line_no, name, params, ret, doc, rust):
    """Record any documentation gap for one definition.

    Arguments: `report` — accumulator; `path`/`line_no`/`name` — where it is;
    `params`/`ret` — the parsed signature; `doc` — the preceding comment;
    `rust` — selects the expected doc vocabulary.
    Returns: None; appends to `report` when something is missing.
    """
    if not doc:
        report.append((path, line_no, name, "NO DOC"))
        return
    low = doc.lower()
    miss = []
    arg_ok = "argument" in low or "@param" in low
    ret_ok = "return" in low or "@return" in low
    if real_params(params, rust) and not arg_ok:
        miss.append("args")
    void = ("()",) if rust else ("void", "Promise<void>")
    if ret and ret not in void and not ret_ok:
        miss.append("returns")
    if miss:
        report.append((path, line_no, name, "missing " + "+".join(miss)))


def is_ts_definition(tail):
    """Whether a TS/Svelte signature tail belongs to a definition, not a call.

    Arguments: `tail` — text immediately after the closing paren.
    Returns: True when a body `{`, an arrow `=>`, or a return type followed by
    one of those comes next; False for statements such as `foo(a);`.
    """
    return bool(re.match(r"\s*(:\s*[^{;=]+)?\s*(\{|=>)", tail))


report = []

# ---- Rust -------------------------------------------------------------------
for f in files("*.rs", "crates/engine/src", "src-tauri/src"):
    text = open(f).read()
    lines = text.splitlines()
    offs, pos = [], 0
    for ln in lines:
        offs.append(pos)
        pos += len(ln) + 1
    in_test = False
    for i, ln in enumerate(lines):
        if re.match(r"\s*mod tests\b", ln):
            in_test = True
        if in_test:
            continue
        m = re.match(r"\s*(pub(\([^)]*\))?\s+)?(const\s+)?(async\s+)?(unsafe\s+)?fn\s+(\w+)", ln)
        if not m:
            continue
        params, ret, _tail = signature(text, offs[i] + m.end(), rust=True)
        check(report, f, i + 1, m.group(6), params, ret,
              prev_doc(lines, i, rust=True), rust=True)

# ---- TS / Svelte ------------------------------------------------------------
for f in files("*.ts", "src") + files("*.svelte", "src"):
    text = open(f).read()
    lines = text.splitlines()
    offs, pos = [], 0
    for ln in lines:
        offs.append(pos)
        pos += len(ln) + 1

    # Track class bodies so indented `name(...)` lines can be recognised as
    # methods rather than mistaken for calls.
    class_depth = None
    depth = 0
    for i, ln in enumerate(lines):
        stripped = ln.strip()

        if class_depth is None and re.search(r"\bclass\s+\w+", ln):
            class_depth = depth

        matched = None  # (name, offset_after_name)

        # Plain function declarations.
        m = re.match(r"\s*(export\s+)?(async\s+)?function\s+(\w+)", ln)
        if m:
            matched = (m.group(3), m.end())

        # Exported arrow-function consts: `export const f = (a): B => …`.
        if not matched:
            m = re.match(r"\s*(export\s+)?const\s+(\w+)\s*=\s*(async\s+)?\(", ln)
            if m and "=>" in ln:
                matched = (m.group(2), m.end() - 1)

        # Class methods and getters/setters.
        if not matched and class_depth is not None and depth > class_depth:
            m = re.match(
                r"\s*(?:public\s+|private\s+|protected\s+|static\s+|readonly\s+)*"
                r"(?:(get|set)\s+)?(#?\w+)\s*\(", ln)
            if m and m.group(2).lstrip("#") not in KEYWORDS and not stripped.startswith("}"):
                matched = (m.group(2), m.end() - 1)

        if matched:
            name, end = matched
            params, ret, tail = signature(text, offs[i] + end, rust=False)
            # Skip call sites that merely look like definitions.
            if is_ts_definition(tail):
                check(report, f, i + 1, name, params, ret,
                      prev_doc(lines, i, rust=False), rust=False)

        depth += ln.count("{") - ln.count("}")
        if class_depth is not None and depth <= class_depth:
            class_depth = None

for f, ln, name, why in report:
    print(f"{why:16} {f}:{ln}  {name}()")
print(f"\n--- {len(report)} function(s) need attention ---")
