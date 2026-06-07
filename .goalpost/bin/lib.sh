# .goalpost/bin/lib.sh — shared plumbing for the compiler2 goalpost
# harnesses. Sourced, not executed.
#
# The harnesses are the EXPENSIVE half of the artifact pattern: they
# actually compile fixtures through compiler2 under the kernel and drop
# machine-readable JSON into .goalpost/artifacts/. The measure scripts
# in .goalpost/measures/ only ever READ those artifacts.
#
# Nothing here mutates the repo: all writes go to .goalpost/artifacts/
# (gitignored) and mktemp scratch.

set -u

GP_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GP_ART="$GP_ROOT/.goalpost/artifacts"
GP_KERNEL="$GP_ROOT/kernel/target/release/kernel"
GP_FLATTEN="$GP_ROOT/scripts/flatten-evident.sh"
GP_ORACLE="${EVIDENT_ORACLE:-/usr/local/bin/evident-oracle}"
GP_STAGE1="$GP_ART/compiler2-stage1.smt2"

# Tunables (env-overridable). Defaults sized for the real, canonical
# run; a reduced-budget run is fine but is RECORDED in the artifact so
# the measures' consumers can see the cap that was used.
GP_C2_TIMEOUT="${EVIDENT_C2_TIMEOUT:-1800}"          # s, per fixture compile via compiler2
GP_RUN_TIMEOUT="${EVIDENT_C2_RUN_TIMEOUT:-240}"      # s, running an emitted unit
GP_JOBS="${EVIDENT_C2_JOBS:-8}"

gp_die() { echo "goalpost: $*" >&2; exit 2; }

gp_require_tools() {
    [ -x "$GP_KERNEL" ]  || gp_die "kernel binary missing at $GP_KERNEL (cargo build --release in kernel/)"
    [ -x "$GP_FLATTEN" ] || gp_die "flatten script missing at $GP_FLATTEN"
    command -v jq >/dev/null || gp_die "jq not on PATH"
    mkdir -p "$GP_ART"
}

# Build (or reuse) the stage-1 compiler2 artifact: compiler2/driver.ev
# compiled to .smt2 so the kernel can run it. Today the only builder is
# the bootstrap oracle; when compiler2 self-hosts, a stage2 produced by
# run-selfhost.sh can be copied over GP_STAGE1 and the oracle removed.
# Records which builder produced it in GP_STAGE1.builder.
gp_build_stage1() {
    if [ -x "$GP_ORACLE" ]; then
        "$GP_ORACLE" emit "$GP_ROOT/compiler2/driver.ev" driver_main -o "$GP_STAGE1" \
            || gp_die "oracle emit of compiler2/driver.ev failed"
        echo oracle > "$GP_STAGE1.builder"
    elif [ -s "$GP_STAGE1" ]; then
        # Oracle gone (post-sunset): reuse the existing stage1 (expected
        # to be a self-produced stage2 dropped here by run-selfhost.sh).
        [ -f "$GP_STAGE1.builder" ] || echo unknown > "$GP_STAGE1.builder"
    else
        gp_die "no compiler2 stage1 builder: oracle absent and no $GP_STAGE1"
    fi
    head -1 "$GP_STAGE1" | grep -q '^;; manifest:' \
        || gp_die "stage1 artifact has no manifest header"
}

gp_stage1_builder() { cat "$GP_STAGE1.builder" 2>/dev/null || echo unknown; }

# gp_c2_compile <compiler.smt2> <src.ev> <claim> <out.smt2> <timeout_s>
#   wave-4o wire protocol: stdin line 1 = flattened source path,
#   line 2 = target claim. Returns 0 ok / 124 timeout / 1 error.
gp_c2_compile() {
    local comp="$1" src="$2" claim="$3" out="$4" tmo="$5"
    local flat; flat="$(mktemp -t gp-flat.XXXXXX.ev)"
    if ! "$GP_FLATTEN" "$src" > "$flat" 2>/dev/null; then
        rm -f "$flat"; return 1
    fi
    printf '%s\n%s\n' "$flat" "$claim" \
        | timeout "$tmo" "$GP_KERNEL" "$comp" 2>/dev/null \
        | grep -v '^\[functionizer\]' > "$out"
    local rc=${PIPESTATUS[1]}
    rm -f "$flat"
    [ "$rc" -eq 124 ] && return 124
    [ "$rc" -ne 0 ] && return 1
    [ -s "$out" ] || return 1
    return 0
}

# gp_run_unit <unit.smt2> <stdout_file> — runs an emitted unit under the
# kernel; prints the kernel's exit code; kernel stdout (functionizer
# line stripped) lands in <stdout_file>.
gp_run_unit() {
    local unit="$1" outf="$2"
    timeout "$GP_RUN_TIMEOUT" "$GP_KERNEL" "$unit" 2>/dev/null \
        | grep -v '^\[functionizer\]' > "$outf"
    echo "${PIPESTATUS[0]}"
}

gp_now() { date +%s; }
