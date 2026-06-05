#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# build-sample-smt2-candidate.sh — produce a CANDIDATE sample.smt2
# for testing without touching the committed production artifact.
#
# Pattern: while iterating on compiler/sample.ev (or its imports),
# we DON'T want to overwrite the committed sample.smt2 — the
# rebuild path uses the current sample.smt2 itself, so a broken
# in-flight candidate would brick the dev loop. Instead build to
# a side-car path, test against it, and promote with `mv` only
# when it's green.
#
# Usage:
#   scripts/build-sample-smt2-candidate.sh                  # → sample_new.smt2
#   scripts/build-sample-smt2-candidate.sh path/to/out.smt2 # → custom path
#
# Then run a lang test against the candidate by setting
# EVIDENT_SAMPLE_SMT2 (the seam wrapper honors it) to the candidate
# path before invoking the test runner.

set -u -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

KERNEL="$ROOT/kernel/target/release/kernel"
COMPILER_SMT2="$ROOT/compiler.smt2"
FLATTEN="$ROOT/scripts/flatten-evident.sh"
SRC="$ROOT/compiler/sample.ev"
OUT="${1:-$ROOT/sample_new.smt2}"

die() { echo "build-sample-smt2-candidate: $*" >&2; exit 1; }

[ -x "$KERNEL" ]        || die "kernel binary missing at $KERNEL"
[ -f "$COMPILER_SMT2" ] || die "compiler.smt2 missing at $COMPILER_SMT2"
[ -x "$FLATTEN" ]       || die "flatten missing at $FLATTEN"
[ -f "$SRC" ]           || die "sample source missing at $SRC"

FLAT="$(mktemp -t sample-flat.XXXXXX.ev)"
trap 'rm -f "$FLAT"' EXIT

if ! "$FLATTEN" "$SRC" > "$FLAT"; then
    die "flatten failed for $SRC"
fi

started=$(date +%s)
echo "build-sample-smt2-candidate: flattened $(wc -l < "$FLAT") lines; compiling via kernel + compiler.smt2 → $OUT (this is the slow step)" >&2

STAGE="$OUT.tmp"
if ! printf '%s\nmain\n' "$FLAT" | "$KERNEL" "$COMPILER_SMT2" > "$STAGE"; then
    rc=$?
    rm -f "$STAGE"
    die "kernel + compiler.smt2 emit failed (exit $rc)"
fi

if [ ! -s "$STAGE" ] || ! head -1 "$STAGE" | grep -q '^;; manifest:'; then
    head -3 "$STAGE" >&2 2>/dev/null
    rm -f "$STAGE"
    die "candidate is empty or missing manifest header (compiler.smt2 didn't produce a valid program)"
fi

mv "$STAGE" "$OUT"
elapsed=$(( $(date +%s) - started ))
echo "build-sample-smt2-candidate: OK ($(wc -l < "$OUT") lines, ${elapsed}s) → $OUT" >&2
