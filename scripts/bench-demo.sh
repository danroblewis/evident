#!/usr/bin/env bash
# bench-demo — measure a demo's wall time + steady-state per-tick cost.
#
# Usage:
#   scripts/bench-demo.sh <demo-path> [-n RUNS] [-w WARMUP_TICKS] [env=val...]
#
# Each run is invoked with `EVIDENT_TICK_MS=1 --timing` and the per-tick
# solve lines are parsed. Reports:
#   * wall_total      — total wall time (includes tick 0 compile cost)
#   * tick0           — cost of the first tick (one-shot setup + first call)
#   * steady_median   — median per-tick cost after dropping the first
#                       WARMUP_TICKS (default 1)
#   * num_ticks       — total ticks observed
#
# Examples:
#   scripts/bench-demo.sh examples/test_25_per_component_jit.ev
#   scripts/bench-demo.sh examples/test_26_value_cache.ev -n 5 EVIDENT_VALUE_CACHE=0

EVIDENT=${EVIDENT:-./runtime/target/release/evident}
RUNS=3
WARMUP=1

# ── parse args ────────────────────────────────────────────────
DEMO=""
EXTRA_ENV=""   # space-separated KEY=VAL list
while [ $# -gt 0 ]; do
    case "$1" in
        -n) RUNS=$2; shift 2 ;;
        -w) WARMUP=$2; shift 2 ;;
        *=*) EXTRA_ENV="$EXTRA_ENV $1"; shift ;;
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
# Echoes one line: <wall_ms>|<tick0_ms>|<median_ms>|<num_ticks>
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

    # Drop first WARMUP, sort numerically, pick middle.
    local median
    median=$(tail -n +$((WARMUP + 1)) "$ticks_file" | sort -n)
    if [ -z "$median" ]; then
        median="-"
    else
        local k mid
        k=$(echo "$median" | wc -l | tr -d ' ')
        mid=$(( (k + 1) / 2 ))
        median=$(echo "$median" | sed -n "${mid}p")
    fi

    rm -f "$tmp" "$ticks_file"
    echo "${wall:-0}|${tick0}|${median}|${n}"
}

# ── run RUNS times, aggregate ─────────────────────────────────
echo "demo:    $DEMO"
echo "runs:    $RUNS   (warmup ticks skipped: $WARMUP)"
[ -n "$EXTRA_ENV" ] && echo "env:    $EXTRA_ENV"
echo ""
printf "%-6s %12s %12s %16s %6s\n" "run" "wall(ms)" "tick0(ms)" "steady-median" "ticks"

wall_file=$(mktemp); tick0_file=$(mktemp); med_file=$(mktemp)
for i in $(seq 1 $RUNS); do
    line=$(one_run)
    wall=$(echo "$line" | cut -d'|' -f1)
    tick0=$(echo "$line" | cut -d'|' -f2)
    med=$(echo "$line" | cut -d'|' -f3)
    nt=$(echo "$line" | cut -d'|' -f4)

    printf "%-6d %12s %12s %16s %6s\n" "$i" "$wall" "$tick0" "$med" "$nt"
    echo "$wall"  >> "$wall_file"
    echo "$tick0" >> "$tick0_file"
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
printf "medians: wall=%sms  tick0=%sms  steady=%sms/tick\n" \
    "$(pick_median "$wall_file")" \
    "$(pick_median "$tick0_file")" \
    "$(pick_median "$med_file")"
rm -f "$wall_file" "$tick0_file" "$med_file"
