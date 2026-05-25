#!/usr/bin/env python3
"""Measure the size of the runtime implementation.

One combined report over two bodies of source:
  - Rust under runtime/src — every `*.rs` with embedded `#[cfg(test)]`
    blocks stripped out. The stripper is string-/comment-/char-literal-aware
    so braces inside string literals or comments don't fool it.
  - Evident under stdlib/passes — every `*.ev` self-hosted pass. Evident
    comments are `--` to end of line; the classifier strips those (and is
    string-literal-aware) before counting code lines and tokens.

A single summary table (Rust / Evident / Total columns), one combined
file-length histogram, and one combined longest-files list — sized to fit
on a screen. Comment lines, blank lines, and raw char counts are
deliberately not reported.

Usage:
    scripts/runtime-size.py                 # summary, fits on a page
    scripts/runtime-size.py --per-file      # also dump a per-file table
"""

import argparse
import math
import os
import re
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RUST_ROOT = os.path.join(REPO_ROOT, "runtime/src")
EVIDENT_ROOT = os.path.join(REPO_ROOT, "stdlib/passes")

CFG_TEST = re.compile(r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]")
WORD = re.compile(r"\S+")
# A "code token" approximation: identifiers/numbers, or runs of operator
# punctuation, treated as individual tokens.
LEX_TOKEN = re.compile(r"[A-Za-z_][A-Za-z0-9_]*|[0-9][0-9_.]*|[^\sA-Za-z0-9_]")


def classify_rust(text: str) -> tuple[list[bool], list[bool]]:
    """Return two per-char masks: (in_code, is_comment).

    in_code[i] is True where the char is in normal code state (not inside a
    //... or /*...*/ comment, a "..."/r"..." string, or a '.' char literal).
    Delimiter chars are marked False; that's fine since none of them are braces.

    is_comment[i] is True only for chars inside a // or /* */ comment — this
    lets callers strip comments while keeping string/char literals (which are
    code)."""
    n = len(text)
    in_code = [True] * n
    is_comment = [False] * n
    i = 0

    def ident_char(ch: str) -> bool:
        return ch.isalnum() or ch == "_"

    while i < n:
        c = text[i]

        # line comment
        if c == "/" and i + 1 < n and text[i + 1] == "/":
            while i < n and text[i] != "\n":
                in_code[i] = False
                is_comment[i] = True
                i += 1
            continue

        # block comment (Rust allows nesting)
        if c == "/" and i + 1 < n and text[i + 1] == "*":
            depth = 1
            in_code[i] = in_code[i + 1] = False
            is_comment[i] = is_comment[i + 1] = True
            i += 2
            while i < n and depth > 0:
                if text[i] == "/" and i + 1 < n and text[i + 1] == "*":
                    depth += 1
                    in_code[i] = in_code[i + 1] = False
                    is_comment[i] = is_comment[i + 1] = True
                    i += 2
                elif text[i] == "*" and i + 1 < n and text[i + 1] == "/":
                    depth -= 1
                    in_code[i] = in_code[i + 1] = False
                    is_comment[i] = is_comment[i + 1] = True
                    i += 2
                else:
                    in_code[i] = False
                    is_comment[i] = True
                    i += 1
            continue

        # raw string: (b?r) #* "  ... " #*   — only at a token boundary
        if c in "rb" and (i == 0 or not ident_char(text[i - 1])):
            m = re.match(r"b?r(#*)\"", text[i:])
            if m:
                hashes = m.group(1)
                close = '"' + hashes
                body_start = i + m.end()
                end = text.find(close, body_start)
                end = n if end == -1 else end + len(close)
                for k in range(i, min(end, n)):
                    in_code[k] = False
                i = end
                continue

        # byte string b"..." or normal string "..."
        if c == '"' or (c == "b" and i + 1 < n and text[i + 1] == '"'):
            j = i + (2 if c == "b" else 1)
            in_code[i] = False
            if c == "b":
                in_code[i + 1] = False
            while j < n:
                in_code[j] = False
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    j += 1
                    break
                j += 1
            i = j
            continue

        # char literal '.'  vs lifetime 'a  — only treat as literal if it
        # closes with a matching quote.
        if c == "'":
            m = re.match(r"'(\\u\{[0-9A-Fa-f_]+\}|\\.|[^'\\\n])'", text[i:])
            if m:
                for k in range(i, i + m.end()):
                    in_code[k] = False
                i += m.end()
                continue
            # lifetime or stray quote: leave as code, advance one char
            i += 1
            continue

        i += 1

    return in_code, is_comment


def classify_evident(text: str) -> tuple[list[bool], list[bool]]:
    """Return (in_code, is_comment) masks for Evident source.

    Evident comments run from `--` to end of line; strings are "..." with
    backslash escapes. in_code is False inside comments and string literals;
    is_comment is True only inside `--` comments."""
    n = len(text)
    in_code = [True] * n
    is_comment = [False] * n
    i = 0
    while i < n:
        c = text[i]

        # line comment: -- to end of line
        if c == "-" and i + 1 < n and text[i + 1] == "-":
            while i < n and text[i] != "\n":
                in_code[i] = False
                is_comment[i] = True
                i += 1
            continue

        # string literal "..."
        if c == '"':
            in_code[i] = False
            j = i + 1
            while j < n:
                in_code[j] = False
                if text[j] == "\\":
                    if j + 1 < n:
                        in_code[j + 1] = False
                    j += 2
                    continue
                if text[j] == '"':
                    j += 1
                    break
                j += 1
            i = j
            continue

        i += 1

    return in_code, is_comment


def strip_comments(text: str, classify) -> str:
    """Remove every comment, preserving string/char literals.

    Comment chars (including any newlines inside a block comment) are dropped;
    line-comment newlines survive. Used so token counts reflect code only."""
    _, is_comment = classify(text)
    return "".join(ch for ch, drop in zip(text, is_comment) if not drop)


def strip_cfg_test(text: str) -> tuple[str, int]:
    """Remove every in-code `#[cfg(test)]` item. Returns (stripped, lines_removed)."""
    in_code, _ = classify_rust(text)
    n = len(text)
    spans: list[tuple[int, int]] = []

    for m in CFG_TEST.finditer(text):
        if not in_code[m.start()]:
            continue
        start = m.start()
        # find the item the attribute applies to: first in-code '{' (a block
        # item like `mod tests { ... }`) or ';' (a statement item) after it.
        j = m.end()
        brace_open = None
        semi = None
        while j < n:
            if in_code[j]:
                if text[j] == "{":
                    brace_open = j
                    break
                if text[j] == ";":
                    semi = j
                    break
            j += 1

        if brace_open is not None:
            depth = 0
            p = brace_open
            end = n
            while p < n:
                if in_code[p]:
                    if text[p] == "{":
                        depth += 1
                    elif text[p] == "}":
                        depth -= 1
                        if depth == 0:
                            end = p + 1
                            break
                p += 1
            spans.append((start, end))
        elif semi is not None:
            spans.append((start, semi + 1))

    lines_removed = 0
    for s, e in spans:
        lines_removed += text.count("\n", s, e)

    # remove spans back-to-front so indices stay valid
    out = text
    for s, e in sorted(spans, reverse=True):
        out = out[:s] + out[e:]
    return out, lines_removed


def count_lines(text: str, classify) -> dict:
    in_code, _ = classify(text)
    lines = text.split("\n")
    # drop a single trailing empty element from a final newline
    if lines and lines[-1] == "":
        lines.pop()

    total = len(lines)
    code = 0
    offset = 0
    for ln in lines:
        if ln.strip():
            # a "code line" has at least one non-whitespace char in code state
            has_code = any(
                in_code[offset + k] and not ch.isspace()
                for k, ch in enumerate(ln)
            )
            if has_code:
                code += 1
        offset += len(ln) + 1  # +1 for the newline we split on

    return {"total": total, "code": code}


def count_tokens(text: str) -> dict:
    return {
        "words": len(WORD.findall(text)),
        "lex_tokens": len(LEX_TOKEN.findall(text)),
        "approx_llm": math.ceil(len(text) / 4),
    }


BUCKETS = [
    ("0–49", 0, 50),
    ("50–99", 50, 100),
    ("100–199", 100, 200),
    ("200–499", 200, 500),
    ("500–999", 500, 1000),
    ("1000+", 1000, float("inf")),
]


def print_histogram(totals: list[int]) -> None:
    counts = [sum(1 for n in totals if lo <= n < hi) for _, lo, hi in BUCKETS]
    peak = max(counts) or 1
    width = 30
    print("  File length distribution")
    for (label, _, _), c in zip(BUCKETS, counts):
        bar = "█" * round(c / peak * width)
        print(f"  {label:>9} │ {bar} {c}")


def print_longest(rows: list[tuple], k: int = 6) -> None:
    by_len = sorted(rows, key=lambda r: r[1], reverse=True)
    k = min(k, len(rows))
    print(f"  Longest {k} files")
    for name, total, *_ in by_len[:k]:
        print(f"    {total:>6,}  {name}")


def gather(root: str, ext: str) -> list[str]:
    files = []
    for dirpath, dirnames, filenames in os.walk(root):
        if "target" in dirnames:
            dirnames.remove("target")
        for f in filenames:
            if f.endswith(ext):
                files.append(os.path.join(dirpath, f))
    return sorted(files)


def collect(root: str, ext: str, classify, strip_tests: bool) -> tuple[dict, list]:
    """Aggregate counts + per-file rows for one source body. Rows carry the
    repo-relative path so combined listings are unambiguous."""
    agg = {"files": 0, "total": 0, "code": 0,
           "words": 0, "lex_tokens": 0, "approx_llm": 0}
    rows = []
    for path in gather(root, ext):
        with open(path, encoding="utf-8") as fh:
            text = fh.read()
        if strip_tests:
            text, _ = strip_cfg_test(text)
        code_only = strip_comments(text, classify)
        lc = count_lines(text, classify)
        tc = count_tokens(code_only)
        agg["files"] += 1
        for key in ("total", "code"):
            agg[key] += lc[key]
        for key in ("words", "lex_tokens", "approx_llm"):
            agg[key] += tc[key]
        rows.append((os.path.relpath(path, REPO_ROOT), lc["total"], lc["code"],
                     tc["approx_llm"]))
    return agg, rows


def print_summary(rust: dict, ev: dict) -> None:
    total = {k: rust[k] + ev[k] for k in rust}
    fields = [("files", "files"), ("lines", "total"), ("  code", "code"),
              ("LLM tokens", "approx_llm"), ("  lexical", "lex_tokens"),
              ("  words", "words")]
    print(f"  {'':<12}{'Rust':>11}{'Evident':>11}{'Total':>11}")
    for label, key in fields:
        print(f"  {label:<12}{rust[key]:>11,}{ev[key]:>11,}{total[key]:>11,}")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--per-file", action="store_true",
                    help="print a combined per-file breakdown table")
    args = ap.parse_args()

    rust_agg, rust_rows = collect(RUST_ROOT, ".rs", classify_rust, strip_tests=True)
    ev_agg, ev_rows = collect(EVIDENT_ROOT, ".ev", classify_evident, strip_tests=False)
    rows = rust_rows + ev_rows

    print("Runtime size — Rust runtime/src + Evident stdlib/passes\n")
    print_summary(rust_agg, ev_agg)
    print()
    print_histogram([r[1] for r in rows])
    print()
    print_longest(rows)

    if args.per_file:
        rows.sort(key=lambda r: r[1], reverse=True)
        print(f"\n{'file':<48}{'lines':>8}{'code':>8}{'~tokens':>10}")
        print("-" * 74)
        for name, total, code, tok in rows:
            print(f"{name:<48}{total:>8}{code:>8}{tok:>10}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
