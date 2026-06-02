#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# test_flatten_compiler.sh — proves scripts/flatten-evident.sh on the most
# complex real import graph in the project, compiler/compiler.ev.
#
# This is a standalone bash test (NOT an `.ev` kernel fixture and NOT a
# conformance feature dir), so neither tests/kernel/run-kernel-tests.sh (globs
# test_*.ev) nor the conformance runner (expects source.ev dirs) auto-runs it
# — keeping ./test.sh untouched. Run it directly:
#
#     tests/kernel/test_flatten_compiler.sh
#
# Checks:
#   1. flatten exits 0 and emits content from every imported file.
#   2. the output has no live `import` lines.
#   3. each imported file's content appears exactly once (diamond dedup).
#   4. dependency order: lexer before parser (parser imports lexer), and
#      every dep of parse_body before parse_body.
#   5. cycle detection exits non-zero.
#   6. smoke test: bootstrap-emitting the flattened source produces SMT-LIB
#      byte-identical to bootstrap-emitting the original compiler.ev.

set -u -o pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
FLATTEN="scripts/flatten-evident.sh"
SRC="compiler/compiler.ev"

pass=0; fail=0
ok()   { echo "  ✓ $1"; pass=$((pass+1)); }
bad()  { echo "  ✗ $1" >&2; fail=$((fail+1)); }

FLAT="$(mktemp -t flat.XXXXXX.ev)"
trap 'rm -f "$FLAT" "$ORIG_SMT" "$FLAT_SMT"' EXIT

# --- run flatten ------------------------------------------------------------
if "$FLATTEN" "$SRC" > "$FLAT" 2>/tmp/flatten_test.err; then
    ok "flatten exits 0"
else
    bad "flatten exit $? — $(cat /tmp/flatten_test.err)"
fi

# --- (2) no live import lines ----------------------------------------------
n_imports="$(grep -cE '^[[:space:]]*import[[:space:]]+"' "$FLAT" || true)"
[ "$n_imports" -eq 0 ] && ok "no live import lines" || bad "$n_imports live import lines remain"

# --- (1) content from every imported file ----------------------------------
# Each transitively-imported file leaves a `-- ===== <path> =====` marker.
for f in stdlib/kernel.ev compiler/lexer.ev compiler/parser.ev \
         compiler/translate_declare.ev compiler/translate_arith.ev \
         compiler/parse_body.ev compiler/compiler.ev; do
    if grep -qF -- "-- ===== $f =====" "$FLAT"; then ok "inlined $f"; else bad "missing $f"; fi
done

# --- (3) each file inlined exactly once ------------------------------------
dups="$(grep -E '^-- ===== ' "$FLAT" | sort | uniq -d)"
[ -z "$dups" ] && ok "every file inlined exactly once" || bad "duplicated: $dups"

# --- (4) dependency order ---------------------------------------------------
line_of() { grep -nF -- "-- ===== $1 =====" "$FLAT" | head -1 | cut -d: -f1; }
lex=$(line_of compiler/lexer.ev); par=$(line_of compiler/parser.ev)
pb=$(line_of compiler/parse_body.ev); td=$(line_of compiler/translate_declare.ev)
ta=$(line_of compiler/translate_arith.ev)
[ "$lex" -lt "$par" ] && ok "lexer before parser" || bad "lexer ($lex) not before parser ($par)"
if [ "$lex" -lt "$pb" ] && [ "$par" -lt "$pb" ] && [ "$td" -lt "$pb" ] && [ "$ta" -lt "$pb" ]; then
    ok "all deps before parse_body"
else
    bad "parse_body ($pb) not after all its deps (lex=$lex par=$par td=$td ta=$ta)"
fi

# --- (5) cycle detection ----------------------------------------------------
CA="$(mktemp -t cyc.XXXXXX.ev)"; CB="$(mktemp -t cyc.XXXXXX.ev)"
printf 'import "%s"\nclaim ca\n    x ∈ Int = 1\n' "$(basename "$CB")" > "$CA"
printf 'import "%s"\nclaim cb\n    y ∈ Int = 2\n' "$(basename "$CA")" > "$CB"
if "$FLATTEN" "$CA" >/dev/null 2>&1; then bad "cycle not detected"; else ok "cycle detected (exit nonzero)"; fi
rm -f "$CA" "$CB"

# --- (6) smoke test: emit equivalence --------------------------------------
EV="$(scripts/evident-self bin)"
ORIG_SMT="$(mktemp -t orig.XXXXXX.smt2)"; FLAT_SMT="$(mktemp -t flat.XXXXXX.smt2)"
if "$EV" emit "$SRC" main -o "$ORIG_SMT" 2>/dev/null \
   && "$EV" emit "$FLAT" main -o "$FLAT_SMT" 2>/dev/null; then
    if diff -q "$ORIG_SMT" "$FLAT_SMT" >/dev/null; then
        ok "flattened source emits byte-identical SMT-LIB to original"
    else
        bad "SMT-LIB differs: $(diff "$ORIG_SMT" "$FLAT_SMT" | head -5)"
    fi
else
    bad "emit failed (bootstrap binary built?)"
fi

echo "flatten test: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
