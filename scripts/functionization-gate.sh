#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# functionization-gate.sh — performance guard for the FTI / driver-IR
# types and the compiler as a whole.
#
# WHY THIS EXISTS. The driver-IR types (FtiBuffer, Z3SolverCtx, …) are
# carried state re-checked EVERY tick, and the compiler ticks thousands
# of times per compile, so their per-tick cost multiplies. A constraint
# that the functionizer cannot keep on the fast path falls to Z3 and is
# re-solved each tick — catastrophic at scale. The classic offender is a
# disequality (`≠`): non-convex, so Z3 case-splits. (Bounds/comparisons
# stay convex and functionize for free.) A single bad `≠` invariant on a
# carried record took conformance fixture-001 from 19 s to a 30-min
# timeout (2026-06-08). This gate catches that class of regression fast.
#
# It checks two layers:
#   A. micro fixtures (tests/compiler2_units/perf/*.ev) — each exercises
#      a type's invariant over many ticks; headers declare the budget:
#        -- perf: max_z3_ms = <N>     (total Z3 ms across the run)
#        -- perf: max_wall_s = <N>
#        -- expect: exit = <N>
#   B. the compiler itself — build stage1 from compiler2/driver.ev and
#      compile a small conformance fixture; assert it stays (near-)fully
#      functionized (low total Z3 ms) and under a wall ceiling.
#
# Exit: 0 = all within budget, 1 = a budget blown / wrong exit, 2 = setup.
#
# DEPENDS ON RUST-KERNEL INSTRUMENTATION (EVIDENT_FUNCTIONIZE_STATS). When
# the functionizer moves into Evident (wave 5c) the equivalent must be
# re-exposed — see docs/plans/wave-5c-functionizer-in-evident.md.

set -u -o pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/.." && pwd)"
ORACLE="${EVIDENT_ORACLE:-/usr/local/bin/evident-oracle}"
KERNEL="${EVIDENT_KERNEL:-$ROOT/kernel/target/release/kernel}"
FLATTEN="$ROOT/scripts/flatten-evident.sh"
PERF_DIR="$ROOT/tests/compiler2_units/perf"
# Compiler-level budgets (generous; the signal is "0 vs blows up").
COMPILER_FIXTURE="${COMPILER_FIXTURE:-$ROOT/tests/conformance/features/001-int-arithmetic-add}"
COMPILER_MAX_Z3_MS="${COMPILER_MAX_Z3_MS:-200}"
COMPILER_MAX_WALL_S="${COMPILER_MAX_WALL_S:-60}"

[ -x "$ORACLE" ] || { echo "gate: oracle not executable: $ORACLE" >&2; exit 2; }
[ -x "$KERNEL" ] || { echo "gate: kernel not executable: $KERNEL" >&2; exit 2; }

# parse_z3_ms / parse_total_ms: read the `[functionizer] … (… / X ms z3)` line.
parse_z3_ms()    { grep -oE '[0-9.]+ ms z3\)'   "$1" | head -1 | grep -oE '[0-9.]+' | head -1; }
parse_total_ms() { grep -oE '; [0-9.]+ ms total' "$1" | head -1 | grep -oE '[0-9.]+' | head -1; }
# float a<=b
fle() { awk -v a="$1" -v b="$2" 'BEGIN{exit !(a+0<=b+0)}'; }

fail=0

echo "── A. micro FTI perf fixtures ───────────────────────────────"
shopt -s nullglob
for fx in "$PERF_DIR"/*.ev; do
    name="$(basename "$fx")"
    entry="$(sed -n 's/^-- entry:[[:space:]]*//p' "$fx" | head -1)"; entry="${entry:-main}"
    exp_exit="$(sed -n 's/^-- expect: exit = \([0-9]*\)$/\1/p' "$fx" | head -1)"; exp_exit="${exp_exit:-0}"
    max_z3="$(sed -n 's/^-- perf: max_z3_ms = \([0-9]*\)$/\1/p' "$fx" | head -1)"; max_z3="${max_z3:-50}"
    max_wall="$(sed -n 's/^-- perf: max_wall_s = \([0-9]*\)$/\1/p' "$fx" | head -1)"; max_wall="${max_wall:-30}"

    flat="$(mktemp)"; smt="$(mktemp)"; fz="$(mktemp)"
    if ! "$FLATTEN" "$fx" > "$flat" 2>/dev/null; then echo "  FAIL $name — flatten"; fail=1; rm -f "$flat" "$smt" "$fz"; continue; fi
    if ! "$ORACLE" emit "$flat" "$entry" -o "$smt" 2>/dev/null; then echo "  FAIL $name — emit"; fail=1; rm -f "$flat" "$smt" "$fz"; continue; fi
    t0=$(date +%s); timeout "$((max_wall+5))" "$KERNEL" "$smt" >/dev/null 2>"$fz"; rc=$?; t1=$(date +%s)
    wall=$((t1-t0)); z3="$(parse_z3_ms "$fz")"; z3="${z3:-0}"; tot="$(parse_total_ms "$fz")"; tot="${tot:-?}"

    why=""
    [ "$rc" = "$exp_exit" ] || why="$why exit=$rc≠$exp_exit"
    fle "$z3" "$max_z3"   || why="$why z3=${z3}ms>${max_z3}"
    fle "$wall" "$max_wall" || why="$why wall=${wall}s>${max_wall}"
    if [ -z "$why" ]; then
        echo "  PASS $name — exit=$rc wall=${wall}s z3=${z3}ms (≤${max_z3}) total=${tot}ms"
    else
        echo "  FAIL $name —$why"; fail=1
    fi
    rm -f "$flat" "$smt" "$fz"
done

echo "── B. compiler-level functionization ────────────────────────"
stage1="$(mktemp)"; dflat="$(mktemp)"
if ! "$FLATTEN" "$ROOT/compiler2/driver.ev" > "$dflat" 2>/dev/null; then echo "  FAIL stage1 flatten"; fail=1
elif ! "$ORACLE" emit "$dflat" driver_main -o "$stage1" 2>/dev/null; then echo "  FAIL stage1 emit"; fail=1
else
    sflat="$(mktemp)"; "$FLATTEN" "$COMPILER_FIXTURE/source.ev" > "$sflat" 2>/dev/null
    claim="$(cat "$COMPILER_FIXTURE/claim.txt" 2>/dev/null || echo main)"
    fz="$(mktemp)"
    t0=$(date +%s)
    printf '%s\n%s\n' "$sflat" "$claim" \
        | timeout "$((COMPILER_MAX_WALL_S+10))" env EVIDENT_FUNCTIONIZE_STATS=summary "$KERNEL" "$stage1" >/dev/null 2>"$fz"
    rc=$?; t1=$(date +%s); wall=$((t1-t0))
    z3="$(parse_z3_ms "$fz")"; z3="${z3:-?}"; tot="$(parse_total_ms "$fz")"; tot="${tot:-?}"
    fxn="$(basename "$COMPILER_FIXTURE")"
    why=""
    [ "$rc" = 0 ] || why="$why compile-exit=$rc"
    [ "$z3" = "?" ] && why="$why no-functionizer-line"
    [ "$z3" = "?" ] || fle "$z3" "$COMPILER_MAX_Z3_MS" || why="$why z3=${z3}ms>${COMPILER_MAX_Z3_MS}"
    fle "$wall" "$COMPILER_MAX_WALL_S" || why="$why wall=${wall}s>${COMPILER_MAX_WALL_S}"
    if [ -z "$why" ]; then
        echo "  PASS compiler/$fxn — wall=${wall}s z3=${z3}ms (≤${COMPILER_MAX_Z3_MS}) total=${tot}ms"
    else
        echo "  FAIL compiler/$fxn —$why"; fail=1
    fi
    rm -f "$sflat" "$fz"
fi
rm -f "$stage1" "$dflat"

echo "─────────────────────────────────────────────────────────────"
[ "$fail" = 0 ] && { echo "functionization-gate: GREEN"; exit 0; } || { echo "functionization-gate: RED" >&2; exit 1; }
