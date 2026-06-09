#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# tests/seq/run.sh — regression driver for the BOUNDED-Seq construction
# catalog (see docs/seq-bounded-catalog.md). Each fixture is a self-
# contained kernel-runnable program on a bounded `Seq` that exercises one
# construction and either:
#   * SAT   → the entry claim solves, emits Exit(0), process exits 0; or
#   * UNSAT → the kernel's tick solve fails, process exits 2.
#
# A fixture declares its expectation in the header (same dialect as
# tests/compiler2_units/run.sh):
#   -- entry:  <ClaimName>
#   -- expect: exit = <N>
#   -- expect: stdout = "<line>"   (optional, repeatable, in order)
#
# Pipeline mirrors the producing path exactly:
#   flatten-evident.sh | evident-oracle emit <entry> | kernel <out.smt2>
#
# Env: EVIDENT_ORACLE (default /usr/local/bin/evident-oracle)
#      EVIDENT_KERNEL (default the MAIN checkout's release kernel — this
#                      worktree carries no kernel/target).
#
# Usage: tests/seq/run.sh [fixture.ev ...]   (default: all fixtures here)
# Exit 0 iff every fixture matches its expected exit (and stdout).

set -u -o pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../.." && pwd)"
ORACLE="${EVIDENT_ORACLE:-/usr/local/bin/evident-oracle}"
KERNEL="${EVIDENT_KERNEL:-/Users/daniellewis/evident/kernel/target/release/kernel}"
FLATTEN="$ROOT/scripts/flatten-evident.sh"

[ -x "$ORACLE" ] || { echo "run.sh: oracle not executable: $ORACLE" >&2; exit 2; }
[ -x "$KERNEL" ] || { echo "run.sh: kernel not executable: $KERNEL" >&2; exit 2; }

if [ "$#" -gt 0 ]; then FIXTURES=("$@"); else FIXTURES=("$DIR"/*.ev); fi

pass=0; fail=0
for fx in "${FIXTURES[@]}"; do
    [ -f "$fx" ] || continue
    name="$(basename "$fx")"
    entry="$(sed -n 's/^-- entry:[[:space:]]*//p' "$fx" | head -1)"
    [ -n "$entry" ] || continue

    exp_out="$(sed -n 's/^-- expect: stdout = "\(.*\)"$/\1/p' "$fx")"
    exp_exit="$(sed -n 's/^-- expect: exit = \([0-9]*\)$/\1/p' "$fx" | head -1)"
    [ -n "$exp_exit" ] || exp_exit=0

    flat="$(mktemp)"; smt="$(mktemp)"; out="$(mktemp)"
    if ! "$FLATTEN" "$fx" > "$flat" 2>/dev/null; then
        echo "FAIL $name — flatten failed"; fail=$((fail+1)); rm -f "$flat" "$smt" "$out"; continue
    fi
    if ! "$ORACLE" emit "$flat" "$entry" -o "$smt" 2>/dev/null; then
        echo "FAIL $name — oracle emit failed (constraint dropped / parse error)"; fail=$((fail+1)); rm -f "$flat" "$smt" "$out"; continue
    fi
    "$KERNEL" "$smt" 2>/dev/null | grep -v '^\[functionizer\]' > "$out"
    act_exit=${PIPESTATUS[0]}
    act_out="$(cat "$out")"
    rm -f "$flat" "$smt" "$out"

    if [ "$act_out" = "$exp_out" ] && [ "$act_exit" = "$exp_exit" ]; then
        echo "PASS $name — exit=$act_exit"
        pass=$((pass+1))
    else
        echo "FAIL $name — expected exit=$exp_exit got exit=$act_exit"
        [ "$act_out" = "$exp_out" ] || { echo "  expected stdout: [$exp_out]"; echo "  actual   stdout: [$act_out]"; }
        fail=$((fail+1))
    fi
done

echo "---"
echo "seq: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
