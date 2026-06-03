#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# diff-vs-bootstrap.sh — equivalence check for ONE .ev source.
#
# Compiles a single source file two ways and diffs the resulting SMT-LIB:
#   * bootstrap  — `scripts/evident-self bin` (the bootstrap runtime) emit
#   * self-hosted — kernel + compiler.smt2
# A clean diff means the self-hosted compiler reproduces the bootstrap
# output for that source — the per-source proof that backs deleting
# bootstrap (CLAUDE.md, "The deletion path", step 4/5).
#
# Self-hosted input mechanism: compiler/compiler.ev reads its source from
# the fixed path /tmp/compiler-input.ev via a ReadFile effect on the first
# tick (see compiler/compiler.ev, "Input path"), NOT from stdin. So we
# write the flattened source there before invoking the kernel. (stdin is
# also redirected for forward-compatibility with a future stdin-reading
# driver; the current driver ignores it.)
#
# Usage:
#   scripts/diff-vs-bootstrap.sh <source.ev> <claim>
#
# Exit codes:
#   0  identical (whitespace-equivalent) OR cleanly SKIPPED
#   1  outputs differ, or a usage / compile error occurred
#
# SKIPPED (exit 0) when compiler.smt2 or the kernel binary is absent — so
# wiring this into a test harness before the cutover never turns it red.

set -u -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

# Bootstrap reference compiler via the single seam (never the hardcoded path —
# keeps scripts/check-deletable.sh's blocker-1 clean). Force the bootstrap
# binary (EVIDENT_SELF_VIA_SMT2=0): this leg IS the bootstrap reference we diff
# the self-hosted output against.
EVIDENT="$(EVIDENT_SELF_VIA_SMT2=0 "$ROOT/scripts/evident-self" bin)"
KERNEL="$ROOT/kernel/target/release/kernel"
FLATTEN="$ROOT/scripts/flatten-evident.sh"
COMPILER_SMT2="$ROOT/compiler.smt2"
INPUT_PATH="/tmp/compiler-input.ev"   # the path compiler.ev ReadFile's

die() { echo "diff-vs-bootstrap: $*" >&2; exit 1; }
skip() { echo "diff-vs-bootstrap: SKIPPED — $*"; exit 0; }

[ "$#" -eq 2 ] || die "usage: diff-vs-bootstrap.sh <source.ev> <claim>"
SRC="$1"; CLAIM="$2"
[ -f "$SRC" ] || die "source not found: $SRC"

# Pre-cutover guards: missing self-hosted compiler / kernel ⇒ SKIP, not fail.
[ -f "$COMPILER_SMT2" ] || skip "compiler.smt2 not built yet (run scripts/build-compiler-smt2.sh)"
[ -x "$KERNEL" ]        || skip "kernel binary not built (run ./test.sh --rust-only)"
[ -x "$EVIDENT" ]       || die  "bootstrap runtime missing at ${EVIDENT#$ROOT/}"
[ -x "$FLATTEN" ]       || die  "flatten preprocessor missing at ${FLATTEN#$ROOT/}"

ORIG="$(mktemp -t orig.XXXXXX.smt2)"
FLAT="$(mktemp -t flat.XXXXXX.ev)"
SELF="$(mktemp -t self.XXXXXX.smt2)"
trap 'rm -f "$ORIG" "$FLAT" "$SELF"' EXIT

# 1. bootstrap reference output.
if ! "$EVIDENT" emit "$SRC" "$CLAIM" -o "$ORIG" 2>"$FLAT.err"; then
    die "bootstrap emit failed: $(head -1 "$FLAT.err" 2>/dev/null)"
fi

# 2. flatten the source into one translation unit (kernel does no imports).
if ! "$FLATTEN" "$SRC" > "$FLAT"; then
    die "flatten failed for $SRC"
fi

# 3. self-hosted output: feed the flattened source to compiler.smt2.
cp "$FLAT" "$INPUT_PATH"
if ! "$KERNEL" "$COMPILER_SMT2" < "$FLAT" > "$SELF" 2>"$FLAT.kerr"; then
    die "self-hosted compile failed: $(head -1 "$FLAT.kerr" 2>/dev/null)"
fi

# 4. diff, treating whitespace-only differences as equivalent.
if diff -q -w -B "$ORIG" "$SELF" >/dev/null 2>&1; then
    echo "diff-vs-bootstrap: MATCH — $SRC ($CLAIM): self-hosted ≡ bootstrap"
    exit 0
else
    echo "diff-vs-bootstrap: DIFFER — $SRC ($CLAIM)" >&2
    diff -w -B "$ORIG" "$SELF" | head -40 >&2
    exit 1
fi
