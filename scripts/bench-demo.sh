#!/usr/bin/env bash
# bench-demo — measure a demo's wall + steady-state per-tick cost.
#
# Usage:
#   scripts/bench-demo.sh <demo-path> [-n RUNS] [-s START_TICK] [env=val...]
#
# `-s N` means "start recording at tick N" — ticks 0..N-1 are treated as
# warmup. Default: N=1 (skip tick 0, which carries one-shot setup +
# JIT compile cost).
#
# Each run is invoked with `EVIDENT_TICK_MS=1 --timing` and the per-tick
# solve lines are parsed. Reports:
#   * wall           — full wall-clock as reported by --timing
#   * tick0          — solve time of the very first tick (one-shot setup)
#   * wall_post      — wall minus the warmup ticks' solve time:
#                      "if we'd started at tick N, total cost would be ~X"
#   * steady_median  — median per-tick solve cost from tick N onwards
#   * num_ticks      — total ticks observed
#
# Examples:
#   scripts/bench-demo.sh examples/test_25_per_component_jit.ev
#   scripts/bench-demo.sh examples/test_26_value_cache.ev -n 5 EVIDENT_VALUE_CACHE=0
#   scripts/bench-demo.sh examples/test_21_mario/main.ev -s 30
#       # exclude first 30 ticks (Mario's setup) from steady-state numbers

ROOT_BD="$(cd "$(dirname "$0")/.." && pwd)"
EVIDENT=${EVIDENT:-$("$ROOT_BD/scripts/evident-self" bin)}
RUNS=3
START=1

# ── parse args ────────────────────────────────────────────────
DEMO=""
EXTRA_ENV=""   # space-separated KEY=VAL list
while [ $# -gt 0 ]; do
    case "$1" in
        -n)         RUNS=$2; shift 2 ;;
        -s|-w)      START=$2; shift 2 ;;
        *=*)        EXTRA_ENV="$EXTRA_ENV $1"; shift ;;
        *)
            if [ -z "$DEMO" ]; then DEMO=$1; shift
            else echo "unexpected arg: $1" >&2; exit 2
            fi
            ;;
    esac
done

if [ -z "$DEMO" ] || [ ! -f "$DEMO" ]; then
    echo "Usage: $0 <demo-path> [-n RUNS] [-w WARMUP_TICKS] [env=val...]" >&2
    exit 2
fi

# ── one bench run ─────────────────────────────────────────────
# Echoes one line: <wall_ms>|<tick0_ms>|<wall_post_ms>|<median_ms>|<num_ticks>
one_run() {
    local out tmp
    tmp=$(mktemp)
    env $EXTRA_ENV EVIDENT_TICK_MS=1 \
        timeout 120 "$EVIDENT" effect-run "$DEMO" --timing >"$tmp" 2>&1

    local wall
    wall=$(awk '/^\[timing\] wall:/{print $3; exit}' "$tmp" | sed 's/ms//')

    # Per-tick solve numbers, in order. Each matching line looks like
    #   [timing] tick 0 fsm=sim: solve=1.19ms (1 effects)
    # Extract the substring after "solve=" and strip "ms".
    local ticks_file
    ticks_file=$(mktemp)
    awk '/^\[timing\] tick [0-9]+ fsm=.*solve=/{
        match($0, /solve=[0-9.]+ms/);
        v=substr($0, RSTART+6, RLENGTH-8);
        print v
    }' "$tmp" > "$ticks_file"

    local n tick0
    n=$(wc -l < "$ticks_file" | tr -d ' ')
    tick0=$(head -1 "$ticks_file")
    [ -z "$tick0" ] && tick0="0"

    # Sum of the warmup ticks' solve cost — what `wall_post` subtracts.
    local warmup_solve_sum
    warmup_solve_sum=$(head -$START "$ticks_file" | awk '{s+=$1} END{printf "%.2f", s+0}')

    # wall_post = wall - warmup_solve_sum  ("if we'd started at tick N").
    local wall_post
    wall_post=$(awk -v w="${wall:-0}" -v s="$warmup_solve_sum" 'BEGIN{printf "%.2f", w - s}')

    # Drop first START ticks, sort numerically, pick middle.
    local median
    median=$(tail -n +$((START + 1)) "$ticks_file" | sort -n)
    if [ -z "$median" ]; then
        median="-"
    else
        local k mid
        k=$(echo "$median" | wc -l | tr -d ' ')
        mid=$(( (k + 1) / 2 ))
        median=$(echo "$median" | sed -n "${mid}p")
    fi

    rm -f "$tmp" "$ticks_file"
    echo "${wall:-0}|${tick0}|${wall_post}|${median}|${n}"
}

# ── run RUNS times, aggregate ─────────────────────────────────
echo "demo:    $DEMO"
echo "runs:    $RUNS   (start recording from tick: $START)"
[ -n "$EXTRA_ENV" ] && echo "env:    $EXTRA_ENV"
echo ""
printf "%-5s %10s %10s %12s %14s %6s\n" \
    "run" "wall(ms)" "tick0(ms)" "wall_post" "steady-median" "ticks"

wall_file=$(mktemp); tick0_file=$(mktemp); post_file=$(mktemp); med_file=$(mktemp)
for i in $(seq 1 $RUNS); do
    line=$(one_run)
    wall=$(echo  "$line" | cut -d'|' -f1)
    tick0=$(echo "$line" | cut -d'|' -f2)
    post=$(echo  "$line" | cut -d'|' -f3)
    med=$(echo   "$line" | cut -d'|' -f4)
    nt=$(echo    "$line" | cut -d'|' -f5)

    printf "%-5d %10s %10s %12s %14s %6s\n" \
        "$i" "$wall" "$tick0" "$post" "$med" "$nt"
    echo "$wall"  >> "$wall_file"
    echo "$tick0" >> "$tick0_file"
    echo "$post"  >> "$post_file"
    [ "$med" != "-" ] && echo "$med" >> "$med_file"
done

# Median of medians for the summary
pick_median() {
    local f=$1
    local k mid
    k=$(wc -l < "$f" | tr -d ' ')
    [ "$k" -eq 0 ] && { echo "-"; return; }
    mid=$(( (k + 1) / 2 ))
    sort -n "$f" | sed -n "${mid}p"
}
echo ""
printf "medians: wall=%sms  tick0=%sms  wall_post=%sms  steady=%sms/tick\n" \
    "$(pick_median "$wall_file")" \
    "$(pick_median "$tick0_file")" \
    "$(pick_median "$post_file")" \
    "$(pick_median "$med_file")"
rm -f "$wall_file" "$tick0_file" "$post_file" "$med_file"
