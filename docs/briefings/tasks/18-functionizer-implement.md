# Task: Implement the functionizer (Z3-tactic version) into kernel/

## Why and authorisation

User quote, explicit authorisation:

> *"Just add the functionizers. I don't know why you froze the kernel
> already, apparently we're not done because we keep working on it.
> How are the other agents working on the kernel right now if we
> don't have the basics working? We have an agent out right now
> trying to get compiler.ev working. But yes we need the functionizer
> unblocked and working in the kernel. That was the actual task I
> asked to be done."*

The kernel is in active construction; the freeze applies after the
project completes. You are authorised to edit `kernel/`.

Scope of this task: the **Z3-tactic functionizer** (the "macro-finder"
variant — `simplify` + `propagate-values` + `extract_program` + native
`eval_program`). **Not** the Cranelift JIT backend (that's a later
follow-up).

## Required reading

1. `CLAUDE.md` — note the updated kernel row in the freeze table.
2. `docs/plans/architecture-invariants.md` — especially the
   functionizability principle and shape rules.
3. `docs/plans/functionizer-integration.md` — the design + the
   exact `kernel/src/tick.rs` seam points (~lines 113/130/176/238
   per session #16's notes).
4. `legacy-rust/functionizer/src/z3_eval.rs` — the
   `simplify_assertions` + `extract_program` source.
5. `legacy-rust/functionizer/src/z3_program.rs` — the `Z3Program`
   IR.
6. `legacy-rust/functionizer/src/decompose.rs` — the union-find
   sub-model partitioner (you may or may not need this in the
   first pass — read the integration doc).
7. `kernel/src/tick.rs` — where you wire the result.
8. `kernel/Cargo.toml` — what's already there.

Cite at least #3, #4, and #7 in your report.

## What you're producing

A new module `kernel/src/functionize.rs` that contains:

1. **`simplify_assertions`** — given the parsed-and-simplified body
   ASTs, run `simplify` + `propagate-values` tactics on each
   assertion (or on the combined goal). Returns the simplified
   AST set.
2. **`Z3Program` IR** — minimal shape: a sequence of `Z3Step`
   entries each representing "this output variable is defined as
   this expression over these inputs." Mirror the legacy IR but
   port only what `eval_program` needs.
3. **`extract_program`** — given a set of asserted equalities of the
   form `(= output (f input1 input2 …))` after simplification,
   produce a `Z3Program`. Cases the legacy code handles that
   matter: literal-RHS pins, simple binop, ITE arms, match arms
   over enums. If a shape doesn't fit `Z3Program`, leave the
   assertion in the residual set the tick loop hands to Z3.
4. **`eval_program`** — given a `Z3Program` and the current tick's
   inputs (state-carry pins, last_results, is_first_tick), compute
   each `Z3Step`'s output natively in Rust. No Z3 call needed for
   any step that's purely arithmetic / boolean / match / ITE over
   known values.

Wire into `kernel/src/tick.rs`:

- After `.simplify()` pre-loop (the existing pass), call
  `extract_program(&simplified_asts) → (Z3Program, Vec<residual_assertion>)`.
- Per tick: call `eval_program(&program, &pins) → outputs`. For any
  output the program produced, set it as a known constant before
  asserting the residual. For anything the program couldn't
  produce, fall through to the existing solve path.
- Effect dispatch, state extraction, halt rules unchanged.

## Acceptance

1. `kernel/src/functionize.rs` exists with the four pieces above.
2. `kernel/src/tick.rs` uses them in the pre-loop and per-tick
   paths.
3. `./test.sh` is fully green — all 68 kernel tests + 138
   conformance features + lang tests pass.
4. The integration doc at
   `docs/plans/functionizer-integration.md` is updated with a
   LANDED section noting what shapes are covered today vs deferred.
5. `scripts/check-deletable.sh` output unchanged (this is kernel
   capability, not deletion-path).
6. `Cargo.toml`: only the additional dependencies the Z3-tactic
   path needs (probably none — should be `z3-sys` only). NO
   Cranelift dependency in this task.
7. Diff limited to:
   - `kernel/src/functionize.rs` (new)
   - `kernel/src/tick.rs` (modified)
   - `kernel/src/main.rs` or wherever the module is declared
     (modified, one line)
   - `kernel/Cargo.toml` (only if you genuinely need a new crate)
   - `docs/plans/functionizer-integration.md` (LANDED section)
   - Possibly `docs/plans/architecture-invariants.md`
     (clarification updates)

## Forbidden

- Cranelift JIT — that's a follow-up task; not in this scope.
- Symbolic Regression or LLM functionizer variants — already
  dropped.
- Editing `bootstrap/`, `compiler/`, `stdlib/`, anything outside
  `kernel/` + the named docs.
- Adding new Python.
- Removing the Z3 fallback path — the functionizer is supplementary;
  it does not replace Z3.

## Measurement

Run the same benchmark from task #12 (`test_consolidated_lexer` +
the synthetic 16/64/256 KB bodies) BOTH ways:

- With the functionizer active (default).
- With it bypassed via `EVIDENT_FUNCTIONIZE=0` env (add this flag,
  default-on for the functionizer).

Report a side-by-side ms/tick table. The expectation: functionizer
should provide meaningful speedup on the larger bodies once any
non-trivial assertions are extracted. If it doesn't (because the
test bodies don't have functionizable shapes), explain why.

## Reporting back

- Branch pushed (`agent-18-functionizer-implement` or similar).
- One sentence: did the functionizer extract anything from the
  test fixtures' bodies, yes/no? Count of extracted Z3Steps.
- Benchmark table (with/without functionizer × 4 fixture sizes).
- `./test.sh` final line.
- Diff stat.
- Cite the docs.

Do NOT paste source. The coordinator reads files.

## If genuinely blocked

The most likely real blocker is that none of the current test
fixtures have functionizable shapes (they're all FSM ticks where
state-carry pins are the changing inputs). In that case:

- Write a tiny test fixture that DOES have a clearly-functionizable
  shape (e.g. `claim arith ; x ∈ Int ; y ∈ Int = x + 5 ; z ∈ Int = y * 2`)
  to exercise the extractor.
- Add it to `tests/kernel/test_functionizer_basic.ev`.
- Verify it works.

If you can't get any functionizable shape to extract, write
`docs/plans/blocked-functionizer.md` describing what `extract_program`
saw and why it found nothing. Do NOT silently land a functionizer
that does nothing on every input.
