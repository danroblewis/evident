#!/usr/bin/env python3
"""Phase A oracle: compare the Rust lexer against the self-hosted Evident lexer.

The Rust lexer in `runtime/src/lexer.rs` is the language's source of truth.
The Evident lexer in `stdlib/lexer.ev` is its self-hosted shadow — running
as a multi-tick FSM on the kernel and currently covering a small subset:

    - alphabetic identifiers (and the keywords: claim, type, schema, fsm,
      enum, import, match)
    - decimal integer literals
    - the single-char operators ( ) , + =
    - whitespace skipping (space, tab, newline)
    - line comments starting with `--`

The oracle drives an input through both lexers and asserts they produce the
same token sequence — modulo tokens the Evident lexer doesn't model yet
(Indent, Newline, EOL-Comment markers, and Eof itself, which we
canonicalize). When another agent extends the Evident lexer (Phase A1–A7),
the oracle will start agreeing on more inputs without needing changes here.

Pipeline:
   for each corpus input string:
      a) write it to /tmp/evident_consolidated_input.txt
      b) `evident dump-tokens <generated.ev>`           → rust token list
      c) `evident emit tests/kernel/test_consolidated_lexer.ev main`
         then `kernel <tmp.smt2>`                        → evident lexer
                                                          per-tick diagnostic
      d) parse the per-tick diagnostic into a token list
      e) project rust tokens → evident-comparable subset
      f) assert sequences match; report diff if not

Acceptance for "Phase A complete" (the milestone this script measures):
all inputs in the corpus produce identical token sequences.

Today: the corpus is curated to fit the Evident lexer's current vocabulary.
Failures are *expected* on inputs containing Unicode operators, strings,
floats, etc. — those are A1–A7 work-list items.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EVIDENT = ROOT / "runtime/target/release/evident"
KERNEL = ROOT / "kernel/target/release/kernel"
CONSOLIDATED_FIXTURE = ROOT / "tests/kernel/test_consolidated_lexer.ev"
CONSOLIDATED_INPUT_PATH = "/tmp/evident_consolidated_input.txt"


# ── Inputs ────────────────────────────────────────────────────────────
# Each input is a (name, source_string) pair. Source strings are what
# the Evident lexer can handle today — alphabetic idents, decimal ints,
# the single-char ops `( ) , + =`, whitespace, line comments.
#
# Stash inputs that we KNOW will diverge in `EXPECTED_FAILURES` so the
# oracle can demonstrate the work-list it'll close as Phase A1–A7 land.

CORPUS: list[tuple[str, str]] = [
    # ---- minimal: one token of each variety the Evident lexer covers
    ("kw_claim",        "claim\n"),
    ("kw_type",         "type\n"),
    ("kw_fsm",          "fsm\n"),
    ("kw_enum",         "enum\n"),
    ("kw_import",       "import\n"),
    ("kw_match",        "match\n"),
    ("kw_schema",       "schema\n"),
    ("ident_one",       "x\n"),
    ("ident_multi",     "hello\n"),
    ("int_one",         "1\n"),
    ("int_multi",       "12345\n"),
    ("op_lparen",       "(\n"),
    ("op_rparen",       ")\n"),
    ("op_comma",        ",\n"),
    ("op_plus",         "+\n"),
    ("op_eq",           "=\n"),

    # ---- combinations: keyword + ident + int + op
    ("claim_x_eq_1",    "claim x = 1\n"),
    ("type_y_eq_42",    "type y = 42\n"),
    ("paren_grouping",  "(x + y)\n"),
    ("multi_idents",    "abc def ghi\n"),
    ("multi_ints",      "1 2 3 4 5\n"),

    # ---- comment skipping
    ("comment_only",    "-- a comment\n"),
    ("comment_after",   "claim x -- trailing\nclaim y\n"),

    # ---- whitespace stress
    ("tabs_and_spaces", "claim\t \tx\n"),

    # ---- adjacent ops (`==` is two `Eq` in both lexers — Evident has no `==`)
    ("two_eq",          "a == b\n"),
]

# Inputs we expect to FAIL with today's Evident lexer. Each entry says
# why and which A-substep would close the gap. Listed so the script
# documents the punch list.
EXPECTED_FAILURES: dict[str, str] = {
    "unicode_in":         "A1 — Unicode operator ∈ not yet recognized",
    "unicode_implies":    "A1 — Unicode operator ⇒ not yet recognized",
    "string_literal":     "A3 — string literals not yet",
    "float_literal":      "A4 — float literals not yet",
    "subclaim_keyword":   "A5 — `subclaim` not in the Evident keyword table",
    "true_false":         "A5 — `true`/`false` not in the Evident keyword table",
    "minus_op":           "A2/A5 — minus not yet a recognized single-char op",
}
CORPUS_FAILURES: list[tuple[str, str]] = [
    ("unicode_in",       "x ∈ Int\n"),
    ("unicode_implies",  "a ⇒ b\n"),
    ("string_literal",   "x = \"hi\"\n"),
    ("float_literal",    "x = 3.14\n"),
    ("subclaim_keyword", "subclaim foo\n"),
    ("true_false",       "x = true\n"),
    ("minus_op",         "x - 1\n"),
]


# ── Rust lexer side ───────────────────────────────────────────────────
def rust_tokens(source: str) -> list[dict]:
    """Write source to a temp file, run `evident dump-tokens`, return JSON tokens."""
    with tempfile.NamedTemporaryFile(suffix=".ev", mode="w", delete=False) as f:
        f.write(source)
        path = f.name
    try:
        r = subprocess.run(
            [str(EVIDENT), "dump-tokens", path],
            capture_output=True, text=True, timeout=10,
        )
        if r.returncode != 0:
            raise RuntimeError(f"rust lexer failed: {r.stderr.strip()}")
        return json.loads(r.stdout)
    finally:
        os.unlink(path)


# ── Evident lexer side ────────────────────────────────────────────────
# We don't generate the FSM from scratch; we re-use the consolidated-lexer
# fixture, just swapping the input file's contents.
LABEL_RE = re.compile(
    r"^(?:"
    r"KW\((?P<kw>[a-z]+)\)|"
    r"ID\((?P<id>[a-zA-Z_]+)\)|"
    r"INT\((?P<int>-?\d+)\)|"
    r"OP\((?P<op>.)\)|"
    r"\[eof\]|"
    r"  \?|"          # collecting
    r"  -|"           # whitespace skip
    r"  # comment|"   # entering comment
    r"  \."           # inside comment
    r")"
    # The composite labels: ID+OP, INT+OP, KW+OP
    r"|^(?P<ck>KW\([a-z]+\)|ID\([a-zA-Z_]+\)|INT\(-?\d+\)) \+ OP\((?P<cop>.)\)$"
)


def evident_tokens(source: str) -> list[dict]:
    """Drive the consolidated-lexer fixture against the given source.

    Returns a token list in the same shape `rust_tokens()` produces but
    only including the kinds the Evident lexer can emit today.
    """
    Path(CONSOLIDATED_INPUT_PATH).write_text(source)

    with tempfile.NamedTemporaryFile(suffix=".smt2", mode="w", delete=False) as f:
        smt_path = f.name
    try:
        r = subprocess.run(
            [str(EVIDENT), "emit", str(CONSOLIDATED_FIXTURE), "main", "-o", smt_path],
            capture_output=True, text=True, timeout=30,
        )
        if r.returncode != 0:
            raise RuntimeError(f"emit failed: {r.stderr.strip() or r.stdout.strip()}")
        r = subprocess.run(
            [str(KERNEL), smt_path],
            capture_output=True, text=True, timeout=30,
        )
        if r.returncode != 0:
            raise RuntimeError(
                f"kernel failed (exit {r.returncode}): {r.stderr.strip()}"
            )
    finally:
        os.unlink(smt_path)

    return parse_evident_labels(r.stdout)


def parse_evident_labels(stdout: str) -> list[dict]:
    """Map per-tick labels into the same token-dict shape `rust_tokens` uses."""
    KW_MAP = {
        "claim": "Claim", "type": "Type", "schema": "Schema",
        "fsm": "Fsm", "enum": "Enum", "import": "Import",
        "match": "Match",
    }
    OP_MAP = {"(": "LParen", ")": "RParen", ",": "Comma", "+": "Plus", "=": "Eq"}

    out: list[dict] = []
    for raw in stdout.splitlines():
        line = raw.rstrip()
        if not line:
            continue
        # Composite tick: "KW(claim) + OP(+)" emits two tokens in order.
        comp = re.match(r"^(KW\([a-z]+\)|ID\([a-zA-Z_]+\)|INT\(-?\d+\)) \+ OP\((.)\)$", line)
        if comp:
            primary, op = comp.group(1), comp.group(2)
            out.extend(parse_evident_labels(primary))
            if op in OP_MAP:
                out.append({"k": OP_MAP[op]})
            else:
                out.append({"k": "Err", "v": op})
            continue
        if line.startswith("KW("):
            kw = line[3:-1]
            out.append({"k": KW_MAP.get(kw, "Ident"), "v": kw} if kw not in KW_MAP
                       else {"k": KW_MAP[kw]})
        elif line.startswith("ID("):
            out.append({"k": "Ident", "v": line[3:-1]})
        elif line.startswith("INT("):
            out.append({"k": "Int", "v": int(line[4:-1])})
        elif line.startswith("OP("):
            op = line[3:-1]
            out.append({"k": OP_MAP[op]} if op in OP_MAP else {"k": "Err", "v": op})
        elif line == "[eof]":
            out.append({"k": "Eof"})
        # else: ?, -, # comment, . — all in-progress / skip ticks; no token emitted.
    return out


# ── Projection: drop tokens the Evident lexer can't emit yet ─────────
EVIDENT_KNOWN_KINDS = {
    "Ident", "Int", "Claim", "Type", "Schema", "Fsm", "Enum",
    "Import", "Match",
    "LParen", "RParen", "Comma", "Plus", "Eq",
    "Eof",
}


def project_rust(tokens: list[dict]) -> list[dict]:
    """Strip tokens the Evident lexer doesn't yet model (Indent, Newline,
    and anything outside the known vocabulary). Tokens that ARE in the
    vocabulary but the Evident lexer would have failed on (e.g. Real)
    stay so the diff surfaces them as failures."""
    return [t for t in tokens if t["k"] not in {"Indent", "Newline"}]


def normalize(tokens: list[dict]) -> list[tuple]:
    """Hashable canonical form for comparison."""
    return [(t["k"], t.get("v")) for t in tokens]


@dataclass
class CaseResult:
    name: str
    source: str
    passed: bool
    rust: list[dict]
    evident: list[dict]
    error: str | None = None


def run_case(name: str, source: str) -> CaseResult:
    try:
        r_toks = project_rust(rust_tokens(source))
        e_toks = evident_tokens(source)
    except Exception as e:
        return CaseResult(name, source, False, [], [], error=str(e))
    return CaseResult(
        name=name,
        source=source,
        passed=(normalize(r_toks) == normalize(e_toks)),
        rust=r_toks,
        evident=e_toks,
    )


def format_token_seq(toks: list[dict]) -> str:
    return " ".join(
        f'{t["k"]}({t["v"]!r})' if "v" in t else t["k"]
        for t in toks
    )


def main() -> int:
    if not EVIDENT.exists():
        print(f"evident binary missing at {EVIDENT}", file=sys.stderr)
        print("build with: (cd runtime && cargo build --release)", file=sys.stderr)
        return 1
    if not KERNEL.exists():
        print(f"kernel binary missing at {KERNEL}", file=sys.stderr)
        print("build with: (cd kernel && cargo build --release)", file=sys.stderr)
        return 1

    print("== Phase A lexer oracle ==")
    print(f"corpus: {len(CORPUS)} expected-pass, "
          f"{len(CORPUS_FAILURES)} expected-fail")
    print()

    passed: list[CaseResult] = []
    failed: list[CaseResult] = []
    unexpected_passes: list[CaseResult] = []
    unexpected_failures: list[CaseResult] = []

    for name, source in CORPUS:
        res = run_case(name, source)
        if res.passed:
            print(f"  ok   {name}")
            passed.append(res)
        else:
            print(f"  FAIL {name}")
            failed.append(res)
            unexpected_failures.append(res)

    print()
    print("-- expected-failure corpus (documenting the A1-A7 work list) --")
    for name, source in CORPUS_FAILURES:
        res = run_case(name, source)
        gap = EXPECTED_FAILURES.get(name, "?")
        if res.passed:
            print(f"  !!   {name}: passed — gap closed?  ({gap})")
            unexpected_passes.append(res)
        else:
            print(f"  ~    {name}: failed (expected) — {gap}")

    print()
    print(f"summary: {len(passed)}/{len(CORPUS)} expected-pass succeeded")
    if unexpected_failures:
        print(f"  {len(unexpected_failures)} unexpected failure(s):")
        for r in unexpected_failures:
            print(f"  -- {r.name} ----")
            print(f"     input    : {r.source!r}")
            if r.error:
                print(f"     error    : {r.error}")
            else:
                print(f"     rust     : {format_token_seq(r.rust)}")
                print(f"     evident  : {format_token_seq(r.evident)}")
    if unexpected_passes:
        print(f"  {len(unexpected_passes)} unexpected pass(es): consider"
              " promoting these from the expected-failure corpus.")
        for r in unexpected_passes:
            print(f"     {r.name}: now passes — gap closed.")

    # Phase A complete = no unexpected failures AND no unexpected passes.
    # Today both can hold while EXPECTED_FAILURES is non-empty — that
    # means the oracle is reflecting the actual capability boundary.
    return 0 if not unexpected_failures and not unexpected_passes else 1


if __name__ == "__main__":
    sys.exit(main())
