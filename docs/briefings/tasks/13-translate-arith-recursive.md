# Task: Make compiler/translate_arith.ev recursive via Stack FTI

## Why

`compiler/translate_arith.ev` today handles one-level `EBinOp(op, EInt, EInt)`.
That's a demo, not a translator pass. A real translator pass walks
an arbitrary Expr tree, producing nested SMT-LIB like `(+ (* 1 2) 3)`.

The Stack FTI (`stdlib/fti/stack.ev`) is the tool. Use it as a
work-stack to drive depth-first traversal: push subtrees, pop and
emit, interleaving structural literals (`(`, ` `, `)`) per the
work-stack pattern documented in `tests/kernel/test_ast_walker.ev`
and elsewhere.

This is the proof-of-concept that:
- FTIs scale to real compiler work, not just toy stack/queue
  fixtures.
- The per-pass `translate_*.ev` files become composable.
- The path to `compiler/compiler.ev` (the full driver) is real.

If this works, the remaining `translate_*.ev` files follow the
same pattern. If it doesn't, we learn what's actually missing.

## Required reading

1. `CLAUDE.md`.
2. `docs/plans/architecture-invariants.md`.
3. `docs/plans/DELETION-CHECKLIST.md`.
4. `stdlib/fti/stack.ev` — the FTI you're using.
5. `tests/kernel/test_fti_stack.ev` — the FTI usage pattern.
6. `compiler/translate_arith.ev` — what's there today.
7. `tests/kernel/test_translate_arith.ev` — the demo test.
8. `tests/kernel/test_ast_walker.ev` — the work-stack walker
   pattern (already in the repo).
9. `legacy-python/docs/fti-composition.md` for FTI composition
   semantics if you're unsure how the FTI's body inlines.

Cite #4, #6, and #8 in your report.

## What you're producing

1. Update `compiler/translate_arith.ev` so it can handle arbitrary
   Expr trees, not just one level. Use the Stack FTI as the work
   stack carrying both `Expr` items to process and literal `String`
   chunks to emit, with a mode/discriminator to tell the two apart
   (see `test_ast_walker.ev`'s `WIProcess(Expr) | WIEmit(String)`
   shape — already in `compiler/parser.ev` as `WorkItem`).

2. Add `tests/kernel/test_translate_arith_recursive.ev`:
   - Build a 3-deep AST: `EBinOp(OpPlus, EBinOp(OpMul, EInt(1), EInt(2)), EInt(3))`.
   - Walk it via the Stack FTI.
   - Emit `(+ (* 1 2) 3)` (or similar SMT-LIB) to stdout.
   - `-- expect:` lines verify.

3. Existing tests (`test_translate_arith.ev` and all others) must
   still pass. If the existing demo needs adjustment to work with
   the new shape, update it; do not delete it.

## Acceptance

1. `tests/kernel/test_translate_arith_recursive.ev` exists and
   emits `(+ (* 1 2) 3)`.
2. `tests/kernel/test_translate_arith.ev` still passes.
3. `./test.sh` is fully green.
4. Diff touches only:
   - `compiler/translate_arith.ev`
   - `tests/kernel/test_translate_arith_recursive.ev` (new)
   - Possibly `tests/kernel/test_translate_arith.ev` (if the demo
     needs to adapt — no destructive changes, just match the new
     signature).

## Forbidden

- Editing `bootstrap/`, `kernel/`, or any other `compiler/*.ev`
  file (only `translate_arith.ev`).
- Adding new Python.
- Editing `stdlib/fti/stack.ev` (the FTI is finished).
- Multi-channel `*_effects` patterns.
- Calling `.simplify()` anywhere — kernel-only.

## Reporting back

- Branch pushed.
- Output of `tests/kernel/test_translate_arith_recursive.ev` (the
  one or two emitted lines).
- `./test.sh` final line.
- One sentence: did the Stack FTI compose with the translator
  pass cleanly, yes/no? If no, where did it break?
- Cite docs.

If blocked, write `docs/plans/blocked-translate-arith.md` —
specifically capturing what about the Stack FTI didn't work for
this use case (the previous FTI work was on isolated fixtures;
composing with a translator is the load-bearing test).

Be terse.
