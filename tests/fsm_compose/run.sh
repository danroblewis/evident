#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# run.sh — driver for the carry-preserving fsm-composition fixtures.
#
# Each fixture in this directory is a kernel-runnable .ev whose header
# declares:
#   -- entry:  <ClaimName>        the top claim to emit
#   -- expect: stdout = "<line>"  one per expected stdout line, in order
#   -- expect: exit = <N>         the expected process exit code
#
# The pipeline mirrors the producing path exactly:
#   flatten-evident.sh (resolves imports + runs expand-fsm-autocarry.sh)
#     | evident-oracle emit <flat> <entry>
#     | kernel <out.smt2>
# It then diffs the kernel's stdout + exit against the header.
#
# Composition is emitted by the FROZEN oracle (the same compiler that
# builds compiler2/driver.ev), so these tests prove the SOURCE TRANSFORM
# is correct against the real oracle, not a mock.
#
# Env overrides:
#   EVIDENT_ORACLE  (default /usr/local/bin/evident-oracle)
#   EVIDENT_KERNEL  (default <repo>/kernel/target/release/kernel)
#
# Usage: tests/fsm_compose/run.sh [fixture.ev ...]   (default: all)
# Exit 0 iff every fixture passes.

set -u -o pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../.." && pwd)"
ORACLE="${EVIDENT_ORACLE:-/usr/local/bin/evident-oracle}"
KERNEL="${EVIDENT_KERNEL:-$ROOT/kernel/target/release/kernel}"
FLATTEN="$ROOT/scripts/flatten-evident.sh"

[ -x "$ORACLE" ] || { echo "run.sh: oracle not executable: $ORACLE" >&2; exit 2; }
[ -x "$KERNEL" ] || { echo "run.sh: kernel not executable: $KERNEL (build it or set EVIDENT_KERNEL)" >&2; exit 2; }

if [ "$#" -gt 0 ]; then FIXTURES=("$@"); else FIXTURES=("$DIR"/*.ev); fi

pass=0; fail=0
for fx in "${FIXTURES[@]}"; do
    name="$(basename "$fx")"
    entry="$(sed -n 's/^-- entry:[[:space:]]*//p' "$fx" | head -1)"
    [ -n "$entry" ] || { echo "FAIL $name — no '-- entry:' header"; fail=$((fail+1)); continue; }

    # Expected stdout (ordered) and exit from the header.
    exp_out="$(sed -n 's/^-- expect: stdout = "\(.*\)"$/\1/p' "$fx")"
    exp_exit="$(sed -n 's/^-- expect: exit = \([0-9]*\)$/\1/p' "$fx" | head -1)"
    [ -n "$exp_exit" ] || exp_exit=0

    flat="$(mktemp)"; smt="$(mktemp)"; out="$(mktemp)"
    if ! "$FLATTEN" "$fx" > "$flat" 2>/dev/null; then
        echo "FAIL $name — flatten failed"; fail=$((fail+1)); rm -f "$flat" "$smt" "$out"; continue
    fi
    if ! "$ORACLE" emit "$flat" "$entry" -o "$smt" 2>/dev/null; then
        echo "FAIL $name — oracle emit failed"; fail=$((fail+1)); rm -f "$flat" "$smt" "$out"; continue
    fi
    "$KERNEL" "$smt" > "$out" 2>/dev/null
    act_exit=$?
    act_out="$(cat "$out")"
    rm -f "$flat" "$smt" "$out"

    if [ "$act_out" = "$exp_out" ] && [ "$act_exit" = "$exp_exit" ]; then
        echo "PASS $name — stdout=[$(echo "$act_out" | tr '\n' '|')] exit=$act_exit"
        pass=$((pass+1))
    else
        echo "FAIL $name"
        echo "  expected exit=$exp_exit stdout:"; echo "$exp_out" | sed 's/^/    /'
        echo "  actual   exit=$act_exit stdout:"; echo "$act_out" | sed 's/^/    /'
        fail=$((fail+1))
    fi
done

echo "---"
echo "fsm_compose: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
