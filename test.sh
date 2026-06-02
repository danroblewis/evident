#!/usr/bin/env bash
# ./test.sh — run every test in this repo.
#
# Phases:
#   1. Build the Rust binary (release).
#   2. cargo test --release in runtime/.
#   3. pytest tests/conformance/ — black-box CLI conformance.
#   4. tests/lang_tests/*.ev — drive each via `evident sample --all --json`,
#      assert sat_*/unsat_* prefixes.
#
# Usage:
#   ./test.sh                   # all phases
#   ./test.sh --rust-only       # skip conformance + lang tests
#   ./test.sh --conformance     # only conformance
#   ./test.sh --lang            # only lang tests

set -e -o pipefail
cd "$(dirname "$0")"

RUST_ONLY=0
CONFORMANCE_ONLY=0
LANG_ONLY=0
for arg in "$@"; do
    case "$arg" in
        --rust-only)      RUST_ONLY=1 ;;
        --conformance)    CONFORMANCE_ONLY=1 ;;
        --lang)           LANG_ONLY=1 ;;
        -h|--help)
            sed -n '2,15p' "$0"; exit 0 ;;
        *)
            echo "test.sh: unknown flag $arg" >&2; exit 2 ;;
    esac
done

if [ -t 1 ]; then
    BOLD=$(printf '\033[1m'); GREEN=$(printf '\033[0;32m'); RED=$(printf '\033[0;31m')
    DIM=$(printf '\033[2m'); OFF=$(printf '\033[0m')
else
    BOLD=''; GREEN=''; RED=''; DIM=''; OFF=''
fi

phase() { echo "${BOLD}── $1 ──${OFF}"; }
ok()    { echo "${GREEN}✓${OFF} $1"; }
fail()  { echo "${RED}✗${OFF} $1" >&2; }

started=$(date +%s)
failures=()

# ── Phase 1: build ───────────────────────────────────────────
if [ "$CONFORMANCE_ONLY" -eq 0 ] && [ "$LANG_ONLY" -eq 0 ]; then
    phase "Phase 1: build runtime (release)"
    if (cd runtime && cargo build --release 2>&1 | tail -3); then
        ok "build"
    else
        fail "build"; failures+=("build")
    fi
    echo
fi

# ── Phase 2: cargo test ──────────────────────────────────────
if [ "$CONFORMANCE_ONLY" -eq 0 ] && [ "$LANG_ONLY" -eq 0 ]; then
    phase "Phase 2: cargo test --release (runtime/)"
    if (cd runtime && cargo test --release 2>&1 | tee /tmp/evident-cargo-test.log) ; then
        passed=$(grep "^test result" /tmp/evident-cargo-test.log \
                 | awk '{p+=$4} END {print p+0}')
        ok "cargo test: $passed passed"
    else
        fail "cargo test"; failures+=("cargo test")
    fi
    echo
fi

# ── Phase 3: conformance ─────────────────────────────────────
if [ "$RUST_ONLY" -eq 0 ] && [ "$LANG_ONLY" -eq 0 ]; then
    phase "Phase 3: conformance (tests/conformance/)"
    if pytest tests/conformance/ -q --tb=short 2>&1 | tee /tmp/evident-pytest.log ; then
        counts=$(grep -E "[0-9]+ passed" /tmp/evident-pytest.log | tail -1)
        ok "conformance: $counts"
    else
        fail "conformance"; failures+=("conformance")
    fi
    echo
fi

# ── Phase 4: lang tests ──────────────────────────────────────
if [ "$RUST_ONLY" -eq 0 ] && [ "$CONFORMANCE_ONLY" -eq 0 ]; then
    phase "Phase 4: lang_tests (tests/lang_tests/)"
    if python3 scripts/run-lang-tests.py 2>&1 | tee /tmp/evident-lang.log ; then
        ok "lang_tests"
    else
        fail "lang_tests"; failures+=("lang_tests")
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
