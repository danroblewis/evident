# Task: Explore pin-application mechanisms; add pre-loop `.simplify()`

## Authorisation

**Another rare exception to the `kernel/` freeze.** The user
explicitly authorised additional kernel work on the pin-application
mechanism. Quote:

> *"The issue was that the agent tried to implement something related
> to pushing and popping in Rust on the Z3 model, and it didn't try the
> `.check(*pins)` thing. We should `.simplify()` the model before we
> start the loop. We can use the other solver to apply the pins, there
> are several ways to apply pins to a Z3 model. We should test all of
> them. If the performance is bad, that's okay, as long as it works
> and is correct."*

Your edits are limited to `kernel/src/tick.rs` and the two doc files
explicitly mentioned at the end of this spec. Do NOT take this as
license to do other kernel work, refactors, or "cleanups."

## Why this exists

The previous kernel-fix (commit `d11eaa9`) implemented
"parse-body-once, cached ASTs, fresh solver per tick" because the
literal push/pop form regressed 36× on `test_consolidated_lexer`
(growing datatype-state pins). What was NOT tried:

- `solver.check_assumptions(&pins)` / `check_with_assumptions` — the
  Z3 API tiny-runtime's Python code used as `s.check(*pins)`.
  Assumptions are added to a single check call and do not persist
  on the solver, so the per-check overhead may differ from push/pop.
- `.simplify()` on the body ONCE before entering the tick loop.
  This was wrongly forbidden by an over-strict reading of invariant
  #4. The invariant has been clarified (commit on `main` with this
  task spec): pre-loop simplify is allowed and desired.
- Combinations: simplify-then-push/pop, simplify-then-check-with-
  assumptions, simplify-then-cached-ASTs (the current form), etc.

This session explores those alternatives, benchmarks them, picks
the best correct one, and documents the rest as fallbacks.

## Required reading

1. `CLAUDE.md`.
2. `docs/plans/architecture-invariants.md` — note the clarified
   invariant #4: pre-loop simplify is OK.
3. `docs/plans/audit-kernel-z3-lifecycle.md` — the audit findings.
4. `docs/plans/kernel-fix-incremental-solving.md` — what's
   currently in `tick.rs` and why the literal push/pop was
   abandoned.
5. `kernel/src/tick.rs` — the current implementation.
6. `legacy-python/docs/runtime-architecture.md` — tiny-runtime's
   `check(*pins)` design rationale.

Cite at minimum #2, #4, and #6 in your report.

## What you're exploring

For each of the mechanisms below, implement it on a scratch branch,
measure perf, then either commit to `tick.rs` (the winner) or
discard. Mechanisms to try, ALL with `.simplify()` run once before
the tick loop:

| # | Mechanism | What it does |
|---|---|---|
| A | **simplify-then-cached-ASTs** (current + simplify) | Add `.simplify()` before the loop; keep current cached-ASTs + fresh-solver-per-tick. Baseline for "does pre-loop simplify alone help?" |
| B | **simplify + check-with-assumptions** | Persistent solver after `.simplify()`. Per tick: build `Z3_ast` array of pinning equalities, call `Z3_solver_check_assumptions`. This is tiny-runtime's pattern. |
| C | **simplify + push/pop** | Persistent solver. Per tick: `push`, assert pins, check, pop. (The form that regressed 36× — re-measure WITH pre-loop simplify; the slowdown may have been driven by un-simplified body.) |
| D | **simplify + reset + assert + check** | Persistent solver. Per tick: `reset` (drop assertions), re-assert simplified-body, assert pins, check. The "naive but explicit" form. |
| E | **Substitute pins directly into body AST** | Use Z3's substitution API (`Z3_substitute_vars` or similar) to inline pinning equalities into the simplified body before each check. Solver is fresh per tick; no incremental state. |
| F | **Different solver tactic** | Try `Solver::new_for_logic(ctx, "QF_S")` or `mk_solver_from_tactic` with a tactic mix that's more datatype-friendly. The "other solver" the user mentioned. |

You may also try combinations (e.g. "fresh solver per tick BUT with
the simplified body assertion vector"), or other Z3 APIs that
qualify as "applying pins to a Z3 model." Document everything you
tried; don't just stop at the first thing that works.

## Benchmarks

For each mechanism, measure wall-clock on at least 3 fixtures
representing different state-shape categories:

1. `tests/kernel/test_consolidated_lexer.ev` (13 ticks; growing
   datatype state — this is the one that timed out with literal
   push/pop).
2. `tests/kernel/test_fti_stack.ev` (7 ticks; mixed Int + cons-list
   state).
3. A short Int-only fixture, e.g. `tests/kernel/test_hello.ev` or
   `test_tokens_carry.ev` (use the existing test_tokens_carry or
   similar primitive-state fixture).

Median of 10 runs each, in milliseconds. Report a table. Bigger
fixtures are better signal but tiny ones catch overhead.

## Selection criteria

Pick the mechanism that:

1. **Is correct.** All 63 kernel tests pass + the 16 conformance
   features pass + `./test.sh` is fully green. This is the hard
   gate.
2. **Is closest to the user's design intent.** tiny-runtime's
   `check(*pins)` is the canonical form; preferences in descending
   order: check-with-assumptions (B), push/pop (C), substitution
   (E), reset (D), cached-ASTs (A or current), tactic variant (F).
3. **Is performant.** "Performance is bad is OK as long as it works
   and is correct" — perf is the tiebreaker, not a gate. But if
   one mechanism is 100× slower than another while both are
   correct, prefer the faster one.

If MULTIPLE are correct and within 2× of each other on perf, prefer
the one closer to tiny-runtime's design.

## If all options are too slow

"Too slow" means `./test.sh` times out (single test taking >30s),
not just "slower than current." If all of A-F are too slow to be
acceptable, do NOT pick the current cached-ASTs form by default
without documenting why. Instead:

- Write `docs/plans/blocked-pin-exploration.md` describing what
  you tried, the benchmark numbers, and which "functionizer" code
  in `bootstrap/runtime/src/` might be useful as the next move
  (the user mentioned bringing back functionizer code as the
  fallback).
- Do NOT implement functionizer in this task — that's a separate
  follow-up.
- Leave the current `tick.rs` in place; do not regress it.

## Acceptance

Same as task #06 plus:

1. `kernel/src/tick.rs` either contains the winning mechanism (if
   different from current) OR is unchanged + you've documented why
   (if current cached-ASTs remained the best).
2. `kernel/src/tick.rs` calls `.simplify()` on the body once before
   entering the tick loop (regardless of which pin mechanism wins).
3. `./test.sh` is fully green.
4. Benchmark table for all mechanisms in your final report.
5. Diff limited to:
   - `kernel/src/tick.rs` (modified)
   - `docs/plans/kernel-fix-incremental-solving.md` (LANDED section
     updated to reflect the exploration outcome)
   - `docs/plans/architecture-invariants.md` (if anything needs
     clarification)
   - Possibly `docs/plans/blocked-pin-exploration.md` if you
     genuinely couldn't pick a correct mechanism.

## Forbidden

- Editing any kernel file OTHER than `kernel/src/tick.rs`.
- Editing `bootstrap/`, `compiler/`, `stdlib/`, or anything outside
  the explicitly listed paths.
- Adding crate dependencies.
- Implementing functionizer code (that's a separate follow-up).
- Calling `.simplify()` INSIDE the tick loop. Only once, before.
- Adding `.simplify()` in places other than the pre-tick-loop
  setup.

## Reporting back

Final message:

- Branch pushed (`agent-11-kernel-pin-exploration` or similar).
- Table: mechanism (A–F + any extras) × fixture × median ms.
- Which mechanism you picked + why (one paragraph).
- Diff stat (only `tick.rs` + documented docs should appear).
- `./test.sh` final line.
- Cite docs/plans/audit-kernel-z3-lifecycle.md,
  docs/plans/architecture-invariants.md, and
  legacy-python/docs/runtime-architecture.md as a minimum.

Do NOT paste full code; the coordinator reads files.
