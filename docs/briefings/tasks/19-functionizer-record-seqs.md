# Task: Extend functionizer with `recompose_record_seqs` — make Seqs first-class

## Authorisation + why

User explicitly authorised this kernel extension as a parallel
session with #17 (compiler MVP), to close the cons-list-vs-Seq
gap **before** session 17 accumulates more cons-shaped code. User
quote:

> *"I don't like using the Cons things because we never see Cons
> in constraint system models. Seq made more sense, and I would
> like to see if we can replace Cons with Seq, even if it has to
> be some rewrite rules."*

And on timing:

> *"Yes, spawn that session, clearly."*

The kernel-construction freeze framing applies: you may edit
`kernel/src/functionize/` and `kernel/src/tick.rs`. Same
authorisation envelope as task #18 (which just landed).

## What this unlocks

Today, `kernel/src/functionize/` extracts only scalar Int/Bool
pins; Seqs are opaque (assertions touching Seqs fall through to
Z3). That means a session writing `xs ∈ Seq(Rect) = ⟨r1, r2, r3⟩`
gets Z3-slow performance, while a session writing the equivalent
cons-list gets functionizer-fast performance. Sessions pick
cons-lists. The cycle keeps going.

`recompose_record_seqs` in the legacy code recognises Seqs of
records (and Seqs of primitives) as functionizable shapes, lifts
the per-element assertions into a vector function, and emits a
compiled per-element computation. After this lands, sessions can
write the constraint-natural Seq form and get equivalent perf.
The invariants doc then flips its recommendation from cons-list
to Seq.

## Required reading

1. `CLAUDE.md` — note the kernel row is "active construction."
2. `docs/plans/architecture-invariants.md` — the
   "TRANSITIONAL — cons-lists are an expedient" section is the
   thing you're about to remove the transitional flag from.
3. `docs/plans/ideas.md` — the cons→Seq deferred-work entry is
   the thing you're unblocking.
4. `docs/plans/functionizer-integration.md` — LANDED section
   (what's covered today) and the `recompose_record_seqs`
   reference.
5. `kernel/src/functionize/mod.rs`, `extract.rs` (or
   wherever extract is), `eval.rs`, `jit.rs` — what's there now.
6. `legacy-rust/functionizer/src/z3_eval.rs` — search for
   `recompose_record_seqs` (or `recompose_seqs`, `record_seq`,
   `seq_record`) and read the implementation. Likely in the
   same file or a sibling. Cross-reference with
   `legacy-rust/functionizer/src/z3_program.rs` if a new IR
   shape is needed (e.g. a `Z3Step` variant for vector ops).
7. `tests/kernel/test_functionizer_basic.ev` — the existing
   fixture exercising the scalar extractor; mirror its shape for
   the new Seq fixture.

Cite at minimum #4, #6, and #7 in your report.

## What you're producing

### 1. Port `recompose_record_seqs` into `kernel/src/functionize/`

- Add it to `mod.rs`'s pipeline: after the current
  `extract_program` pass, run `recompose_record_seqs` over the
  remaining residual assertions to identify Seq-shaped extractable
  shapes.
- Add new `Z3Step` IR variants if needed (e.g. `SeqLit`,
  `SeqMap`, `SeqIndex`, `SeqLen` — whatever the legacy used).
- Extend `eval.rs` to interpret the new step shapes.
- Extend `jit.rs` to compile them where Cranelift can (likely:
  `SeqLen`, `SeqIndex` over fixed-known sizes; the variable-length
  cases stay in the interpreter for now).

### 2. Test fixture

Add `tests/kernel/test_functionizer_seqs.ev` that:

- Defines a Seq of records (e.g. `type Rect(x, y, w, h ∈ Int) ; rs ∈ Seq(Rect) = ⟨Rect(1,2,3,4), Rect(5,6,7,8)⟩`).
- Asserts a property that's pure-function over the Seq (e.g. sum of all `w` fields, or count of rects with `x > 0`).
- Runs at scale — at least 100 ticks if FSM-shaped, or one large
  evaluation if not.
- Verifies the functionizer extracted the Seq-shaped step (your
  report should include the extracted-step count).

### 3. Benchmark

Run the same 3-mode benchmark from task #18 on:
- `test_functionizer_basic.ev` (existing scalar fixture).
- `test_functionizer_seqs.ev` (new Seq fixture).

Report a table:

```
                          JIT  interp  bypass
basic   (scalar)          ...     ...     ...
seqs    (record-Seq)      ...     ...     ...
```

The seqs row should show JIT or interp materially faster than
bypass. If they're identical, recompose_record_seqs isn't
extracting and you need to investigate.

### 4. Update invariants

Update `docs/plans/architecture-invariants.md`:
- Remove the "TRANSITIONAL — cons-lists are an expedient"
  paragraph (the one ending "Sessions should know this and not
  entrench cons-list-specific patterns").
- Replace it with the destination-shape guidance: Seqs are
  preferred for bounded data; cons-lists are acceptable for
  AST-traversal work stacks where the destructuring is more
  ergonomic, but new state-carry of typed collections should be
  Seqs.

Update `docs/plans/ideas.md`:
- Mark the "Replace Cons-lists with Seqs" entry as PARTIALLY
  COMPLETE — the functionizer side is done; the cons→Seq sweep
  rewrite of existing compiler code is still a separate task.

## Acceptance

1. `kernel/src/functionize/` has the `recompose_record_seqs`
   path wired into `extract`, `eval`, and `jit`.
2. `tests/kernel/test_functionizer_seqs.ev` exists, passes,
   and exercises the new path.
3. `./test.sh` is fully green in all 3 modes (default,
   `EVIDENT_FUNCTIONIZE_JIT=0`, `EVIDENT_FUNCTIONIZE=0`).
4. Benchmark table shows the seqs row faster than bypass.
5. Invariants and ideas docs updated as above.
6. Diff limited to:
   - `kernel/src/functionize/*.rs`
   - `kernel/src/tick.rs` (only if the wiring change is needed)
   - `tests/kernel/test_functionizer_seqs.ev` (new)
   - `docs/plans/architecture-invariants.md`
   - `docs/plans/ideas.md`
   - `docs/plans/functionizer-integration.md` (LANDED section
     extended)
   - `kernel/Cargo.toml` only if a new dep is required
     (probably not — Cranelift already covers JIT codegen).

## Forbidden

- Cranelift JIT for variable-length Seq operations is OUT OF
  SCOPE (the legacy code may have this; defer if so).
- Editing `bootstrap/`, `compiler/`, `stdlib/`, anything outside
  `kernel/` + the named docs.
- Removing the Z3 fallback path (the functionizer is supplementary).
- Adding new Python.

## Reporting back

- Branch pushed.
- One sentence: did record-Seq extraction work, yes/no? How many
  new Z3Steps does the test fixture produce?
- Benchmark table (3 modes × 2 fixtures).
- `./test.sh` final lines in all 3 modes.
- Diff stat.
- Cite docs.

If you discover the legacy code's `recompose_record_seqs` is more
complex than expected (e.g. requires the union-find decompose
pass first), document the dependency, port the minimum needed,
and STOP. Don't over-port — partial extension is fine if it
covers the cases the invariants need.
