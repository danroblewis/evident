#!/usr/bin/env bash
# scripts/build-oracle.sh — build the FROZEN bootstrap oracle binary.
#
# Operator decision (2026-06-07): the deleted bootstrap compiler may be
# used as a build-time ORACLE — restored from git history OUTSIDE the
# working tree, built once, kept only as a binary. The source never
# re-enters the repo, so there is nothing to be tempted to extend.
#
#   - Pinned source: c218dca^ (the re-deletion's parent — includes the
#     c817c6c expr_as_var fix and the 22-predicate Z3 coverage).
#   - Install: /usr/local/bin/evident-oracle
#   - SUNSET: delete this script and the binary the day compiler2
#     compiles itself. The oracle is scaffolding, not a dependency.
#
# Idempotent; ~7 s build. Requires the repo's git history + cargo.

set -euo pipefail

PIN="c218dca^"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILD=/opt/bootstrap-oracle
BIN=/usr/local/bin/evident-oracle

if [ -x "$BIN" ]; then
    echo "oracle already installed: $BIN"
    exit 0
fi

mkdir -p "$BUILD"
git -C "$ROOT" archive "$PIN" bootstrap | tar -x -C "$BUILD"
(cd "$BUILD/bootstrap/runtime" && cargo build --release)
cp "$BUILD/bootstrap/runtime/target/release/evident" "$BIN"
echo "oracle installed: $BIN (source pinned at $PIN, build dir $BUILD)"
