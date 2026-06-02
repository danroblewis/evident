#!/usr/bin/env python3
"""Strip comments from Rust (and similar) source.

Reads source from stdin or a file path. Writes stripped output to stdout.

Handles:
  - `//` line comments
  - `/* ... */` block comments (multi-line, non-nested)
  - Skips `//`-like sequences inside string literals (`"..."`),
    raw strings (`r"..."`, `r#"..."#`, etc.), and char literals (`'x'`)
  - Distinguishes char literals from lifetimes by checking for a
    closing `'` before a newline.

Also collapses runs of blank lines down to one and trims trailing
whitespace from each line.

Usage:
    strip-comments.py path/to/file.rs  > stripped.rs
    cat file.rs | strip-comments.py    > stripped.rs

The output is NOT guaranteed to be a verbatim semantic-preserving
strip (it's a best-effort heuristic suitable for dumping into an LLM
context, not for compilation).
"""
import sys


def strip_rust_comments(text: str) -> str:
    out = []
    i = 0
    n = len(text)
    while i < n:
        c = text[i]

        # Double-quoted string.
        if c == '"':
            out.append(c)
            i += 1
            while i < n:
                if text[i] == "\\" and i + 1 < n:
                    out.append(text[i:i + 2])
                    i += 2
                    continue
                if text[i] == '"':
                    out.append('"')
                    i += 1
                    break
                out.append(text[i])
                i += 1
            continue

        # Raw string: r"..." or r#"..."# (any number of #).
        if c == "r" and i + 1 < n and (text[i + 1] == '"' or text[i + 1] == "#"):
            j = i + 1
            hashes = 0
            while j < n and text[j] == "#":
                hashes += 1
                j += 1
            if j < n and text[j] == '"':
                # Confirmed raw string; copy it whole including its closing.
                close = '"' + "#" * hashes
                end = text.find(close, j + 1)
                if end < 0:
                    out.append(text[i:])
                    i = n
                else:
                    out.append(text[i:end + len(close)])
                    i = end + len(close)
                continue

        # Char literal vs lifetime. Char is 'x' or '\n', short and ends with '.
        if c == "'":
            # Look for a closing ' within the next few chars on the same line.
            j = i + 1
            if j < n and text[j] == "\\" and j + 1 < n:
                # Escaped: 'x' style. Consume the escape sequence then look for '.
                k = j + 2
                while k < n and text[k] != "'" and text[k] != "\n":
                    k += 1
                if k < n and text[k] == "'":
                    out.append(text[i:k + 1])
                    i = k + 1
                    continue
            elif j < n and text[j] != "'" and j + 1 < n and text[j + 1] == "'":
                # 'x' — simple char.
                out.append(text[i:j + 2])
                i = j + 2
                continue
            # Otherwise treat as a lifetime token: emit ' and continue.
            out.append(c)
            i += 1
            continue

        # Comments.
        if c == "/" and i + 1 < n:
            nxt = text[i + 1]
            if nxt == "/":
                # Line comment — skip until newline (newline itself preserved).
                end = text.find("\n", i)
                if end < 0:
                    i = n
                else:
                    i = end
                continue
            if nxt == "*":
                # Block comment — skip until matching */.
                end = text.find("*/", i + 2)
                if end < 0:
                    i = n
                else:
                    i = end + 2
                continue

        out.append(c)
        i += 1
    return "".join(out)


def collapse_blanks(text: str) -> str:
    """Trim trailing whitespace from each line, then collapse runs of
    blank lines down to one. Removes the bulk of post-strip whitespace
    while keeping a single visual break between top-level items."""
    lines = [ln.rstrip() for ln in text.splitlines()]
    out = []
    blank_run = 0
    for ln in lines:
        if ln == "":
            blank_run += 1
            if blank_run <= 1:
                out.append("")
        else:
            blank_run = 0
            out.append(ln)
    return "\n".join(out) + ("\n" if text.endswith("\n") else "")


def main(argv):
    if len(argv) > 1 and argv[1] not in ("-", "--stdin"):
        with open(argv[1]) as f:
            text = f.read()
    else:
        text = sys.stdin.read()
    sys.stdout.write(collapse_blanks(strip_rust_comments(text)))


if __name__ == "__main__":
    main(sys.argv)
