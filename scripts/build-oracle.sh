#!/usr/bin/env bash
# scripts/build-oracle.sh — build the FROZEN bootstrap oracle binary.
#
# Operator decision (2026-06-07): the deleted bootstrap compiler may be
# used as a build-time ORACLE — restored from git history OUTSIDE the
# working tree, built once, kept only as a binary. The source never
# re-enters the repo, so there is nothing to be tempted to extend.
#
#   - Pinned source: c95710c (the record-carry fix — recovered c218dca^
#     bootstrap plus the two translate/emit.rs fixes for record-typed
#     state fields: pinned-constant manifest exclusion and dotted
#     record-field carry. Supersedes c218dca^, which had the
#     c817c6c expr_as_var fix and the 22-predicate Z3 coverage but
#     mis-translated records built from constants and never carried
#     record fields). The fixed bootstrap source lives in that commit
#     only; it is removed from the working tree in the next commit.
#   - Install: /usr/local/bin/evident-oracle
#   - SUNSET: delete this script and the binary the day compiler2
#     compiles itself. The oracle is scaffolding, not a dependency.
#
# Idempotent; ~7 s build. Requires the repo's git history + cargo.

set -euo pipefail

# Pin the record-carry-fixed bootstrap (see header). The source is
# present in this commit's history even though it is absent from the
# working tree; `git archive` reads it from the object store.
PIN="c95710c"
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
