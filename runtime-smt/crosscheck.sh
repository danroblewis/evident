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

if [[ "$fail" == 0 ]]; then echo "cross-check: all paired fixtures agree"; else echo "cross-check: MISMATCH"; fi
exit $fail
