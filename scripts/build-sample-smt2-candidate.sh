#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# build-sample-smt2-candidate.sh — produce a CANDIDATE sample.smt2
# for testing without touching the committed production artifact.
#
# Pattern: while iterating on compiler/sample.ev (or its imports),
# we DON'T want to overwrite the committed sample.smt2 — replacing
# the production artifact mid-iteration would brick the dev loop
# if the candidate is broken. Instead build to a side-car path,
# test against it, and promote with `mv` only when it's green.
#
# Usage:
#   scripts/build-sample-smt2-candidate.sh                  # → sample_new.smt2
#   scripts/build-sample-smt2-candidate.sh path/to/out.smt2 # → custom path
#
# Compiler path: bootstrap by default (fast, seconds). Set
# EVIDENT_SELF_VIA_SMT2=1 to force the kernel + compiler.smt2 seam
# (slow, 10+ minutes, may OOM for large inputs — kept as the
# capability check for self-host eventually).
#
# Once compiled, run lang tests against the candidate by setting
# EVIDENT_SAMPLE_SMT2_OVERRIDE to the candidate path before invoking
# the test runner.

set -u -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

KERNEL="$ROOT/kernel/target/release/kernel"
COMPILER_SMT2="$ROOT/compiler.smt2"
BOOTSTRAP_EVIDENT="$ROOT/bootstrap/runtime/target/release/evident"
FLATTEN="$ROOT/scripts/flatten-evident.sh"
SRC="$ROOT/compiler/sample.ev"
OUT="${1:-$ROOT/sample_new.smt2}"

die() { echo "build-sample-smt2-candidate: $*" >&2; exit 1; }

[ -x "$FLATTEN" ] || die "flatten missing at $FLATTEN"
[ -f "$SRC" ]     || die "sample source missing at $SRC"

FLAT="$(mktemp -t sample-flat.XXXXXX.ev)"
trap 'rm -f "$FLAT"' EXIT

if ! "$FLATTEN" "$SRC" > "$FLAT"; then
    die "flatten failed for $SRC"
fi

started=$(date +%s)
STAGE="$OUT.tmp"

if [ "${EVIDENT_SELF_VIA_SMT2:-0}" = "1" ]; then
    [ -x "$KERNEL" ]        || die "kernel binary missing at $KERNEL"
    [ -f "$COMPILER_SMT2" ] || die "compiler.smt2 missing at $COMPILER_SMT2"
    echo "build-sample-smt2-candidate: flattened $(wc -l < "$FLAT") lines; seam path (kernel + compiler.smt2) → $OUT (slow)" >&2
    if ! printf '%s\nmain\n' "$FLAT" | "$KERNEL" "$COMPILER_SMT2" > "$STAGE"; then
        rc=$?
        rm -f "$STAGE"
        die "kernel + compiler.smt2 emit failed (exit $rc)"
    fi
else
    [ -x "$BOOTSTRAP_EVIDENT" ] || die "bootstrap binary missing at $BOOTSTRAP_EVIDENT (build with cd bootstrap/runtime && cargo build --release)"
    echo "build-sample-smt2-candidate: flattened $(wc -l < "$FLAT") lines; bootstrap path → $OUT" >&2
    if ! "$BOOTSTRAP_EVIDENT" emit "$FLAT" main -o "$STAGE" 2>/dev/null; then
        rc=$?
        rm -f "$STAGE"
        die "bootstrap emit failed (exit $rc)"
    fi
fi

if [ ! -s "$STAGE" ] || ! head -1 "$STAGE" | grep -q '^;; manifest:'; then
    head -3 "$STAGE" >&2 2>/dev/null
    rm -f "$STAGE"
    die "candidate is empty or missing manifest header"
fi

mv "$STAGE" "$OUT"
elapsed=$(( $(date +%s) - started ))
echo "build-sample-smt2-candidate: OK ($(wc -l < "$OUT") lines, ${elapsed}s) → $OUT" >&2
