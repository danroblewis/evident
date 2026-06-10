#!/usr/bin/env bash
# scripts/build-oracle.sh — build the FROZEN bootstrap oracle binary.
#
# Operator decision (2026-06-07, amended 2026-06-10): the deleted
# bootstrap compiler may be used as a build-time ORACLE. Its source
# lives on the dedicated `oracle` branch (NEVER merged into main; the
# working tree of main never contains it). PIN below is an exact SHA
# on that branch, so builds are reproducible.
#
#   - ORACLE-CHANGE RULES (the freeze, restated for the branch era):
#     1. Bugfix-to-spec ONLY, features NEVER. An oracle commit is
#        legal iff it makes the oracle agree with semantics pinned by
#        conformance fixtures, and its message names those fixtures.
#        Capabilities go in compiler2/, which must implement the same
#        semantics regardless — the oracle is scaffolding.
#     2. The `oracle` branch never merges anywhere. Fix → commit on
#        the branch → update PIN here → rebuild.
#   - Pin lineage: c218dca^ (original bootstrap + c817c6c expr_as_var
#     fix) → c95710c (record-carry fix: pinned-constant manifest
#     exclusion + dotted record-field carry; branch point of `oracle`).
#   - Install: /usr/local/bin/evident-oracle
#   - SUNSET: delete this script, the binary, and the `oracle` branch
#     the day compiler2 compiles itself.
#
# Idempotent; ~7 s build. Requires the repo's git history + cargo.

set -euo pipefail

# PIN: exact SHA on the `oracle` branch (see header rules).
PIN="c95710c"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILD=/opt/bootstrap-oracle
BIN=/usr/local/bin/evident-oracle
STAMP="$BUILD/.pin"

if [ -x "$BIN" ] && [ -f "$STAMP" ] && [ "$(cat "$STAMP")" = "$PIN" ]; then
    echo "oracle already installed at pin $PIN: $BIN"
    exit 0
fi

mkdir -p "$BUILD"
rm -rf "$BUILD/bootstrap"
git -C "$ROOT" archive "$PIN" bootstrap | tar -x -C "$BUILD"
(cd "$BUILD/bootstrap/runtime" && cargo build --release)
cp "$BUILD/bootstrap/runtime/target/release/evident" "$BIN"
echo "$PIN" > "$STAMP"
echo "oracle installed: $BIN (source pinned at $PIN, build dir $BUILD)"
