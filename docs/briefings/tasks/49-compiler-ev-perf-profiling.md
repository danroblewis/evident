# Task: profile compiler.smt2's per-tick hot shapes (wave 4r)

## Why

compiler.smt2 runs at ~750 ms/tick on Z3 — for a 142-line lang
fixture that's ~28 min. Per-tick is dominated by Z3 solve cost.
Two past wins prove encoding choices matter ENORMOUSLY at this
layer:

- Wave 21 (cons-list → Seq for one specific accumulator): **250×**
- Wave 4f (lex bulk-skip): **8.2×** on comment-heavy / 2.8× general

We don't know which shapes in compiler.ev cost the most per-tick.
This wave instruments the kernel to attribute per-tick time to
assertion groups (or proxies), runs the kernel on a representative
input, identifies the top 5 most expensive shapes, and recommends
ONE concrete encoding change for the next wave.

This is **diagnostic-only**. No source code in compiler.ev/stdlib
gets edited. Output is a report doc.

## Authorisation

Edit:
- `kernel/src/functionize/mod.rs` and/or `kernel/src/tick.rs` —
  add per-tick / per-step timing instrumentation behind a new env
  var (`EVIDENT_FUNCTIONIZE_TIMING=1` or similar). The
  instrumentation must NOT change default behaviour.
- `docs/plans/wave-4r-...md` — the report.
- Small fixture `.ev` files if needed to drive the kernel.

Forbidden: `compiler/*.ev`, `stdlib/`, `bootstrap/`,
`tests/lang_tests/`, `tests/conformance/`, Python.

## Required reading

1. `CLAUDE.md` — Functionizer diagnostics section.
2. `docs/plans/grammar-wave4e-perf-diagnostic.md` — past
   diagnostic; this is the next-level extension.
3. `kernel/src/functionize/mod.rs` — the per-tick run loop and
   timing collection (`stats.t_func`, `stats.t_z3`).
4. `kernel/src/tick.rs` — the parent tick loop.
5. `compiler/compiler.ev` — for mapping step names back to source
   lines.
6. Memory: [[project_functionizer_macro_finder_extracted]] for the
   shapes the functionizer recognizes.

## Scope

### Item 1: instrumentation

Add per-step (or per-assertion-group) timing. Suggested approach:
- During the per-tick Z3 solve, capture which assertions take the
  most time. Z3 doesn't expose this directly, but we can split
  the body into bands and time each band's incremental cost.
- OR: time each STATE FIELD's value extraction from the Z3 model
  (current state-field count is ~200; the readback should be
  per-tick measurable).
- OR: take a sample-profile of the Z3 invocation (use the
  existing `sample` macOS tool, captured into the wave doc).

Pick whichever gives actionable signal — bands give shape info;
per-state-field gives "which fields are expensive to compute";
sample-profile gives "which Z3 tactics dominate." Any of these
unblocks the next wave.

### Item 2: representative inputs

Run with at least two inputs:
- A **trivial** `.ev` (1 claim, 1 line) — baseline noise floor
- A **lang_test-shaped** `.ev` — e.g.
  `tests/lang_tests/test_enums_basic.ev` (19 claims, exercises
  enums + ternary + record + match-style shapes)

For each: capture total wall-clock, per-tick cost, ticks consumed,
hot bands / state fields / tactics.

### Item 3: analysis

Identify the TOP 5 most expensive shapes by total time
contribution. For each:
- What's the shape (e.g. "200-field state-carry pin pass",
  "char-by-char tokenize", "string concatenation chain")?
- Where in compiler.ev does it come from (line/claim)?
- What encoding change might reduce it (cite past wins where
  similar)?
- Estimated speedup if changed?

### Item 4: recommendation

ONE concrete recommendation for the next wave. Format:

> Change X in compiler.ev (specific lines / claim) from shape A
> to shape B. Hypothesized speedup: Y×. Verify by Z.

This is the input to wave 4s, which actually implements the
change.

## Acceptance

1. Instrumentation lands behind an env var, doesn't change default
   behaviour.
2. Report doc exists with all 4 sections.
3. Report identifies top 5 shapes with concrete recommendations.
4. ONE recommendation is picked for wave 4s with hypothesized
   speedup.
5. `./test.sh` green (default mode, no flag set — instrumentation
   is opt-in).

## Forbidden

- Editing `compiler/*.ev`, `stdlib/`, `bootstrap/`, Python.
- Implementing the recommendation (that's wave 4s).
- Skipping Item 2's lang-test-shaped input.

## Known gotchas

- The kernel's `EVIDENT_FUNCTIONIZE_STATS=verbose` already gives
  some per-step shape categories. Build on that, don't duplicate.
- Z3 timing instrumentation must be careful — high-frequency
  per-call profiling can dominate the cost being measured.
- compiler.smt2 has 200+ state fields; whole-body timing isn't
  useful — need a finer-grain attribution.

## Reporting back

- Branch (`agent-49-compiler-ev-perf-profiling`).
- Items 1-4 status.
- Top 5 shapes (the headline).
- The ONE recommendation for wave 4s.
- Cite docs.

Be terse — the report doc is the deliverable, not the chat
message.
