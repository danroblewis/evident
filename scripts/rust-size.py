#!/usr/bin/env python3
"""Measure the size of the Rust *implementation* under runtime/src.

What counts as "implementation":
  - All `*.rs` under the scan root (default: runtime/src), which by
    construction excludes runtime/tests/ and runtime/examples/ (they are
    siblings of src/, not children).
  - With every embedded `#[cfg(test)]` block stripped out — the unit tests
    that live at the bottom (or middle) of implementation files.

The `#[cfg(test)]` stripper is string-/comment-/char-literal-aware: it will
not be fooled by braces that appear inside string literals (e.g. the
`"\\u{a}"` escapes in translate/extract.rs) or comments, and it handles
`#[cfg(test)]` blocks that are followed by more implementation code rather
than sitting at end-of-file.

Reports line count and token count for the stripped implementation. The line
breakdown keeps comments (so the comment-line tally survives), but the token
counts are measured on comment-stripped code — // and /* */ are removed first,
while string and char literals (which are code) are kept.

Usage:
    scripts/rust-size.py                 # scan runtime/src, print summary
    scripts/rust-size.py --per-file      # also print a per-file table
    scripts/rust-size.py path/to/src     # scan a different root
"""

import argparse
import math
import os
import re
import sys

CFG_TEST = re.compile(r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]")
WORD = re.compile(r"\S+")
# A "code token" approximation: identifiers/numbers, or runs of operator
# punctuation, treated as individual tokens.
LEX_TOKEN = re.compile(r"[A-Za-z_][A-Za-z0-9_]*|[0-9][0-9_.]*|[^\sA-Za-z0-9_]")


def classify_code(text: str) -> tuple[list[bool], list[bool]]:
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


def strip_comments(text: str) -> str:
    """Remove every // and /* */ comment, preserving string/char literals.

    Comment chars (including any newlines inside a block comment) are dropped;
    line-comment newlines survive since the // scan stops at '\\n'. Used so the
    token counts reflect code only, not doc/explanatory prose."""
    _, is_comment = classify_code(text)
    return "".join(ch for ch, drop in zip(text, is_comment) if not drop)


def strip_cfg_test(text: str) -> tuple[str, int]:
    """Remove every in-code `#[cfg(test)]` item. Returns (stripped, lines_removed)."""
    in_code, _ = classify_code(text)
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


def count_lines(text: str) -> dict:
    in_code, _ = classify_code(text)
    lines = text.split("\n")
    # drop a single trailing empty element from a final newline
    if lines and lines[-1] == "":
        lines.pop()

    total = len(lines)
    blank = 0
    code = 0
    offset = 0
    for ln in lines:
        stripped = ln.strip()
        if not stripped:
            blank += 1
        else:
            # a "code line" has at least one non-whitespace char in code state
            has_code = any(
                in_code[offset + k] and not ch.isspace()
                for k, ch in enumerate(ln)
            )
            if has_code:
                code += 1
        offset += len(ln) + 1  # +1 for the newline we split on

    comment = total - blank - code
    return {"total": total, "blank": blank, "comment": comment, "code": code}


def count_tokens(text: str) -> dict:
    chars = len(text)
    return {
        "chars": chars,
        "words": len(WORD.findall(text)),
        "lex_tokens": len(LEX_TOKEN.findall(text)),
        "approx_llm": math.ceil(chars / 4),
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
    width = 36
    print("  File length (lines)   distribution")
    for (label, _, _), c in zip(BUCKETS, counts):
        bar = "█" * round(c / peak * width)
        print(f"  {label:>9} │ {bar} {c}")


def print_extremes(rows: list[tuple], k: int = 5) -> None:
    by_len = sorted(rows, key=lambda r: r[1])
    print(f"  Longest {k} files")
    for name, total, *_ in reversed(by_len[-k:]):
        print(f"    {total:>6,}  {name}")
    print(f"\n  Shortest {k} files")
    for name, total, *_ in by_len[:k]:
        print(f"    {total:>6,}  {name}")


def gather(root: str) -> list[str]:
    files = []
    for dirpath, dirnames, filenames in os.walk(root):
        if "target" in dirnames:
            dirnames.remove("target")
        for f in filenames:
            if f.endswith(".rs"):
                files.append(os.path.join(dirpath, f))
    return sorted(files)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("root", nargs="?", default="runtime/src",
                    help="directory to scan (default: runtime/src)")
    ap.add_argument("--per-file", action="store_true",
                    help="print a per-file breakdown table")
    args = ap.parse_args()

    if not os.path.isdir(args.root):
        print(f"error: not a directory: {args.root}", file=sys.stderr)
        return 2

    files = gather(args.root)
    if not files:
        print(f"error: no .rs files under {args.root}", file=sys.stderr)
        return 2

    agg = {"total": 0, "blank": 0, "comment": 0, "code": 0,
           "chars": 0, "words": 0, "lex_tokens": 0, "approx_llm": 0,
           "test_lines_stripped": 0, "comment_chars_stripped": 0}
    rows = []

    for path in files:
        with open(path, encoding="utf-8") as fh:
            text = fh.read()
        impl, test_lines = strip_cfg_test(text)
        # Line breakdown keeps comments (so the comment-line stat survives);
        # token counts run on the comment-stripped code only.
        code_only = strip_comments(impl)
        lc = count_lines(impl)
        tc = count_tokens(code_only)
        agg["comment_chars_stripped"] += len(impl) - len(code_only)
        agg["total"] += lc["total"]
        agg["blank"] += lc["blank"]
        agg["comment"] += lc["comment"]
        agg["code"] += lc["code"]
        agg["chars"] += tc["chars"]
        agg["words"] += tc["words"]
        agg["lex_tokens"] += tc["lex_tokens"]
        agg["approx_llm"] += tc["approx_llm"]
        agg["test_lines_stripped"] += test_lines
        rows.append((os.path.relpath(path, args.root), lc["total"], lc["code"],
                     tc["approx_llm"], test_lines))

    if args.per_file:
        rows.sort(key=lambda r: r[1], reverse=True)
        print(f"{'file':<44}{'lines':>8}{'code':>8}{'~tokens':>10}{'test-':>8}")
        print(f"{'':<44}{'':>8}{'':>8}{'':>10}{'strip':>8}")
        print("-" * 78)
        for name, total, code, tok, ts in rows:
            print(f"{name:<44}{total:>8}{code:>8}{tok:>10}{ts:>8}")
        print("-" * 78)

    print(f"\nRust implementation under {args.root}/  ({len(files)} files, "
          f"embedded #[cfg(test)] stripped)\n")
    print(f"  Lines (total)        {agg['total']:>10,}")
    print(f"    code               {agg['code']:>10,}")
    print(f"    comment            {agg['comment']:>10,}")
    print(f"    blank              {agg['blank']:>10,}")
    print()
    print(f"  Tokens (~LLM, chars/4) {agg['approx_llm']:>8,}   (comments stripped)")
    print(f"    lexical tokens     {agg['lex_tokens']:>10,}")
    print(f"    words              {agg['words']:>10,}")
    print(f"    chars              {agg['chars']:>10,}")
    print()
    print(f"  (test lines stripped from impl files: {agg['test_lines_stripped']:,})")
    print(f"  (comment chars stripped from token counts: {agg['comment_chars_stripped']:,})")
    print()
    print_histogram([r[1] for r in rows])
    print()
    print_extremes(rows)
    return 0


if __name__ == "__main__":
    sys.exit(main())
