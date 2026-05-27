#!/usr/bin/env bash
# Cross-check runtime-smt against the legacy Rust runtime (the oracle).
#
# For each paired fixture, run the SMT-LIB version through `runtime-smt run` and
# the equivalent Evident program through `evident effect-run`, and assert the
# stdout AND the exit code match byte-for-byte. This is the milestone gate:
# the new engine reproduces the legacy runtime's observable behavior.
#
# Usage:  runtime-smt/crosscheck.sh        (from anywhere)
# Assumes the oracle is built:  cargo build --release --manifest-path runtime/Cargo.toml
# Builds runtime-smt itself (debug) if needed.
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
ORACLE="$ROOT/runtime/target/release/evident"

cd "$ROOT" || exit 2
if [[ ! -x "$ORACLE" ]]; then
    echo "missing oracle $ORACLE — build it: cargo build --release --manifest-path runtime/Cargo.toml"
    exit 2
fi

echo "building runtime-smt (debug)…"
cargo build -q --manifest-path runtime-smt/Cargo.toml || exit 2
SMT="$ROOT/runtime-smt/target/debug/runtime-smt"

# Paired fixtures:  <smt-lib fixture>  <equivalent .ev>
PAIRS=(
    "runtime-smt/fixtures/countdown.smt2   runtime-smt/crosscheck/countdown.ev"
    "runtime-smt/fixtures/two_fsms.smt2    runtime-smt/crosscheck/two_fsms.ev"
)

fail=0
for pair in "${PAIRS[@]}"; do
    set -- $pair
    smt_fix="$1"; ev_fix="$2"
    [[ -f "$ROOT/$smt_fix" ]] || { continue; }   # fixture not landed yet
    [[ -f "$ROOT/$ev_fix" ]]  || { continue; }

    smt_out="$("$SMT" run "$smt_fix" 2>/dev/null)"; smt_code=$?
    ev_out="$("$ORACLE" effect-run "$ev_fix" 2>/dev/null)"; ev_code=$?

    if [[ "$smt_out" == "$ev_out" && "$smt_code" == "$ev_code" ]]; then
        echo "  OK   $(basename "$smt_fix")  (exit $smt_code, $(echo "$smt_out" | grep -c .) lines)"
    else
        echo "  FAIL $(basename "$smt_fix")  — runtime-smt vs oracle mismatch:"
        echo "    exit: runtime-smt=$smt_code oracle=$ev_code"
        diff <(echo "$smt_out") <(echo "$ev_out") | sed 's/^/      /'
        fail=1
    fi
done

# --- N4b front-end cross-check: transpile a scalar claim, compare sat verdict ---
# Pairs:  <scalar .ev>  <claim name>  (the front-end + oracle must agree on sat/unsat)
FE_PAIRS=(
    "runtime-smt/crosscheck/scalar.ev   sat_band"
)
for pair in "${FE_PAIRS[@]}"; do
    set -- $pair
    ev_fix="$1"; claim="$2"
    [[ -f "$ROOT/$ev_fix" ]] || continue
    fe_verdict="$("$SMT" transpile "$ev_fix" 2>/dev/null | head -1)"               # "sat" / "unsat"
    or_verdict="$("$ORACLE" sample "$ev_fix" "$claim" 2>/dev/null | grep -qi . && echo sat || echo unsat)"
    # `evident sample` prints model lines on sat, nothing on unsat → map to a verdict.
    if [[ "$fe_verdict" == "$or_verdict" ]]; then
        echo "  OK   $(basename "$ev_fix"):$claim  (front-end + oracle agree: $fe_verdict)"
    else
        echo "  FAIL $(basename "$ev_fix"):$claim  — front-end=$fe_verdict oracle=$or_verdict"
        fail=1
    fi
done

if [[ "$fail" == 0 ]]; then echo "cross-check: all paired fixtures agree"; else echo "cross-check: MISMATCH"; fi
exit $fail
