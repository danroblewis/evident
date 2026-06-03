# Task: compiler.ev perf diagnostic — measure before refactoring

## Why

Wave 4d landed the prelude and the full self-hosted round trip
works on minimal programs (`kernel + compiler.smt2` compiles a
small `.ev`, kernel runs the output). The full `test_hello` smoke
test times out: ~40-60s/lex-tick × ~thousands of ticks on the
4137-char flattened input.

Before refactoring `compiler.ev` for perf, **measure what's
actually slow.** We built the functionizer diagnostics
(`EVIDENT_FUNCTIONIZE_STATS=verbose`) specifically to answer
"which step shapes are escaping into Z3 vs functionizing." This
task runs those diagnostics on `compiler.smt2` and produces a
report that informs the actual refactor.

This is a **diagnostic-only** session. No code changes to
`compiler/`. Output is one report doc.

## Authorisation

You may add docs and small test fixtures, but do NOT modify any
`compiler/*.ev`, `kernel/`, `bootstrap/`, or `stdlib/` source. The
goal is measurement, not refactoring.

## Required reading

1. `CLAUDE.md` §"Functionizer diagnostics (on by default)".
2. `docs/plans/grammar-wave4d.md` — what just landed.
3. `docs/plans/blocked-grammar-wave4d.md` — the perf blocker spec.
4. `kernel/src/functionize/mod.rs` §"Diagnostics" — the
   `EVIDENT_FUNCTIONIZE_STATS=verbose` shape-category labels.
5. `scripts/build-compiler-smt2.sh` — to (re)build compiler.smt2
   from the current compiler.ev.
6. `tests/kernel/test_hello.ev` — the smoke-test target (4137
   chars after flatten).
7. `compiler/compiler.ev` (read-only) — to map step labels back
   to source claims.

## What you're producing

`docs/plans/grammar-wave4e-perf-diagnostic.md` containing:

### Section 1: Setup

- Build `compiler.smt2` via `scripts/build-compiler-smt2.sh`.
- Capture its size + line count.

### Section 2: Functionizer load report

Run on a SHORT input first (something simple like a single
`claim foo\n x ∈ Int = 5`):

```bash
EVIDENT_FUNCTIONIZE_STATS=verbose ./kernel/target/release/kernel compiler.smt2 < /tmp/short.ev
```

Capture the verbose load report. Quote it. Note:
- Total assertions extracted.
- Counts: JIT / interp / residual.
- Per-step shape categories (binop, ite, select, accessor,
  guarded-seq, seq-literal, unfunctionizable).
- Which steps are residual and why.

### Section 3: Per-tick trace on a short input

Add `EVIDENT_FUNCTIONIZE_TRACE=1` to the same run. Capture a
sample of per-tick lines:

```
[functionizer] tick N: X ms func / Y ms z3 / Z ms dispatch
```

Identify:
- What fraction of tick time is Z3 vs func vs dispatch?
- Is there per-tick growth (state-size related) or constant cost?

### Section 4: Same measurements on a real-shaped input

Pick a SMALLER-than-`test_hello` real input. The full
`test_hello` flattened is 4137 chars and times out — try
something like ~300-500 chars first. Goal is to extrapolate the
shape of the perf problem without running into timeouts.

Document tick count, total wall-clock, ms/tick mean and max.

### Section 5: Identify the bottleneck

From the data above, name the dominant bottleneck:

- Are most steps **residual** (the functionizer can't extract
  them)? → refactor target: change shapes in `compiler.ev` so
  they extract.
- Are steps **interp-only** (no JIT)? → look for shape categories
  the JIT doesn't cover.
- Is Z3 time itself dominant despite extraction? → maybe a
  pre-loop simplify isn't reaching the right shapes; investigate.
- Does cost grow per-tick? → state-size penalty; consider
  cons-list vs Array+len for the carried lexer state.

Provide ONE concrete refactor recommendation grounded in the
data, with a hypothesized speedup. (This is the input to wave
4f, which would actually do the refactor.)

### Section 6: Open questions

If you find puzzles you can't resolve from data, list them.

## Acceptance

1. The diagnostic report doc exists and has all 6 sections.
2. Section 2 has a real verbose load report quoted.
3. Section 5 names ONE recommended refactor with a hypothesis.
4. `./test.sh` is fully green (unchanged from before — no source
   code was touched).
5. Diff scoped to the new doc plus possibly a tiny test fixture
   under `tests/kernel/` for the SHORT diagnostic input.

## Forbidden

- Editing `compiler/*.ev`, `kernel/*`, `bootstrap/*`, `stdlib/*`.
- Adding Python.
- Implementing the refactor — that's wave 4f, informed by THIS
  task's findings.

## Reporting back

- Branch pushed.
- One-line headline: "Bottleneck is X; recommended refactor is Y;
  hypothesized speedup is Z."
- Path to the report doc.
- Cite the docs.

Be terse — the report doc is the deliverable, not the chat
message.
