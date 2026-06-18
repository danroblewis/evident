#!/usr/bin/env bash
# ./test.sh — run every test in this repo.
#
# Phases (each phase fails the run if it fails, except --examples
# which is informational):
#   1. Build the Rust binary (release).
#   2. cargo test --release in runtime/ — Rust units + integration
#      tests. Includes the multi-FSM scheduler tests and the demo
#      driver (which runs every examples/test_*.ev that has
#      an EXPECTATIONS row).
#   3. pytest tests/conformance/ — black-box CLI conformance tests.
#
# Optional phase (NOT run by default):
#   --examples                  Run every examples/test_*.ev
#                               via the binary, end-to-end. For
#                               visual demos (SDL etc.), capture
#                               screenshots into
#                               /tmp/evident-screenshots/. Use this
#                               when you want eyes-on confirmation
#                               that the visible demos still render.
#                               (Visual demos are NOT in cargo
#                               test's expectations because they
#                               need a display.)
#
# This is THE test command. Any time an agent finishes a chunk of
# work that touches code or stdlib or examples/, run this before
# declaring done.
#
# Usage:
#   ./test.sh                   # phases 1-3 (default)
#   ./test.sh --rust-only       # phases 1-2 (skip conformance)
#   ./test.sh --conformance     # only conformance
#   ./test.sh --examples        # phases 0-3 PLUS the examples
#                               # runner with screenshots
#   ./test.sh --examples-only   # only examples runner
#                               # (assumes binary already built)

set -e -o pipefail

cd "$(dirname "$0")"

RUST_ONLY=0
CONFORMANCE_ONLY=0
EXAMPLES=0
EXAMPLES_ONLY=0
for arg in "$@"; do
    case "$arg" in
        --rust-only)      RUST_ONLY=1 ;;
        --conformance)    CONFORMANCE_ONLY=1 ;;
        --examples)       EXAMPLES=1 ;;
        --examples-only)  EXAMPLES=1 ; EXAMPLES_ONLY=1 ;;
        -h|--help)
            sed -n '2,38p' "$0"
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
    YELLOW=$(printf '\033[0;33m')
    DIM=$(printf '\033[2m')
    OFF=$(printf '\033[0m')
else
    BOLD=''; GREEN=''; RED=''; YELLOW=''; DIM=''; OFF=''
fi

phase() { echo "${BOLD}── $1 ──${OFF}"; }
ok()    { echo "${GREEN}✓${OFF} $1"; }
fail()  { echo "${RED}✗${OFF} $1" >&2; }
note()  { echo "${YELLOW}!${OFF} $1"; }

started=$(date +%s)
failures=()

# ── Phase 1: build ────────────────────────────────────────────
if [ "$CONFORMANCE_ONLY" -eq 0 ] && [ "$EXAMPLES_ONLY" -eq 0 ]; then
    phase "Phase 1: build runtime (release)"
    if (cd runtime && cargo build --release 2>&1 | tail -3); then
        ok "build"
    else
        fail "build"
        failures+=("build")
    fi
    echo
fi

# ── Phase 2: cargo test ───────────────────────────────────────
if [ "$CONFORMANCE_ONLY" -eq 0 ] && [ "$EXAMPLES_ONLY" -eq 0 ]; then
    phase "Phase 2: cargo test --release (runtime/)"
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

# ── Phase 3: conformance ──────────────────────────────────────
if [ "$RUST_ONLY" -eq 0 ] && [ "$EXAMPLES_ONLY" -eq 0 ]; then
    phase "Phase 3: conformance (tests/conformance/)"
    if ! command -v pytest >/dev/null 2>&1; then
        fail "pytest not found in PATH; install it or run inside a venv"
        failures+=("conformance: pytest missing")
    else
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

# ── Optional: examples runner ────────────────────────────────
# Walks examples/, runs each via effect-run. For visual
# demos (anything that imports packages/sdl/), spawn the program,
# screenshot after a brief wait, kill, save the PNG. Doesn't
# fail the run on visual issues — those need eyes-on review by
# either a human or by an LLM that Reads the captured PNGs.
if [ "$EXAMPLES" -eq 1 ]; then
    phase "Phase 4: examples runner (examples/)"

    EVIDENT="$PWD/runtime/target/release/evident"
    if [ ! -x "$EVIDENT" ]; then
        fail "binary not built at $EVIDENT — run without --examples-only first"
        failures+=("examples: binary missing")
    else
        SHOTDIR="/tmp/evident-screenshots"
        rm -rf "$SHOTDIR"
        mkdir -p "$SHOTDIR"
        echo "${DIM}screenshots → $SHOTDIR${OFF}"

        examples_total=0
        examples_ok=0
        examples_visual=0
        examples_failed=()

        # Match flat single-file demos AND directory-based ones with a main.ev.
        files=( examples/test_*.ev examples/test_*/main.ev )
        for f in "${files[@]}"; do
            [ -e "$f" ] || continue
            if [ "$(basename "$f")" = "main.ev" ]; then
                name=$(basename "$(dirname "$f")")
            else
                name=$(basename "$f" .ev)
            fi
            examples_total=$((examples_total + 1))

            # Visual demo? Check for SDL imports.
            if grep -q "packages/sdl" "$f"; then
                examples_visual=$((examples_visual + 1))
                # Run in background, screenshot, kill.
                "$EVIDENT" effect-run "$f" --max-steps 80 \
                    >/tmp/evident-example.out 2>/tmp/evident-example.err &
                pid=$!
                sleep 2
                # macOS screencapture; on Linux you'd swap this for
                # `import` or `gnome-screenshot`. Best-effort either way.
                if command -v screencapture >/dev/null 2>&1; then
                    screencapture -x "$SHOTDIR/$name.png" 2>/dev/null
                fi
                # Wait for natural exit (capped) or kill.
                for _ in 1 2 3; do
                    if ! kill -0 $pid 2>/dev/null; then break; fi
                    sleep 1
                done
                kill $pid 2>/dev/null || true
                wait $pid 2>/dev/null || true
                if [ -f "$SHOTDIR/$name.png" ]; then
                    ok "$name (visual; screenshot saved)"
                else
                    note "$name (visual; no screencapture tool found)"
                fi
            else
                # Non-visual: run with a short timeout, check exit.
                # Demos that need stdin (test_14_stdin) get an empty
                # stdin and short max-steps so they don't hang.
                # `timeout` lives in /opt/homebrew/bin on macOS,
                # /usr/bin on most Linux. Fall back to "run without
                # timeout" if neither's around (fail-open: better
                # to hang than to silently skip).
                TO=$(command -v timeout || command -v gtimeout || echo "")
                if [ -n "$TO" ]; then RUN="$TO 8 $EVIDENT"; else RUN="$EVIDENT"; fi
                if $RUN effect-run "$f" --max-steps 60 \
                        </dev/null >/tmp/evident-example.out 2>&1 ; then
                    examples_ok=$((examples_ok + 1))
                    ok "$name"
                else
                    ec=$?
                    # Some demos exit non-zero deliberately (test_08_exit_code → 42).
                    case "$name" in
                        test_08_exit_code)
                            if [ $ec -eq 42 ]; then
                                examples_ok=$((examples_ok + 1))
                                ok "$name (exit 42 expected)"
                            else
                                fail "$name (exit $ec, expected 42)"
                                examples_failed+=("$name")
                            fi
                            ;;
                        test_14_stdin|test_15_signal)
                            # These need real stdin / SIGINT to be useful.
                            note "$name (skipped: needs interactive input)"
                            examples_ok=$((examples_ok + 1))
                            ;;
                        *)
                            fail "$name (exit $ec)"
                            examples_failed+=("$name")
                            ;;
                    esac
                fi
            fi
        done

        echo
        ok "examples: $examples_ok/$examples_total ran cleanly, $examples_visual visual"
        if [ ${#examples_failed[@]} -gt 0 ]; then
            fail "examples: ${examples_failed[*]}"
            failures+=("examples")
        fi
        if [ $examples_visual -gt 0 ]; then
            note "Visual demos captured. Review the PNGs in $SHOTDIR — "
            note "an agent should Read each and verify it matches the demo's"
            note "docstring (red window for sdl_red, RGB triangle for triangle)."
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
