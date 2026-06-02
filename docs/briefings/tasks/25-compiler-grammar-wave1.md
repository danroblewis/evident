# Task: compiler.ev grammar coverage — wave 1

## Why

`compiler/compiler.ev` today handles exactly `claim main\n    x ∈ Int = 5`
end-to-end (verified by `tests/kernel/test_compiler_driver_mvp.ev`
and `test_compiler_driver_readfile.ev`). That's an MVP. To produce
`compiler.smt2` (the deletion-path goal), `compiler.ev` needs to
handle every shape that `compiler.ev` itself uses, AND every
shape that the test corpus uses.

This task is **wave 1** of that extension. Wave 2/3+ will follow
based on the grammar-coverage survey running in parallel
(session B).

## Authorisation

You may edit `compiler/*.ev`, `tests/kernel/*.ev` (new fixtures),
and `docs/`. No `bootstrap/`, no `kernel/`, no Python.

## Required reading

1. `CLAUDE.md`.
2. `STATE.md` — the deletion-path blockers `compiler.smt2` is
   meant to clear.
3. `compiler/compiler.ev` — the current MVP driver.
4. `compiler/lexer.ev`, `compiler/parser.ev` — the primitives.
5. `compiler/translate_declare.ev`, `translate_bool.ev`,
   `translate_manifest.ev`, `translate_arith.ev`, `translate_match.ev`,
   `translate_seq.ev`, `translate_quant.ev` — the passes
   `compiler.ev` composes.
6. `tests/kernel/test_compiler_driver_mvp.ev` — the load-bearing
   integration test.
7. `tests/kernel/test_translate_arith_recursive.ev` — the
   recursive walker pattern.
8. `docs/plans/architecture-invariants.md` §functionizability
   — write your new passes in functionizable shape.

Cite #3 and #6 in your report.

## Wave 1 scope

Extend `compiler/compiler.ev` to handle these shapes:

### Shape 1: multi-membership claim body

```evident
claim foo
    x ∈ Int
    y ∈ Int
```

Expected SMT-LIB output (manifest then body):

```
;; manifest: state-fields = x:Int y:Int
;; (… other manifest lines …)
(declare-fun x () Int)
(declare-fun y () Int)
```

This requires the driver to handle multiple `BIMembership` items
in a `BodyItemList`, iterate over them, and emit one
`declare-fun` per. Use the work-stack pattern (cons-list is fine
per the invariants).

### Shape 2: arithmetic expression in a membership pin

```evident
claim bar
    x ∈ Int = 1
    y ∈ Int = x + 2
```

Expected output:

```
(declare-fun x () Int)
(assert (= x 1))
(declare-fun y () Int)
(assert (= y (+ x 2)))
```

This requires the driver to compose `translate_arith.ev`'s
recursive walker for the `= rhs` part. The MVP only handles
`= <literal>`; you're extending it to `= <Expr>`.

### Shape 3: minimum to keep this manageable

These two shapes are enough for wave 1. Do NOT attempt to handle:
- Enum declarations.
- Match expressions.
- Recursive claims / subclaim / quantifiers.
- `..` (passthrough) composition.
- Seq state-carry.
- generics.

These are all subsequent waves, informed by session B's survey.

## Test fixtures

Add the following kernel tests:

1. `tests/kernel/test_compiler_driver_multi_member.ev`
   Source: `claim foo\n    x ∈ Int\n    y ∈ Int\n`
   Expected output: contains both `(declare-fun x () Int)` and
   `(declare-fun y () Int)`.

2. `tests/kernel/test_compiler_driver_arith.ev`
   Source: `claim bar\n    x ∈ Int = 1\n    y ∈ Int = x + 2\n`
   Expected output: contains
   `(declare-fun x () Int)`, `(assert (= x 1))`,
   `(declare-fun y () Int)`, `(assert (= y (+ x 2)))`.

Use the constant-input pattern (no `ReadFile`) for these MVP
fixtures; the `ReadFile` path is proven by
`test_compiler_driver_readfile.ev` and the UTF-8 fix.

## Acceptance

1. `compiler/compiler.ev` modified to handle both shapes.
2. Both new test fixtures pass.
3. The existing MVP / readfile fixtures still pass byte-identical.
4. `./test.sh` is fully green in all 3 functionizer modes.
5. The functionizer diagnostic line at exit shows reasonable
   counts (don't expect everything to extract; do confirm it's
   not silently broken).
6. Diff scoped to `compiler/*.ev` + the two new test fixtures
   + possibly `docs/plans/grammar-wave1.md` documenting what
   landed.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `stdlib/`, or
  `compiler/lexer.ev`/`parser.ev` core enums (extend them only
  if a grammar shape genuinely needs a new AST variant; document
  in the report).
- Adding Python.
- Tackling shapes outside wave 1 (no enum, match, quant, seq,
  subclaim, generics, etc. — those are later waves).
- Hardcoding ASTs — the new fixtures must actually lex+parse
  their string input.

## Reporting back

- Branch pushed.
- The two new test fixtures' emitted SMT-LIB (1-2 lines each).
- `./test.sh` final line.
- Test count delta (current: 74).
- Any compiler/lexer.ev or parser.ev gaps you hit + how you
  worked around them (or, if blocked, write
  `docs/plans/blocked-grammar-wave1.md`).
- Cite the docs.

Be terse.
