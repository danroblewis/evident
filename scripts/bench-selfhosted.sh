#!/usr/bin/env bash
# bench-selfhosted.sh — E3 perf benchmark for the self-hosted pipeline.
#
# Times the kernel-side pipeline fixtures end-to-end (evident emit +
# kernel exec) and reports total + per-fixture wall-clock. The gate is
# the Phase E3 acceptance threshold from docs/plans/completion-roadmap.md:
#
#     "compiling a 100-line `.ev` file → `.smt2` finishes in < 60s"
#
# Since the full self-hosted compiler doesn't exist yet, we proxy the
# threshold against the three pipeline fixtures that demonstrate the
# kernel-driven compile pattern:
#
#     tests/kernel/test_pipeline_lex_parse.ev
#     tests/kernel/test_pipeline_full.ev
#     tests/kernel/test_pipeline_full_d2.ev
#
# Usage:
#     scripts/bench-selfhosted.sh           # run, report, gate
#     scripts/bench-selfhosted.sh --quiet   # numbers only, no banners
#
# Exit code:
#     0  total wall-clock < 60s
#     1  total wall-clock ≥ 60s (perf regression — investigate)
#     2  one of the fixtures failed to compile or run

set -u
cd "$(dirname "$0")/.."

QUIET=0
for arg in "$@"; do
    case "$arg" in
        --quiet|-q) QUIET=1 ;;
        -h|--help)
            sed -n '2,26p' "$0"; exit 0 ;;
        *)
            echo "bench-selfhosted.sh: unknown flag $arg" >&2; exit 2 ;;
    esac
done

EVIDENT=${EVIDENT:-./bootstrap/runtime/target/release/evident}
KERNEL=${KERNEL:-./kernel/target/release/kernel}
THRESHOLD_S=${THRESHOLD_S:-60}

if [ ! -x "$EVIDENT" ]; then
    echo "bench-selfhosted.sh: $EVIDENT not found — run ./test.sh first" >&2
    exit 2
fi
if [ ! -x "$KERNEL" ]; then
    echo "bench-selfhosted.sh: $KERNEL not found — run ./test.sh first" >&2
    exit 2
fi

FIXTURES=(
    "tests/kernel/test_pipeline_lex_parse.ev"
    "tests/kernel/test_pipeline_full.ev"
    "tests/kernel/test_pipeline_full_d2.ev"
)

if [ "$QUIET" -eq 0 ]; then
    echo "── E3 self-hosted perf benchmark ──"
    echo "threshold: total wall-clock < ${THRESHOLD_S}s"
    echo
    printf "%-44s %12s\n" "fixture" "wall(s)"
    printf "%-44s %12s\n" "-------" "-------"
fi

# Use awk for portable floating-point summing (bash itself does int only).
total_s="0"
fail=0
for fixture in "${FIXTURES[@]}"; do
    if [ ! -f "$fixture" ]; then
        echo "bench-selfhosted.sh: missing fixture $fixture" >&2
        fail=1
        continue
    fi

    smt_out=$(mktemp -t bench-selfhosted.XXXXXX.smt2)
    timing_out=$(mktemp -t bench-selfhosted-time.XXXXXX)

    # /usr/bin/time -p emits POSIX-format wall/user/sys on three lines.
    # The `real` line is wall-clock in seconds (decimal).
    /usr/bin/time -p sh -c \
        "'$EVIDENT' emit '$fixture' main -o '$smt_out' && '$KERNEL' '$smt_out' > /dev/null" \
        >/dev/null 2>"$timing_out"
    rc=$?

    wall=$(awk '/^real/{print $2; exit}' "$timing_out")
    [ -z "$wall" ] && wall="0.00"

    rm -f "$smt_out" "$timing_out"

    if [ "$rc" -ne 0 ]; then
        if [ "$QUIET" -eq 0 ]; then
            printf "%-44s %12s  FAILED (exit %d)\n" "$fixture" "$wall" "$rc"
        fi
        fail=1
        continue
    fi

    if [ "$QUIET" -eq 0 ]; then
        printf "%-44s %12s\n" "$fixture" "$wall"
    fi
    total_s=$(awk -v a="$total_s" -v b="$wall" 'BEGIN{printf "%.3f", a + b}')
done

if [ "$fail" -ne 0 ]; then
    echo "bench-selfhosted.sh: one or more fixtures failed" >&2
    exit 2
fi

if [ "$QUIET" -eq 0 ]; then
    echo
    printf "total wall: %ss   (threshold %ss)\n" "$total_s" "$THRESHOLD_S"
fi

# Gate: total_s < THRESHOLD_S
under=$(awk -v t="$total_s" -v th="$THRESHOLD_S" 'BEGIN{print (t+0 < th+0) ? 1 : 0}')
if [ "$under" -eq 1 ]; then
    if [ "$QUIET" -eq 0 ]; then
        echo "PASS: under threshold."
    fi
    exit 0
else
    echo "WARN: total ${total_s}s exceeds ${THRESHOLD_S}s threshold (E3 gate)." >&2
    exit 1
fi
