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
#
# Optional phase (NOT run by default):
#   --examples                  Run every examples/test_*.ev via the
#                               binary, end-to-end. Visual demos (SDL)
#                               render on the Xvfb display ($DISPLAY)
#                               and are screenshotted into
#                               /tmp/evident-screenshots/ for eyes-on
#                               review.
#
# This is THE test command. Any time an agent finishes a chunk of
# work that touches code or stdlib or examples/, run this before
# declaring done.
#
# Usage:
#   ./test.sh                   # phases 1-2 (default)
#   ./test.sh --examples        # phases 1-2 PLUS the examples runner
#   ./test.sh --examples-only   # only the examples runner
#                               # (assumes binary already built)

set -e -o pipefail

cd "$(dirname "$0")"

EXAMPLES=0
EXAMPLES_ONLY=0
for arg in "$@"; do
    case "$arg" in
        --examples)       EXAMPLES=1 ;;
        --examples-only)  EXAMPLES=1 ; EXAMPLES_ONLY=1 ;;
        -h|--help)
            sed -n '2,28p' "$0"
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
if [ "$EXAMPLES_ONLY" -eq 0 ]; then
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
if [ "$EXAMPLES_ONLY" -eq 0 ]; then
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

# ── Phase 2.5: lint ratchet (Python code-quality guardrail) ───
# Fails ONLY when ide/ + viz/ pick up NEW violations over ide/.lint-baseline
# (file/function length, free-function count, coupling). Existing debt is
# grandfathered; this just stops it growing. `ide/lint.py --write-baseline`
# after an intentional refactor that legitimately changes the counts.
if [ "$EXAMPLES_ONLY" -eq 0 ] && command -v python3 >/dev/null 2>&1; then
    phase "Phase 2.5: lint ratchet (ide/ + viz/)"
    if python3 ide/lint.py --ratchet; then
        ok "lint"
    else
        fail "lint: new code-quality violations introduced (see above)"
        failures+=("lint")
    fi
    echo
fi

# ── Phase 2.6: render smoke-test (the viz/ Python renderers) ───
# The Rust tests + demos do NOT exercise the IDE's view renderers — a refactor that breaks a
# renderer is otherwise caught only by manually driving the browser. This renders EVERY view on a
# couple of scalar/enum samples headlessly and fails if any errors or produces no PNG.
if [ "$EXAMPLES_ONLY" -eq 0 ] && command -v python3 >/dev/null 2>&1; then
    phase "Phase 2.6: render smoke-test (viz/ renderers)"
    if python3 ide/render_smoke.py > /tmp/evident-render-smoke.log 2>&1; then
        ok "render smoke-test ($(tail -1 /tmp/evident-render-smoke.log))"
    else
        fail "render smoke-test: a renderer errored or produced no PNG"
        cat /tmp/evident-render-smoke.log >&2
        failures+=("render smoke-test")
    fi
    echo
fi

# ── Phase 2.7: BMC completeness-certification test (Ana #270) ──
# The unroll export prepends a COMPLETE-vs-BOUNDED verdict driven by the reachable set's closing
# depth. This pins the right verdict for terminating (counter), cyclic (traffic), unbounded, and
# real-valued models — so a regression can never silently claim "complete" for a capped/real model.
if [ "$EXAMPLES_ONLY" -eq 0 ] && command -v python3 >/dev/null 2>&1; then
    phase "Phase 2.7: BMC completeness certification"
    if python3 ide/test_completeness.py > /tmp/evident-completeness.log 2>&1; then
        ok "completeness ($(tail -1 /tmp/evident-completeness.log))"
    else
        fail "completeness: wrong COMPLETE/BOUNDED verdict (see above)"
        cat /tmp/evident-completeness.log >&2
        failures+=("completeness certification")
    fi
    echo
fi

# ── Phase 2.8: liveness-under-fairness verdicts (Ana #269) ────
# The temporal check's `fair` mode excludes UNFAIR lassos: □◇/◇/⤳ hold iff the goal is reachable
# from every reachable (P-)state. This pins both directions — a dodger model where fairness flips
# REFUTED→HOLDS (no trap), and a TRAP model that FAILS even under fairness with the trap + its run —
# so a regression can't silently re-refute on unfair runs or claim HOLDS over a real trap.
if [ "$EXAMPLES_ONLY" -eq 0 ] && command -v python3 >/dev/null 2>&1; then
    phase "Phase 2.8: liveness under fairness"
    if python3 ide/test_fairness.py > /tmp/evident-fairness.log 2>&1; then
        ok "fairness ($(tail -1 /tmp/evident-fairness.log))"
    else
        fail "fairness: wrong under-fairness verdict (see above)"
        cat /tmp/evident-fairness.log >&2
        failures+=("fairness liveness")
    fi
    echo
fi

# ── Phase 2.9: value-symmetry witness fold (Ana #271) ────────
# The enumerate fold collapses witnesses that differ only by permuting INTERCHANGEABLE enum values
# to one canonical rep + a count. Soundness is the whole bar: this pins BOTH directions — a colouring
# whose colours are PROVABLY interchangeable folds into S_3 orbits ×6 (each rep a real witness), and
# models that NAME a value or ORDER an enum stay UNFOLDED — so a regression can't silently over-claim.
if [ "$EXAMPLES_ONLY" -eq 0 ] && command -v python3 >/dev/null 2>&1; then
    phase "Phase 2.9: value-symmetry witness fold"
    if python3 ide/test_symmetry.py > /tmp/evident-symmetry.log 2>&1; then
        ok "symmetry ($(tail -1 /tmp/evident-symmetry.log))"
    else
        fail "symmetry: wrong fold verdict (see above)"
        cat /tmp/evident-symmetry.log >&2
        failures+=("symmetry fold")
    fi
    echo
fi

# ── Phase 2.10: all-initial-conditions transition graph (diagram #1) ──
# full_state_graph enumerates EVERY valid carried assignment (the bounded discrete product,
# ignoring the seed) and applies the EXISTING successor relation — the GLOBAL dynamics, not the
# forward orbit of one init. Pins the headline win (a deterministic bistable's all-conditions graph
# ⊋ its from-init graph, surfacing BOTH basins) and the honesty fallback (a real-valued model is NOT
# enumerated → discrete=False, caller falls back to from-init).
if [ "$EXAMPLES_ONLY" -eq 0 ] && command -v python3 >/dev/null 2>&1; then
    phase "Phase 2.10: all-initial-conditions graph"
    if python3 ide/test_all_conditions.py > /tmp/evident-allcond.log 2>&1; then
        ok "all-conditions ($(tail -1 /tmp/evident-allcond.log))"
    else
        fail "all-conditions: wrong global-dynamics graph (see above)"
        cat /tmp/evident-allcond.log >&2
        failures+=("all-conditions graph")
    fi
    echo
fi

# ── Phase 2.11: multiple fsms / claims — last-defined entry (#290) ──
# A program may declare several fsms AND several claims; export renders the LAST-DEFINED fsm-or-claim
# in source order, and the entry picker overrides with an explicit name. Pins the six routing cases
# (claim-then-fsm, fsm-then-claim, two-fsms, two-claims, single-fsm, single-claim) + the override +
# a bogus-entry error — so a regression can't re-introduce the old "exactly one" hard-error or pick
# the wrong entry.
if [ "$EXAMPLES_ONLY" -eq 0 ] && command -v python3 >/dev/null 2>&1; then
    phase "Phase 2.11: multi-entry (last-defined) routing"
    if python3 ide/test_multi_entry.py > /tmp/evident-multientry.log 2>&1; then
        ok "multi-entry ($(tail -1 /tmp/evident-multientry.log))"
    else
        fail "multi-entry: wrong entry rendered (see above)"
        cat /tmp/evident-multientry.log >&2
        failures+=("multi-entry routing")
    fi
    echo
fi

# ── Phase 2.12: claim_space categorical feasibility grid (#136) ──
# claim_space used to DEGRADE to a bare N/A card ("no numeric variable to bound") for exactly the
# Seq(Int)/enum claims a verification engineer most wants to see — N-queens, sudoku, graph-coloring,
# toposort. Now each gets a per-POSITION feasibility grid (z3 SAT of `body ∧ seq[i] == v` per cell).
# This pins BOTH directions: a Seq/enum claim yields a REAL grid (N-queens has infeasible corner
# cells — information, not a flat sheet), and a scalar-only claim stays an honest N/A (None, no
# fabricated grid).
if [ "$EXAMPLES_ONLY" -eq 0 ] && command -v python3 >/dev/null 2>&1; then
    phase "Phase 2.12: claim_space categorical grid"
    if python3 ide/test_claim_space.py > /tmp/evident-claimspace.log 2>&1; then
        ok "claim-space grid ($(tail -1 /tmp/evident-claimspace.log))"
    else
        fail "claim-space grid: Seq/enum claim fell back to N/A, or a scalar claim fabricated a grid"
        cat /tmp/evident-claimspace.log >&2
        failures+=("claim-space grid")
    fi
    echo
fi

# ── Phase 2.13: parameterized passthrough `..Name(field ↦ other)` (#294) ──
# The parser used to ERROR on `..Name(...)`. Now it parses as a rename-arg list, and the
# included claim's un-renamed carried vars are freshened per instance so two passthroughs of
# the SAME claim with DIFFERENT renames compose into INDEPENDENT sub-systems. This pins both
# halves: the parameterized-passthrough fsm exports with 0 dropped constraints (two-instance,
# single, and bare forms), and the runtime sat/unsat claims prove the compose semantics —
# both walks seed independently, each respects its own ±1 bound, and the two can step in
# opposite directions on one tick (only possible if `da` is freshened, not shared).
if [ "$EXAMPLES_ONLY" -eq 0 ] && command -v python3 >/dev/null 2>&1; then
    phase "Phase 2.13: parameterized passthrough compose"
    if python3 ide/test_passthrough_params.py > /tmp/evident-passthrough.log 2>&1; then
        ok "parameterized passthrough ($(tail -1 /tmp/evident-passthrough.log))"
    else
        fail "parameterized passthrough: parse/compose regressed (drop or wrong semantics)"
        cat /tmp/evident-passthrough.log >&2
        failures+=("parameterized passthrough")
    fi
    echo
fi

# ── Phase 2.14: reachability_tree rooted from ALL initial conditions (diagram review) ──
# The reachability tree used to root from initial_state() (one seed) and stop at a hard
# MAX_DEPTH=8 cap. The diagram review scored that NO: root from the SET of initial conditions
# (a forest off a synthetic ∅ root) and use closing_depth() to show the tree CLOSING at its
# true saturation depth for finite discrete systems — not a misleading depth-8 cap. Pins the
# all-conditions forest (traffic closes cyclically; a terminating counter closes at its true
# depth with the fixed point marked absorbing; a free-init FSM fans to MANY roots) AND the
# real/unbounded honesty fallback (no false 'complete', single-seed depth-capped sample).
if [ "$EXAMPLES_ONLY" -eq 0 ] && command -v python3 >/dev/null 2>&1; then
    phase "Phase 2.14: reachability_tree from all initial conditions"
    if python3 ide/test_reachability_tree.py > /tmp/evident-reachtree.log 2>&1; then
        ok "reachability-tree ($(tail -1 /tmp/evident-reachtree.log))"
    else
        fail "reachability-tree: did not root from all inits / didn't close / false fallback"
        cat /tmp/evident-reachtree.log >&2
        failures+=("reachability-tree rooting")
    fi
    echo
fi

# ── Phase 2.15: transition_matrix over all initial conditions (diagram review) ──
# The transition_matrix is a state×state incidence (cell (i,j) lit ⇔ state_i → state_j). The
# diagram review scored it PARTIAL: it rooted on ONE z3 model's from-init orbit and, in SAMPLED
# mode, fabricated a linspace grid. It now roots the discrete path on full_state_graph (EVERY valid
# carried assignment) and fills the cells from the REAL transition edges. Pins: counter (6) + traffic
# (9) take the global root (mode='all initial conditions'), state set == full_state_graph (⊇ from-init),
# lit cells == the true successor relation (no sampled grid); a real-valued model falls back honestly.
if [ "$EXAMPLES_ONLY" -eq 0 ] && command -v python3 >/dev/null 2>&1; then
    phase "Phase 2.15: transition_matrix all-conditions root"
    if python3 ide/test_transition_matrix_global.py > /tmp/evident-tmxglobal.log 2>&1; then
        ok "transition-matrix global ($(tail -1 /tmp/evident-tmxglobal.log))"
    else
        fail "transition-matrix: not rooted on the global graph, or cells aren't the real transition"
        cat /tmp/evident-tmxglobal.log >&2
        failures+=("transition-matrix global root")
    fi
    echo
fi

# ── Phase 2.16: all_conditions analyze STATS/BANNER follow the toggle (#316) ──
# The all_conditions toggle re-roots the state_graph PNG on the GLOBAL graph (full_state_graph,
# every initial condition). This pins that the analyze STATS + banner follow the SAME graph: the
# bistable flips 2 (from-init) → 7 (global) with the flag; the banner says "over all initial
# conditions" only when on; non-state_graph views are unaffected; a real-valued model falls back.
if [ "$EXAMPLES_ONLY" -eq 0 ] && command -v python3 >/dev/null 2>&1; then
    phase "Phase 2.16: all_conditions stats/banner toggle"
    if python3 ide/test_all_conditions_stats.py > /tmp/evident-allcond-stats.log 2>&1; then
        ok "all-conditions stats ($(tail -1 /tmp/evident-allcond-stats.log))"
    else
        fail "all-conditions stats: banner/stats did not follow the global graph (see above)"
        cat /tmp/evident-allcond-stats.log >&2
        failures+=("all-conditions stats")
    fi
    echo
fi

# ── Phase 2.20: scatter_matrix over a CLAIM/Solve solution space (diagram review #5) ──
# scatter_matrix used to handle only FSM carried-state; a pure claim/Solve program (free decision
# vars, no transition) KeyError'd or rendered empty. This pins the new claim path: a 2-Int claim's
# scatter matrix samples its SOLUTION SPACE (distinct block-and-resolve z3 witnesses, every one
# satisfying the constraint), and UNSAT / categorical-only claims fall to the honest empty card.
if [ "$EXAMPLES_ONLY" -eq 0 ] && command -v python3 >/dev/null 2>&1; then
    phase "Phase 2.20: scatter_matrix claim/Solve solution space"
    if python3 ide/test_scatter_claim.py > /tmp/evident-scatter-claim.log 2>&1; then
        ok "scatter_matrix claim ($(tail -1 /tmp/evident-scatter-claim.log))"
    else
        fail "scatter_matrix claim: solution-space sampling did not render (see above)"
        cat /tmp/evident-scatter-claim.log >&2
        failures+=("scatter_matrix claim")
    fi
    echo
fi

# ── Optional: examples runner ────────────────────────────────
# Walks examples/, runs each via effect-run. For visual demos (anything
# that imports packages/sdl/), spawn the program, screenshot the Xvfb
# display after a brief wait, kill, save the PNG. Doesn't fail the run on
# visual issues — those need eyes-on review (a human or an LLM that Reads
# the captured PNGs).
if [ "$EXAMPLES" -eq 1 ]; then
    phase "Phase 3: examples runner (examples/)"

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
                # Linux: capture the Xvfb root window via imagemagick.
                # macOS: screencapture (no X server, so `import` can't grab).
                # Best-effort either way — `|| true` so a failed grab never
                # aborts the runner under `set -e`.
                if [ "$(uname)" = "Darwin" ] && command -v screencapture >/dev/null 2>&1; then
                    screencapture -x "$SHOTDIR/$name.png" 2>/dev/null || true
                elif command -v import >/dev/null 2>&1; then
                    import -display "${DISPLAY:-:99}" -window root "$SHOTDIR/$name.png" 2>/dev/null || true
                elif command -v screencapture >/dev/null 2>&1; then
                    screencapture -x "$SHOTDIR/$name.png" 2>/dev/null || true
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
                    note "$name (visual; no screenshot tool found)"
                fi
            else
                # Non-visual: run with a short timeout, check exit.
                # Demos that need stdin (test_14_stdin) get an empty
                # stdin and short max-steps so they don't hang.
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
