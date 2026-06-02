#!/usr/bin/env bash
# ./test.sh — run every test in this repo.
#
# Phases:
#   1. Build the Rust binary (release).
#   2. cargo test --release in runtime/.
#   3. pytest tests/conformance/ — black-box CLI conformance.
#   4. tests/lang_tests/*.ev — drive each via `evident sample --all --json`,
#      assert sat_*/unsat_* prefixes.
#   5. tests/kernel/*.ev — drive each via `evident emit` + `kernel`, assert
#      stdout + exit code match `-- expect:` header comments.
#
# Usage:
#   ./test.sh                   # all phases
#   ./test.sh --rust-only       # skip conformance + lang + kernel tests
#   ./test.sh --conformance     # only conformance
#   ./test.sh --lang            # only lang tests
#   ./test.sh --kernel          # only kernel tests

set -e -o pipefail
cd "$(dirname "$0")"

RUST_ONLY=0
CONFORMANCE_ONLY=0
LANG_ONLY=0
KERNEL_ONLY=0
for arg in "$@"; do
    case "$arg" in
        --rust-only)      RUST_ONLY=1 ;;
        --conformance)    CONFORMANCE_ONLY=1 ;;
        --lang)           LANG_ONLY=1 ;;
        --kernel)         KERNEL_ONLY=1 ;;
        -h|--help)
            sed -n '2,17p' "$0"; exit 0 ;;
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
if [ "$CONFORMANCE_ONLY" -eq 0 ] && [ "$LANG_ONLY" -eq 0 ] && [ "$KERNEL_ONLY" -eq 0 ]; then
    phase "Phase 1: build runtime + kernel (release)"
    if (cd runtime && cargo build --release 2>&1 | tail -3) \
       && (cd kernel && cargo build --release 2>&1 | tail -3); then
        ok "build"
    else
        fail "build"; failures+=("build")
    fi
    echo
fi

# ── Phase 2: cargo test ──────────────────────────────────────
if [ "$CONFORMANCE_ONLY" -eq 0 ] && [ "$LANG_ONLY" -eq 0 ] && [ "$KERNEL_ONLY" -eq 0 ]; then
    phase "Phase 2: cargo test --release (runtime/ + kernel/)"
    if (cd runtime && cargo test --release 2>&1 | tee /tmp/evident-cargo-test.log) \
       && (cd kernel  && cargo test --release 2>&1 | tee /tmp/evident-kernel-test.log) ; then
        passed_rt=$(grep "^test result" /tmp/evident-cargo-test.log  | awk '{p+=$4} END {print p+0}')
        passed_kn=$(grep "^test result" /tmp/evident-kernel-test.log | awk '{p+=$4} END {print p+0}')
        ok "cargo test: $passed_rt runtime + $passed_kn kernel"
    else
        fail "cargo test"; failures+=("cargo test")
    fi
    echo
fi

# ── Phase 3: conformance ─────────────────────────────────────
if [ "$RUST_ONLY" -eq 0 ] && [ "$LANG_ONLY" -eq 0 ] && [ "$KERNEL_ONLY" -eq 0 ]; then
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
if [ "$RUST_ONLY" -eq 0 ] && [ "$CONFORMANCE_ONLY" -eq 0 ] && [ "$KERNEL_ONLY" -eq 0 ]; then
    phase "Phase 4: lang_tests (tests/lang_tests/)"
    if python3 scripts/run-lang-tests.py 2>&1 | tee /tmp/evident-lang.log ; then
        ok "lang_tests"
    else
        fail "lang_tests"; failures+=("lang_tests")
    fi
    echo
fi

# ── Phase 5: kernel tests ────────────────────────────────────
if [ "$RUST_ONLY" -eq 0 ] && [ "$CONFORMANCE_ONLY" -eq 0 ] && [ "$LANG_ONLY" -eq 0 ]; then
    phase "Phase 5: kernel tests (tests/kernel/)"
    if python3 scripts/run-kernel-tests.py 2>&1 | tee /tmp/evident-kernel.log ; then
        ok "kernel_tests"
    else
        fail "kernel_tests"; failures+=("kernel_tests")
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
