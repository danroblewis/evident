#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# tests/compiler2_units/run.sh — isolation-test driver for the
# driver_main decomposition modules (§5 of the execution plan).
#
# Each module extracted from compiler2/driver.ev ships ≥1 isolation
# fixture under tests/compiler2_units/<module>/*.ev. A fixture is a
# self-contained kernel-runnable program that:
#   - imports the module under test (and only what it needs),
#   - constructs the module's inputs as concrete values,
#   - drives the module via `..Module` names-match lift (or a slot call),
#   - emits an effect (puts/Exit) that encodes the module's output,
#   - declares its expectation in the header.
#
# A fixture that goes green ONLY when the module's contract holds is the
# proof the module is correct in isolation — conformance never tells you
# WHICH module broke; these do.
#
# Header (same dialect as tests/fsm_compose/run.sh):
#   -- entry:  <ClaimName>
#   -- expect: stdout = "<line>"   (repeat, in order)
#   -- expect: exit = <N>
#
# Pipeline mirrors the producing path exactly:
#   flatten-evident.sh | evident-oracle emit <entry> | kernel <out.smt2>
#
# Env: EVIDENT_ORACLE (default /usr/local/bin/evident-oracle)
#      EVIDENT_KERNEL (default <repo>/kernel/target/release/kernel)
#
# Usage: tests/compiler2_units/run.sh [module ...]   (default: all modules)
# Exit 0 iff every fixture passes.

set -u -o pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../.." && pwd)"
ORACLE="${EVIDENT_ORACLE:-/usr/local/bin/evident-oracle}"
KERNEL="${EVIDENT_KERNEL:-$ROOT/kernel/target/release/kernel}"
FLATTEN="$ROOT/scripts/flatten-evident.sh"

[ -x "$ORACLE" ] || { echo "run.sh: oracle not executable: $ORACLE" >&2; exit 2; }
[ -x "$KERNEL" ] || { echo "run.sh: kernel not executable: $KERNEL" >&2; exit 2; }

if [ "$#" -gt 0 ]; then
    FIXTURES=()
    for m in "$@"; do FIXTURES+=("$DIR/$m"/*.ev); done
else
    FIXTURES=("$DIR"/*/*.ev)
fi

pass=0; fail=0
for fx in "${FIXTURES[@]}"; do
    [ -f "$fx" ] || continue
    name="$(basename "$(dirname "$fx")")/$(basename "$fx")"
    entry="$(sed -n 's/^-- entry:[[:space:]]*//p' "$fx" | head -1)"
    # A file with no `-- entry:` header is a module/support file imported
    # by a sibling fixture, not a fixture itself — skip it silently.
    [ -n "$entry" ] || continue

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
    "$KERNEL" "$smt" 2>/dev/null | grep -v '^\[functionizer\]' > "$out"
    act_exit=${PIPESTATUS[0]}
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
echo "compiler2_units: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
