#!/usr/bin/env bash
# TODO: rewrite in Evident
# build-lowerseq.sh — rebuild the bounded-Seq lowering pass artifacts
# (.smt2) from their Evident sources after editing lowerseq_*.ev.
#
# Bootstrap note (mirrors build-autocarry.sh): the pass programs are fsm
# sources, so this build flattens them through the awk reference autocarry
# pass (EVIDENT_AUTOCARRY=awk) and compiles with the frozen oracle —
# rebuilding never depends on the artifacts being rebuilt. lower-bounded-seq.sh
# stays as the reference implementation and this rebuild bootstrap; the wired
# pipeline pass is the Evident port (lowerseq-evident.sh, once complete).
#
# Usage: scripts/passes/build-lowerseq.sh
# Env:   EVIDENT_ORACLE (default /usr/local/bin/evident-oracle)

set -e -u -o pipefail
export EVIDENT_AUTOCARRY=awk

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../.." && pwd)"
ORACLE="${EVIDENT_ORACLE:-/usr/local/bin/evident-oracle}"

for prog in lowerseq_scan lowerseq_plan lowerseq_emit; do
    [ -f "$DIR/$prog.ev" ] || { echo "skip $prog (no source yet)"; continue; }
    flat="$(mktemp)"
    "$ROOT/scripts/flatten-evident.sh" "$DIR/$prog.ev" > "$flat"
    "$ORACLE" emit "$flat" "$prog" -o "$DIR/$prog.smt2"
    rm -f "$flat"
    echo "built $DIR/$prog.smt2"
done
