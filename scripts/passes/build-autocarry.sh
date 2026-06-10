#!/usr/bin/env bash
# build-autocarry.sh — rebuild the autocarry pass artifacts (.smt2) from
# their Evident sources after editing scripts/passes/autocarry_*.ev.
#
# Bootstrap note: the pass programs are themselves fsm sources, so this
# build flattens them through the EXISTING awk pipeline
# (scripts/flatten-evident.sh, which runs expand-fsm-autocarry.sh) and
# compiles with the frozen oracle. The awk pass stays in the repo as the
# reference implementation and this rebuild bootstrap.
#
# Usage: scripts/passes/build-autocarry.sh
# Env:   EVIDENT_ORACLE (default /usr/local/bin/evident-oracle)

set -e -u -o pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../.." && pwd)"
ORACLE="${EVIDENT_ORACLE:-/usr/local/bin/evident-oracle}"

for prog in autocarry_analyze autocarry_fix autocarry_apply; do
    flat="$(mktemp)"
    "$ROOT/scripts/flatten-evident.sh" "$DIR/$prog.ev" > "$flat"
    "$ORACLE" emit "$flat" "$prog" -o "$DIR/$prog.smt2"
    rm -f "$flat"
    echo "built $DIR/$prog.smt2"
done
