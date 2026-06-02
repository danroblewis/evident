# Task: Functionizer diagnostics + observability flags

## Why

The functionizer landed in tasks #18 + #19 with a working
extractor + interp + JIT + recompose_record_seqs. It now silently
does its work — sessions can't see whether their code shape
actually functionized.

The architecture-invariants doc tells sessions to prefer
functionizable shapes over Z3-fast ones, but currently there's no
runtime feedback to confirm a shape worked. Future
performance-sensitive sessions (cons→Seq sweep verification, FTI
honesty audit, compiler.ev extensions) need visibility.

User quote:

> *"The agents working on performance things probably want to
> know if something got functionized, or how much of a model got
> functionized, or what portions of the Z3 model got
> functionized. This would help the worker sessions a lot."*

## Authorisation

Kernel work (`kernel/src/functionize/` + `kernel/src/tick.rs`).
Active-construction; same authorisation envelope as #18/#19.

## Required reading

1. `CLAUDE.md` (freeze table).
2. `docs/plans/architecture-invariants.md` §Functionizability
   over Z3-fast.
3. `docs/plans/functionizer-integration.md` (especially §6 LANDED
   covering what shapes the functionizer handles today).
4. `kernel/src/functionize/{mod,eval,jit}.rs` — what's there.
5. `kernel/src/tick.rs` — the existing 3-mode env flags
   (`EVIDENT_FUNCTIONIZE`, `EVIDENT_FUNCTIONIZE_JIT`).
6. `tests/kernel/test_functionizer_basic.ev` and
   `test_functionizer_seqs.ev` — for testing the flags.

Cite #3 and #4 in your report.

## What you're producing

Three env-flag-gated diagnostic levels, off by default.

### Level 1: `EVIDENT_FUNCTIONIZE_STATS=1`

A one-line summary printed to stderr at program exit:

```
[functionizer] N total / J JIT / I interp / R residual; T_total ms (T_func ms / T_z3 ms)
```

Where:
- N = total assertions in the simplified body
- J = number of Z3Steps that compiled to JIT
- I = number of Z3Steps that fell back to interpreter
- R = residual assertions that still go to Z3 each tick
- T_total = total wall time in the tick loop
- T_func = wall time spent in functionizer eval/JIT calls
- T_z3 = wall time in Z3 check/model-read

### Level 2: `EVIDENT_FUNCTIONIZE_STATS=verbose`

Everything in level 1 PLUS, at program load, before the tick loop:

```
[functionizer] load:
  body asserts: N
  extracted:    K (J JIT, I interp)
  residual:     R
  steps:
    [1] count        ← (+ _count 1)            JIT     [binop]
    [2] r0.x         ← (Rect.x (select rs 0))  JIT     [select+accessor]
    [3] effects      ← guarded(...)            interp  [guarded-seq]
    [4] last_results ← unfunctionizable        residual [seq-typed input]
    ...
```

Each step's shape category in the bracket lets sessions see
WHICH category fell through (and they can adjust accordingly).

### Level 3: `EVIDENT_FUNCTIONIZE_TRACE=1`

In addition to whatever stats level is set, print per-tick
timing to stderr:

```
[functionizer] tick 17:  0.3ms func / 1.2ms z3 / 0.05ms dispatch
```

This is high-frequency output; only useful for short
investigations.

### Off (no flags set)

Zero output. Zero perf overhead beyond a couple of branch
predictions that the JIT should elide.

## Implementation notes

- Add a `FunctionizeStats` struct in `kernel/src/functionize/mod.rs`
  that tracks counts + accumulated timings. Initialize it once
  per program; update it incrementally during extract and
  per-tick.
- The verbose load report needs the IR to carry a "shape
  category" tag per step. The categories should be small and
  obvious: `binop`, `ite`, `select`, `accessor`, `seq-literal`,
  `guarded-seq`, `unfunctionizable`. Add to `Z3Step` if needed.
- Use `std::time::Instant` for timing; print at exit via a
  `Drop` impl or an explicit shutdown call from tick.rs.
- Print to stderr (not stdout — stdout is the program's output).

## Acceptance

1. All three flags work independently. Setting `STATS=1` alone
   produces the one-line summary. `STATS=verbose` produces the
   table. `TRACE=1` produces per-tick lines.
2. With no flags set, output is unchanged from current behavior.
3. `./test.sh` is fully green in all combinations:
   - default (no flags)
   - `EVIDENT_FUNCTIONIZE_STATS=1`
   - `EVIDENT_FUNCTIONIZE_STATS=verbose`
   - `EVIDENT_FUNCTIONIZE_TRACE=1`
4. Run `test_functionizer_basic.ev` and `test_functionizer_seqs.ev`
   under each flag setting and capture the diagnostic output.
   Include those captures in your report (this is the proof the
   diagnostics work).
5. Diff scoped to:
   - `kernel/src/functionize/mod.rs` (and possibly `eval.rs`,
     `jit.rs`)
   - `kernel/src/tick.rs`
   - Possibly `docs/plans/functionizer-integration.md` (new
     §"Diagnostic flags" section).
6. No `Cargo.toml` changes (use stdlib's `std::time::Instant`).
7. `scripts/check-deletable.sh` output unchanged.

## Forbidden

- Editing outside `kernel/src/functionize/` + `kernel/src/tick.rs`
  + docs.
- Editing `bootstrap/`, `compiler/`, `stdlib/`, `tests/`.
  (Exception: the test fixtures may need a single new test that
  exercises the flags end-to-end via `EVIDENT_FUNCTIONIZE_STATS`;
  if you add one, name it `tests/kernel/test_functionizer_diagnostics.ev`.)
- Adding Python.
- Changing the existing flags' semantics (`EVIDENT_FUNCTIONIZE`,
  `EVIDENT_FUNCTIONIZE_JIT`).
- Making the diagnostics on-by-default. Sessions running tests
  don't want noise.

## Reporting back

- Branch pushed.
- The four captures from acceptance #4 (per flag × per test
  fixture). Trim to the relevant lines.
- `./test.sh` final line.
- Diff stat.
- Cite the docs.

Be terse.
