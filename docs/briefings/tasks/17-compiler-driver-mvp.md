# Task: MVP compiler/compiler.ev driver

## Why

`scripts/check-deletable.sh` lists "compiler.smt2 doesn't exist"
as a blocker. To produce it, we need `compiler/compiler.ev` — a
single Evident program that reads a `.ev` file from disk, runs
it through lex + parse + translate, and emits the corresponding
`.smt2` to stdout.

This is the load-bearing integration test. Today we have:
- `compiler/lexer.ev` — token recognizers (per-char FSM).
- `compiler/parser.ev` — Expr/BodyItem/SchemaDecl ASTs + recognizer claims.
- `compiler/translate_arith.ev`, `_bool.ev`, `_match.ev`, `_seq.ev`,
  `_quant.ev` — recursive Expr→SMT-LIB walkers (tasks #13, #14).
- `compiler/translate_declare.ev`, `_manifest.ev` — non-recursive demos.

What's missing: a top-level FSM that drives the pipeline.

## Required reading

1. `CLAUDE.md`.
2. `docs/plans/architecture-invariants.md` — especially the
   functionizability principle (cons-lists > Seqs, fixed-arity
   match > variadic).
3. `docs/plans/DELETION-CHECKLIST.md` Phase 3.
4. `docs/plans/functionizer-integration.md` — shape rules.
5. `compiler/lexer.ev`, `compiler/parser.ev`.
6. `tests/kernel/test_pipeline_full.ev` (D2) and `test_pipeline_lex_parse.ev` (D1) — prior attempts that
   hardcode AST. Your job is to NOT hardcode AST.
7. `tests/kernel/test_translate_arith_recursive.ev` — the
   WorkList walker pattern.
8. `legacy-python/docs/runtime-architecture.md` — phase-driven
   multi-stage FSM pattern.

Cite #2, #4, and #5 in your report.

## What you're producing

`compiler/compiler.ev` that:

1. Reads a small `.ev` file via `ReadFile` (use a hardcoded path
   like `/tmp/compiler-input.ev`).
2. Lexes the input string into a TokenList using
   `compiler/lexer.ev`'s primitives in an accumulator FSM.
3. Parses the TokenList into one or more SchemaDecl ASTs using
   `compiler/parser.ev`'s primitives.
4. Translates each SchemaDecl body into SMT-LIB via
   `compiler/translate_*.ev`.
5. Prepends the manifest header.
6. Writes the result to stdout via `LibCall("libc", "puts", …)`.
7. Exits 0.

For MVP scope, the input must work on ONE small canonical example:

```evident
claim main
    x ∈ Int = 5
```

Expected output (manifest + body):

```smtlib
;; manifest: state-fields = x:Int
;; manifest: effects-name = effects
;; manifest: effect-enum-name = Effect
;; manifest: result-enum-name = Result
;; manifest: max-effects = 0
(declare-fun x () Int)
(assert (= x 5))
```

It does NOT need to handle every grammar shape. It needs to prove
the wiring works end-to-end on this one input.

## Test fixture

Add `tests/kernel/test_compiler_driver_mvp.ev` that:

- Writes the canonical input to `/tmp/compiler-input.ev` via
  `LibCall("libc", "write", …)` to a file descriptor opened with
  `LibCall("libc", "fopen", …)` — OR use Evident's `WriteFile`
  effect if it's available (see `legacy-python/docs/runtime-architecture.md`
  and the kernel's effect set).
- Invokes `compiler/compiler.ev` as a separate FSM (or — simpler
  — the test fixture IS the compiler driver, running on the
  hardcoded input).
- Verifies the emitted SMT-LIB matches the expected text.

Two acceptable shapes:

- **Two-FSM:** test fixture writes input, then "spawns"
  `compiler.ev` (kernel doesn't have spawn today — so probably skip).
- **One-FSM:** test fixture reads input as a String constant in
  the source (no ReadFile needed for MVP), runs the pipeline,
  emits expected output. This is the cleanest MVP.

Pick the simpler shape that demonstrates the pipeline.

## Functionizability constraints

Per `docs/plans/architecture-invariants.md` and
`docs/plans/functionizer-integration.md`:

- Use cons-lists for the work-stack (already proven in
  `test_translate_arith_recursive.ev`).
- Use fixed-arity match arms in the dispatch from
  TokenKind / BodyItem / SchemaDecl variants.
- Use `_<name>` state-carry for FSM memory.
- Avoid Z3 Seqs where cons-lists work.
- Avoid intra-tick recursion (use multi-tick walker + work-stack).

## Acceptance

1. `compiler/compiler.ev` exists.
2. `tests/kernel/test_compiler_driver_mvp.ev` passes.
3. Its emitted output matches the expected SMT-LIB.
4. `./test.sh` is fully green.
5. Diff touches only `compiler/compiler.ev`,
   `tests/kernel/test_compiler_driver_mvp.ev`, and possibly
   `docs/plans/blocked-compiler-driver.md` if blocked.

## If blocked

The most likely blockers, in expected order:

1. The existing `compiler/lexer.ev` / `parser.ev` only handles
   smaller subsets of the grammar than `claim main\n    x ∈ Int = 5`.
   Document which grammar shapes are missing and EITHER (a) pick
   an even smaller MVP input that the existing primitives handle,
   OR (b) write `docs/plans/blocked-compiler-driver.md`.
2. Composing 4-5 FSMs into one program with multiple state-pairs
   has unknown translator behavior. The legacy-python branch's
   pattern is `phase ∈ Int` driving a `match _phase` dispatch.
   Use that.
3. Effect-channel sharing between sub-FSMs (lexer emits diagnostic
   puts, parser does too, etc.) requires the `++`-composition
   pattern from architecture-invariants. If any of the constituent
   passes emits to `effects` directly, you'll need to refactor
   them to expose a `*_part` pattern.

Don't try to solve all of these in one task. If the MVP doesn't
land, write `docs/plans/blocked-compiler-driver.md` with the
specific blocker AND a concrete suggestion for what to do next
(extend a primitive, change the MVP scope, add a translator
capability).

## Forbidden

- Editing `kernel/`, `bootstrap/`, `legacy-*`, anything in
  `stdlib/` other than `stdlib/fti/` (which you almost certainly
  don't need for the MVP).
- Adding Python.
- Multi-channel `*_effects`.
- Calling `.simplify()` anywhere.
- Hardcoding an AST (the prior D2 fixture did this — that's not
  the goal here; the input must be a string that gets actually
  lexed and parsed).

## Reporting back

- Branch pushed.
- One sentence: did the pipeline work end-to-end on the MVP
  input, yes/no?
- The actual emitted SMT-LIB (or the closest you got).
- `./test.sh` final line.
- Test count delta.
- Any `docs/plans/blocked-*.md` you wrote.
- Cite the docs.

Be terse.
