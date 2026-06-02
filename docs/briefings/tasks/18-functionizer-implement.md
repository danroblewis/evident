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

Scope of this task: the **full Z3-tactic functionizer** — both
parts in one session:

1. The extractor + native interpreter — `simplify` +
   `propagate-values` + `extract_program` + native `eval_program`.
2. The **Cranelift JIT backend** — compile each `Z3Step` to a
   native function that the tick loop calls directly.

User correction (the previous spec deferred Cranelift; user
overruled): *"We do want the Cranelift JIT working. So we need yet
another session to get it done?"* — no, both land in this task.

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
   sub-model partitioner.
7. `legacy-rust/functionizer/src/cranelift.rs` — the JIT backend
   you're porting.
8. `legacy-rust/functionizer/tests/cranelift_jit_hello.rs`,
   `tests/jit_minimal.rs`, `tests/per_component_jit.rs` —
   reference test fixtures showing the JIT shape.
9. `kernel/src/tick.rs` — where you wire the result.
10. `kernel/Cargo.toml` — what's already there.

Cite at least #3, #4, #7, and #9 in your report.

## What you're producing

A new module `kernel/src/functionize/` (a directory module, since
this has multiple files) containing:

1. **`mod.rs`** — public surface + the high-level
   `functionize(body_asts) -> (Program, Vec<residual>)` entry.
2. **`simplify_assertions`** (likely in `mod.rs` or its own file) —
   given the parsed body ASTs, run `simplify` +
   `propagate-values` tactics. Returns the simplified AST set.
3. **`program.rs`** — `Z3Program` IR: a sequence of `Z3Step`
   entries each representing "this output variable is defined as
   this expression over these inputs." Mirror the legacy IR.
4. **`extract.rs`** (or in `mod.rs`) — `extract_program` over the
   simplified ASTs. Cases that matter: literal-RHS pins, binop,
   ITE arms, match arms over enums. Shapes that don't fit stay
   in the residual set Z3 still handles.
5. **`eval.rs`** — `eval_program`, the native (non-JIT)
   interpreter. Falls back to this when the JIT isn't available
   or when the shape doesn't compile.
6. **`jit.rs`** — Cranelift JIT backend. For each `Z3Step`,
   produce a native function. Cache the compiled functions. The
   per-tick path prefers calling the JIT'd function over the
   interpreter when available.

Wire into `kernel/src/tick.rs`:

- After `.simplify()` pre-loop: call `functionize(&simplified)` →
  `(Program, residual_assertions)`.
- For each step in `Program`, attempt to JIT-compile it (`jit::compile_step`).
  Steps that compile go into a `Vec<JitFn>`; steps that don't fall
  back to the interpreter. Cache both.
- Per tick: for each input → output mapping the Program covers,
  call the JIT'd function (or interpreter) with the current pin
  values. Set the result as a known constant on the solver before
  asserting residuals.
- For anything the Program couldn't extract, fall through to the
  existing solve path. Z3 still runs on the residual.
- Effect dispatch, state extraction, halt rules unchanged.

Env flags:
- `EVIDENT_FUNCTIONIZE=0` — bypass the extractor and JIT entirely
  (current pre-Functionizer behavior).
- `EVIDENT_FUNCTIONIZE_JIT=0` — extract + interpret natively, but
  don't JIT-compile (useful for measuring JIT overhead vs interp).
- Both flags default to "on."

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
6. `Cargo.toml`: the additional crates this work needs. At
   minimum Cranelift's runtime crates (`cranelift`,
   `cranelift-jit`, `cranelift-module`, `cranelift-frontend`,
   `cranelift-codegen` — see what `legacy-rust/functionizer/`'s
   own `Cargo.toml` references and pin the same versions).
7. Diff limited to:
   - `kernel/src/functionize/mod.rs` + supporting files (new)
   - `kernel/src/tick.rs` (modified)
   - `kernel/src/main.rs` or wherever the module is declared
     (modified, one line)
   - `kernel/Cargo.toml` (Cranelift deps added)
   - `docs/plans/functionizer-integration.md` (LANDED section)
   - Possibly `docs/plans/architecture-invariants.md`
     (clarification updates)

## Forbidden

- Symbolic Regression or LLM functionizer variants — already
  dropped.
- Editing `bootstrap/`, `compiler/`, `stdlib/`, anything outside
  `kernel/` + the named docs.
- Adding new Python.
- Removing the Z3 fallback path — the functionizer is supplementary;
  it does not replace Z3.

## Measurement

Run the same benchmark from task #12 (`test_consolidated_lexer` +
the synthetic 16/64/256 KB bodies) three ways:

- Default: functionize + JIT compile + JIT call.
- `EVIDENT_FUNCTIONIZE_JIT=0`: functionize + interpret natively.
- `EVIDENT_FUNCTIONIZE=0`: bypass entirely (the prior kernel).

Report a 3-column ms/tick table. The expectation: JIT > interpret
> bypass on the larger bodies, once any non-trivial assertions are
extracted. If they're equivalent (no functionizable shapes
present), explain why and add a fixture that does have a
functionizable shape so the JIT path is exercised.

## Reporting back

- Branch pushed (`agent-18-functionizer-implement` or similar).
- One sentence: did the functionizer extract anything from the
  test fixtures' bodies, yes/no? Count of extracted Z3Steps.
  Count of Steps JIT-compiled vs falling back to interpret.
- 3-column benchmark table (JIT × interpret × bypass × fixture sizes).
- `./test.sh` final line.
- Diff stat (Cargo.toml will show Cranelift deps; that's expected).
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
