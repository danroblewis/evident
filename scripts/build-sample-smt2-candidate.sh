#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# build-sample-smt2-candidate.sh — produce a CANDIDATE sample.smt2
# for testing without touching the committed production artifact.
#
# Pattern: while iterating on compiler/sample.ev (or its imports),
# we build to a side-car path, test against it, and promote with
# `mv sample_new.smt2 sample.smt2` only when it's green. Git is the
# safety net — `git checkout sample.smt2` reverts a bad promotion.
#
# Compile path: kernel + the current committed compiler.smt2 (the
# seam). Slow (10+ minutes for compiler/sample.ev), may OOM for
# very large inputs; honest about its current capability. When the
# seam can't compile a particular shape, that's a capability gap
# in compiler.smt2 to track and fix at the source level — not
# something to route around by restoring bootstrap.
#
# Usage:
#   scripts/build-sample-smt2-candidate.sh                  # → sample_new.smt2
#   scripts/build-sample-smt2-candidate.sh path/to/out.smt2 # → custom path

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

[ -x "$KERNEL" ]        || die "kernel binary missing at $KERNEL (run: cd kernel && cargo build --release)"
[ -f "$COMPILER_SMT2" ] || die "compiler.smt2 missing at $COMPILER_SMT2"
[ -x "$FLATTEN" ]       || die "flatten missing at $FLATTEN"
[ -f "$SRC" ]           || die "sample source missing at $SRC"

FLAT="$(mktemp -t sample-flat.XXXXXX.ev)"
trap 'rm -f "$FLAT"' EXIT

if ! "$FLATTEN" "$SRC" > "$FLAT"; then
    die "flatten failed for $SRC"
fi

started=$(date +%s)
STAGE="$OUT.tmp"

echo "build-sample-smt2-candidate: flattened $(wc -l < "$FLAT") lines; compiling via kernel + compiler.smt2 → $OUT" >&2
if ! printf '%s\nmain\n' "$FLAT" | "$KERNEL" "$COMPILER_SMT2" > "$STAGE"; then
    rc=$?
    rm -f "$STAGE"
    die "kernel + compiler.smt2 emit failed (exit $rc) — likely a capability gap in compiler.smt2"
fi

if [ ! -s "$STAGE" ] || ! head -1 "$STAGE" | grep -q '^;; manifest:'; then
    head -3 "$STAGE" >&2 2>/dev/null
    rm -f "$STAGE"
    die "candidate is empty or missing manifest header"
fi

mv "$STAGE" "$OUT"
elapsed=$(( $(date +%s) - started ))
echo "build-sample-smt2-candidate: OK ($(wc -l < "$OUT") lines, ${elapsed}s) → $OUT" >&2
