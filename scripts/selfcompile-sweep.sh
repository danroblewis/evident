#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# selfcompile-sweep.sh — fast per-module self-compile gate.
#
# The full `driver_main` self-compile (kernel stage1.smt2 < flat\ndriver_main)
# is the integration test for the self-hosting path, but it is SLOW (~10-45 min:
# it lexes ~750KB and builds a ~1MB result as carried state). The
# tests/compiler2_units/*.ev fixtures are small module-wrappers — each declares
# the minimal shared state and `..`-lifts ONE driver module — so self-compiling
# THEM through the compiler is a fraction of the work (~30-90s each) and
# localizes a gap to one module. This is the fast iteration loop; the full
# driver_main run remains the cross-module integration check (some gaps only
# form in the real merge — a per-module wrapper can't reproduce a sibling↔
# sibling carry back-edge).
#
# Usage:
#   scripts/selfcompile-sweep.sh                 # sweep all unit fixtures
#   scripts/selfcompile-sweep.sh driver_emit driver_quant   # only these modules
#   STAGE1=/path/to/stage1.smt2 scripts/selfcompile-sweep.sh # reuse a built stage1
#
# rc legend: 0 = self-compiles clean (emitted a stage2). 7/9 = a 0-handle /
# unresolved-name gap (the compiler can't translate some construct in that
# module). Other = different failure.

set -u -o pipefail
cd "$(dirname "$0")/.."

KERNEL=kernel/target/release/kernel
ORACLE="${EVIDENT_ORACLE:-/usr/local/bin/evident-oracle}"
UNITS_DIR=tests/compiler2_units
TIMEOUT="${SWEEP_TIMEOUT:-150}"
JOBS="${SWEEP_JOBS:-6}"

# 1. Build (or reuse) the stage1 compiler from the current compiler2/driver.ev.
STAGE1="${STAGE1:-}"
if [ -z "$STAGE1" ]; then
    echo "building stage1 from compiler2/driver.ev ..."
    DFLAT="$(mktemp -t sweep-drv.XXXXXX.ev)"
    scripts/flatten-evident.sh compiler2/driver.ev > "$DFLAT" 2>/dev/null \
        || { echo "FATAL: driver flatten failed"; exit 1; }
    STAGE1="$(mktemp -t sweep-stage1.XXXXXX.smt2)"
    "$ORACLE" emit "$DFLAT" driver_main -o "$STAGE1" 2>/dev/null \
        || { echo "FATAL: oracle emit of driver_main failed"; exit 1; }
    rm -f "$DFLAT"
fi
echo "stage1: $STAGE1"

# 2. Enumerate the fixtures to sweep (optionally filtered by module-name args).
mapfile -t FIXTURES < <(
    if [ "$#" -gt 0 ]; then
        for m in "$@"; do find "$UNITS_DIR/$m" -name '*.ev' 2>/dev/null; done
    else
        find "$UNITS_DIR" -name '*.ev'
    fi | sort
)
[ "${#FIXTURES[@]}" -gt 0 ] || { echo "no fixtures matched"; exit 1; }
echo "sweeping ${#FIXTURES[@]} fixtures (jobs=$JOBS, timeout=${TIMEOUT}s) ..."

# 3. Self-compile each fixture through stage1, in parallel.
RESULTS="$(mktemp -t sweep-res.XXXXXX)"
compile_one() {
    local fx="$1" stage1="$2" tmo="$3" kernel="$4"
    local nm flat out rc t0 dt el
    nm="$(echo "$fx" | sed "s#tests/compiler2_units/##; s#\.ev\$##")"
    flat="$(mktemp -t sweep-fx.XXXXXX.ev)"
    out="$(mktemp -t sweep-out.XXXXXX.smt2)"
    if ! scripts/flatten-evident.sh "$fx" > "$flat" 2>/dev/null; then
        echo "$nm FLATFAIL 0 0"; rm -f "$flat" "$out"; return
    fi
    t0=$SECONDS
    printf '%s\nmain\n' "$flat" | timeout "$tmo" "$kernel" "$stage1" 2>/dev/null \
        | grep -vE '^\[functionizer\]' > "$out"
    rc=${PIPESTATUS[1]}
    dt=$((SECONDS - t0)); el=$(wc -l < "$out")
    echo "$nm $rc $el $dt"
    rm -f "$flat" "$out"
}
export -f compile_one
printf '%s\n' "${FIXTURES[@]}" \
    | xargs -P "$JOBS" -I{} bash -c 'compile_one "$@"' _ {} "$STAGE1" "$TIMEOUT" "$KERNEL" \
    > "$RESULTS"

# 4. Report.
echo
echo "── per-fixture (rc 0 = clean self-compile; 7/9 = gap) ──"
sort "$RESULTS" | while read -r nm rc el dt; do
    tag=$([ "$rc" = 0 ] && echo "ok  " || echo "GAP ")
    printf "  %s %-44s rc=%s lines=%s wall=%ss\n" "$tag" "$nm" "$rc" "$el" "$dt"
done
clean=$(awk '$2==0' "$RESULTS" | wc -l)
total=$(wc -l < "$RESULTS")
echo "─────────────────────────────────────────────"
echo "selfcompile-sweep: $clean/$total fixtures self-compile clean"
rm -f "$RESULTS"
[ "$clean" -eq "$total" ]
