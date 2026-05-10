#!/usr/bin/env bash
# ./test.sh — run every test in this repo.
#
# Phases (each phase fails the run if it fails):
#   1. Build the Rust binary (release).
#   2. cargo test --release in runtime/ — Rust units + integration
#      tests. Includes the multi-FSM scheduler tests and the demo
#      driver (which runs every programs/demos/test_*.ev).
#   3. pytest tests/conformance/ — black-box CLI conformance tests.
#
# This is THE test command. Any time an agent finishes a chunk of
# work that touches code or stdlib or programs/, run this before
# declaring done. Anything less leaves room for the kind of drift
# that the conformance triage just surfaced.
#
# Usage:
#   ./test.sh                   # run everything
#   ./test.sh --rust-only       # skip conformance phase
#   ./test.sh --conformance     # only run conformance phase
#                               # (useful while iterating; skips the
#                               # ~30s release build)

set -e -o pipefail

cd "$(dirname "$0")"

RUST_ONLY=0
CONFORMANCE_ONLY=0
for arg in "$@"; do
    case "$arg" in
        --rust-only)    RUST_ONLY=1 ;;
        --conformance)  CONFORMANCE_ONLY=1 ;;
        -h|--help)
            sed -n '2,16p' "$0"
            exit 0
            ;;
        *)
            echo "test.sh: unknown flag $arg" >&2
            exit 2
            ;;
    esac
done

# Color support (only when stdout is a tty).
if [ -t 1 ]; then
    BOLD=$(printf '\033[1m')
    GREEN=$(printf '\033[0;32m')
    RED=$(printf '\033[0;31m')
    DIM=$(printf '\033[2m')
    OFF=$(printf '\033[0m')
else
    BOLD=''; GREEN=''; RED=''; DIM=''; OFF=''
fi

phase() { echo "${BOLD}── $1 ──${OFF}"; }
ok()    { echo "${GREEN}✓${OFF} $1"; }
fail()  { echo "${RED}✗${OFF} $1" >&2; }

started=$(date +%s)
failures=()

if [ "$CONFORMANCE_ONLY" -eq 0 ]; then
    phase "Phase 1: build runtime (release)"
    if (cd runtime && cargo build --release 2>&1 | tail -3); then
        ok "build"
    else
        fail "build"
        failures+=("build")
    fi
    echo

    phase "Phase 2: cargo test --release (runtime/)"
    # Counts: each "test result: ok. N passed; M failed" line.
    # Failing the script if cargo exits non-zero — its own exit
    # code is the source of truth.
    if (cd runtime && cargo test --release 2>&1 | tee /tmp/evident-cargo-test.log) ; then
        passed=$(grep "^test result" /tmp/evident-cargo-test.log \
                 | awk '{p+=$4} END {print p+0}')
        ok "cargo test: $passed passed"
    else
        passed=$(grep "^test result" /tmp/evident-cargo-test.log \
                 | awk '{p+=$4} END {print p+0}')
        failed=$(grep "^test result" /tmp/evident-cargo-test.log \
                 | awk '{f+=$6} END {print f+0}')
        fail "cargo test: $passed passed, $failed failed"
        failures+=("cargo test")
    fi
    echo
fi

if [ "$RUST_ONLY" -eq 0 ]; then
    phase "Phase 3: conformance (tests/conformance/)"
    # Check pytest is available; if not, skip with a warning rather
    # than silently passing.
    if ! command -v pytest >/dev/null 2>&1; then
        fail "pytest not found in PATH; install it or run inside a venv"
        failures+=("conformance: pytest missing")
    else
        # The conftest defaults EVIDENT_CMD to the release binary
        # under runtime/target/release/evident. Build phase ensures
        # that path exists when running the full suite.
        if pytest tests/conformance/ -q --tb=short 2>&1 | tee /tmp/evident-pytest.log ; then
            counts=$(grep -E "[0-9]+ passed" /tmp/evident-pytest.log | tail -1)
            ok "conformance: $counts"
        else
            counts=$(grep -E "[0-9]+ (passed|failed)" /tmp/evident-pytest.log | tail -1)
            fail "conformance: $counts"
            failures+=("conformance")
        fi
    fi
    echo
fi

elapsed=$(( $(date +%s) - started ))

if [ ${#failures[@]} -eq 0 ]; then
    echo "${GREEN}${BOLD}All phases passed.${OFF} ${DIM}(${elapsed}s)${OFF}"
    exit 0
else
    echo "${RED}${BOLD}FAILED:${OFF} ${failures[*]} ${DIM}(${elapsed}s)${OFF}"
    exit 1
fi
