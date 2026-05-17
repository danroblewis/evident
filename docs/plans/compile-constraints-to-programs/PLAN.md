# Plan: Compile Constraint Models to Programs

**Branch:** `feat/compile-constraints-to-programs`
**Target:** measurable performance improvements on Mario-class programs by
compiling constraint claims into native programs that meet their specs.
**Estimated duration:** 3-6 weeks of autonomous agent work.
**Disposition:** relentless. We will iterate, get rejected, try new angles,
and keep going. We commit to the *outcome* (real perf), not any particular
*technique*.

## What "win" looks like

Concrete success criteria, by descending priority:

1. **Mario per-tick FSM solve drops from ~25ms to ≤5ms** (5× speedup) on
   at least one FSM body, without regressions on the others. Bench:
   `EVIDENT_LOOP_TIMING=1 evident effect-run examples/test_21_mario/main.ev`.
2. **The dispatcher's self-hosted toposort path drops from 521ms to ≤10ms**
   (50×). Bench: `EVIDENT_TOPOSORT_IMPL=evident EVIDENT_DISPATCH_TIMING=1`.
3. **`EVIDENT_FUNCTIONIZE=1` becomes the default**, with the full test
   suite green and at least one real-program speedup demonstrated.

Anything that delivers (1) is enough to call this plan succeeded.
(2) and (3) are secondary acceptance criteria — won't block declaration
of success on (1), but they're additional wins to chase if the
opportunity arises.

## The strategic frame

Today the function-izer's gate accepts 27% of claims and those are
trivial (constructor types, constants). It does not fire on Mario.
We need to widen what's accepted while preserving soundness. The
expansion path is open-ended; many techniques could work. Some
candidates we haven't built yet:

- **AST interpreter expansion**: handle `∀ x ∈ range`, `seq[i]`, `#seq`,
  `match` over enums, `Field` access, `Ternary` (already partial).
- **Native Rust emit + libloading**: generate rustc-compiled .so per
  compiled chain. Order of magnitude faster than tree-walk eval.
- **JIT via Cranelift**: skip rustc, emit machine code directly.
- **E-graph normalization**: use the `egg` Rust crate to canonicalize
  formulas before matching patterns.
- **Z3 macro_finder + elim-predicates** in our tactic chain to catch
  quantified function definitions automatically.
- **Symbolic regression** for arithmetic-shaped residuals (PySR).
- **Partial evaluation / Futamura projection** of our claim interpreter
  against specific claim bodies.
- **Constraint hoisting**: move tick-invariant computation out of the
  per-tick solve into a one-shot compile-time pass.
- **Differential / incremental Z3** with push/pop per tick instead of
  fresh solver each call.
- **Algebra extraction**: use Gröbner bases (msolve) or HNF/SNF (flint)
  to recover linear/polynomial functional dependencies that solve-eqs
  doesn't catch.
- **Compile-on-second-use**: track call counts; compile when a (claim,
  given-shape) is hit ≥ N times.
- **Compile the verifier, not the solver**: for any compiled fast path,
  generate a Z3 verifier that confirms it matches the spec on every
  input — adds confidence to expanded gate.

Plenty of room. We'll explore these systematically.

## The Loop

Each round is six phases. Rounds run sequentially; phases within a
round may run agents in parallel.

```
┌─────────────────────────────────────────────────────────────┐
│ Round N                                                      │
│                                                              │
│  Phase 1: IDEATE                                             │
│    Multiple idea-generator agents propose techniques and     │
│    tactics, with diverse vocabulary (one per agent).         │
│                                                              │
│  Phase 2: RESEARCH                                           │
│    Researcher agents investigate the top 3-4 ideas in        │
│    parallel. Each returns: feasibility, citations, scope,    │
│    estimated effort, expected payoff.                        │
│                                                              │
│  Phase 3: PLAN                                               │
│    Synthesize research into a concrete implementable         │
│    intervention. ONE technique to build this round.         │
│    Write a sub-plan with clear acceptance test.              │
│                                                              │
│  Phase 4: BUILD                                              │
│    Implementor agent (or main loop) writes the code:         │
│    new module(s), tests, benches. Commits to the branch.     │
│                                                              │
│  Phase 5: CRITIQUE                                           │
│    Critic agent reviews the build for correctness gaps,      │
│    soundness violations, scope creep, missing tests.         │
│    Verifier runs the full suite + new bench.                 │
│                                                              │
│  Phase 6: REVIEW                                             │
│    Decide based on data:                                     │
│      - PASS: real perf win. Commit + push. Next round picks  │
│        up a new technique to compound or expand coverage.    │
│      - PARTIAL: works but doesn't move the needle on real    │
│        programs. Keep the code, route around the limit       │
│        next round.                                           │
│      - FAIL: bug, soundness issue, or no perf gain.          │
│        Revert if needed; iterate or pivot.                   │
│      - DEAD END: technique fundamentally won't pay off.      │
│        Record why, pick a different idea next round.         │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

A round produces:
- A new module or feature in the branch
- A bench file demonstrating the technique
- A doc entry in this plan directory recording the round's outcome

## Discipline

- **Bench before declaring done.** Every round MUST include a bench
  that compares before vs after on a real workload (not synthetic).
  No more "242× on Pair" — Pair isn't real.
- **Soundness gates stay tight.** When in doubt, refuse and fall through
  to Z3. A correctness regression undoes all the work. The function-izer's
  `is_pure_assignment_body` is a model: conservative-by-default, expanded
  case by case with explicit soundness justifications.
- **Each round commits.** No long-lived uncommitted state. Failed rounds
  commit a "Round N — dead end" note in the plan directory.
- **Reuse what's there.** We have decomposition, classification, chain
  extraction, the cache. Don't duplicate. Extend.

## Round catalog

This is the running log. Each round writes its own file. The plan
adapts as evidence comes in.

```
Round 1: docs/plans/compile-constraints-to-programs/round-01-*.md
Round 2: docs/plans/compile-constraints-to-programs/round-02-*.md
...
```

Each round file's name includes the technique chosen (e.g.
`round-02-ast-interpreter-expansion.md`).

## Round 1 — entry point

The first round's goal: **survey what techniques to try, in what
order, with what expected payoff.** Output a ranking. This sets up
the next 3-4 rounds.

Concrete steps for Round 1:
1. Launch 4 parallel idea-generator agents with diverse prompts.
2. Wait for results. Synthesize into a ranked list.
3. Pick the top technique by (expected payoff × feasibility) /
   build cost.
4. Write `round-01-survey.md` recording the rankings and rationale.
5. Round 2 starts with that technique.

The remainder of this plan is written as we go.
