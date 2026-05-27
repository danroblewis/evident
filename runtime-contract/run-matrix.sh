#!/usr/bin/env bash
# run-matrix.sh — run the dual-engine behavior-contract matrix end to end.
#
# Runs every engine that implements the FsmEngine contract over all 15 fixtures
# and prints each engine's pass/fail-matrix column:
#
#   * CurrentRuntime + SmtLib(pure-Z3)  — runtime/tests/behavior_contract.rs
#       (the original gate: the real runtime produces the golden, and Z3 alone
#        confirms the SMT-LIB capture is faithful — Method A/B/UNSAT)
#   * Existing+SMTLIB v1 + enum-increment (strategy 2)
#                                       — runtime/tests/contract_evolve.rs
#   * runtime-smt greenfield (strategy 1)
#                                       — runtime-smt/tests/contract.rs
#
# A FAIL (wrong answer) fails the corresponding cargo test; a Gap (documented
# capability boundary) is green. Run from anywhere.
#
# Usage:  runtime-contract/run-matrix.sh
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
cd "$ROOT" || exit 2

sep() { printf '\n================================================================\n%s\n================================================================\n' "$1"; }

# The two new engines need the oracle's release deps; the original two engines
# (behavior_contract.rs) need the real runtime built. Build release once.
echo "building runtime (release) …"
cargo build --release --manifest-path runtime/Cargo.toml >/dev/null 2>&1 || { echo "runtime build failed"; exit 1; }

fail=0

sep "Original gate — CurrentRuntime + SmtLib (pure-Z3)  [runtime/tests/behavior_contract.rs]"
cargo test --release --manifest-path runtime/Cargo.toml --test behavior_contract -- --nocapture 2>&1 \
    | grep -E "✓|✗|passed|FAILED|result" | grep -vE "warning|Compiling|Finished" || true
cargo test --release --manifest-path runtime/Cargo.toml --test behavior_contract >/dev/null 2>&1 || fail=1

sep "Strategy 2 — Existing runtime in SMT-LIB mode (v1 scalar + enum-increment)  [runtime/tests/contract_evolve.rs]"
cargo test --release --manifest-path runtime/Cargo.toml --test contract_evolve -- --nocapture 2>&1 \
    | sed -n '/| Fixture |/,/test result/p' || true
cargo test --release --manifest-path runtime/Cargo.toml --test contract_evolve >/dev/null 2>&1 || fail=1

sep "Strategy 1 — Greenfield SMT-LIB engine  [runtime-smt/tests/contract.rs]"
cargo test --manifest-path runtime-smt/Cargo.toml --test contract -- --nocapture 2>&1 \
    | sed -n '/| Fixture |/,/test result/p' || true
cargo test --manifest-path runtime-smt/Cargo.toml --test contract >/dev/null 2>&1 || fail=1

sep "Convergence — Evident FSM → SMT-LIB → greenfield engine, vs the binary  [runtime-smt/crosscheck.sh]"
runtime-smt/crosscheck.sh 2>&1 | grep -E "OK|FAIL|agree|MISMATCH" || true
runtime-smt/crosscheck.sh >/dev/null 2>&1 || fail=1

sep "RESULT"
if [ "$fail" -eq 0 ]; then
    echo "all engines green (passes + documented gaps; zero wrong answers); convergence byte-identical"
    exit 0
else
    echo "a matrix engine reported a FAIL (wrong answer) or the convergence diverged — see above"
    exit 1
fi
