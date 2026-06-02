# Task: Make 4 more translator passes recursive

## Why

Task #13 proved the pattern: `compiler/translate_arith.ev` walks an
arbitrary recursive Expr tree using an in-Evident `WorkList`
cons-list and the depth-first walker from `test_ast_walker.ev`.
Output: `(+ (* 1 2) 3)`. Now extend the same pattern to the other
foundational passes so we converge on a usable
`compiler/compiler.ev` driver.

## Required reading

1. `CLAUDE.md`.
2. `docs/plans/architecture-invariants.md` — especially the
   FTI-vs-cons-list section just added.
3. `compiler/translate_arith.ev` + `tests/kernel/test_translate_arith_recursive.ev` — the worked pattern.
4. `tests/kernel/test_ast_walker.ev` — the walker template.
5. The four passes you'll modify (read each one in `compiler/`):
   `translate_bool.ev`, `translate_match.ev`, `translate_seq.ev`,
   `translate_quant.ev`.

Cite #3 in your report.

## What you're producing

For each of the 4 passes:

1. Modify the pass to handle arbitrary recursive shapes via the
   `WorkList` cons-list pattern. The existing one-level demo claim
   should stay (as a fixed-arity convenience), but a new
   `<PassName>Step` claim should be added that consumes a
   `WorkList` and emits the recursive structure.
2. Add `tests/kernel/test_translate_<pass>_recursive.ev` per pass:
   - `translate_bool`: `EBinOp(OpAnd, EBinOp(OpOr, EIdent("x"), EIdent("y")), EIdent("z"))` → `(and (or x y) z)`
   - `translate_match`: `EMatch(EIdent("e"), [MArm(MPCtor("Some", …), EInt(1)), MArm(MPWild, EInt(0))])` → nested `(ite ((_ is Some) e) 1 0)`
   - `translate_seq`: nested seq concat: `⟨1⟩ ++ ⟨2, 3⟩` → `(seq.++ (seq.unit 1) (seq.++ (seq.unit 2) (seq.unit 3)))`
   - `translate_quant`: `EForall("x", EIdent("items"), EBinOp(OpEq, EIdent("x"), EInt(0)))` → `(forall ((x Int)) (=> (>= x 0) (= x 0)))`
3. All existing tests must still pass.

If any of the 4 passes hits a real blocker (e.g. a Z3-side construct
the WorkList pattern can't express), document it in
`docs/plans/blocked-translate-<pass>.md` and proceed with the other
3 — partial wins land.

## Acceptance

1. 4 new `tests/kernel/test_translate_<pass>_recursive.ev` files.
2. Each emits the expected nested SMT-LIB.
3. `./test.sh` is fully green (current 64 kernel tests + 4 new = 68).
4. Diff touches only `compiler/translate_{bool,match,seq,quant}.ev`,
   the 4 new test fixtures, and any blocked-* notes.

## Forbidden

- Editing `kernel/`, `bootstrap/`, `stdlib/`, or other `compiler/*.ev`.
- Stack/Queue FTIs (per the doc, in-Evident cons-lists win for
  bounded data).
- Adding Python.

## Reporting back

- Branch pushed.
- Per-pass status: worked / blocked.
- 4 emitted SMT-LIB strings.
- `./test.sh` final line.
- Test count delta.
- Cite the docs.

Be terse.
