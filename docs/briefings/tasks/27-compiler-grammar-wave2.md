# Task: compiler.ev grammar coverage — wave 2

## Why

Wave 1 (task #25) added multi-membership + arithmetic pins to
`compiler/compiler.ev`. The grammar coverage survey
(`docs/plans/grammar-coverage-survey.md`) identified wave 2's
must-haves: the **scalar bodies & flow primitives** that bridge to
wave 3 (enum + match), without which `compiler.ev` cannot compile
itself.

The single most important wave 2 item is **ternary**: 191 uses
across the corpus, no dedicated translator pass exists, and
`compiler.ev` itself is built on it.

## Authorisation

You may edit `compiler/*.ev`, `tests/kernel/*.ev` (new fixtures),
and `docs/`. No `bootstrap/`, no `kernel/`, no Python.

## Required reading

1. `CLAUDE.md`.
2. `docs/plans/grammar-coverage-survey.md` — the wave plan you're
   implementing wave 2 of.
3. `docs/plans/grammar-wave1.md` — what wave 1 landed (and the
   variable-name-collision finding from session #25).
4. `compiler/compiler.ev` — the driver to extend.
5. `compiler/parse_body.ev` — wave 1's `MembershipStep` /
   `TLHd`/`TLTl` helpers, the pattern you'll follow.
6. `compiler/translate_bool.ev`, `compiler/translate_arith.ev` —
   passes you'll compose / extend.
7. `compiler/parser.ev` — already has `Expr`/`Op` AST + `Ternary?`
   in survey-flagged shapes; check if a `TernaryExpr` variant
   exists or needs adding.
8. `docs/plans/architecture-invariants.md` §functionizability —
   write the new passes in functionizable shape (ITE-form for
   ternary maps directly to the functionizer's `ite` category).

Cite #2 and #5 in your report.

## Wave 2 scope (all five items)

### Item 1: ternary (`? :`) — THE biggest gap

Translator: NEW pass `compiler/translate_ternary.ev` (no existing
pass). Walk pattern: recursive Expr walker, mirroring
`translate_arith.ev`. Lowering:

```
EBinOp(OpTernary, cond, EBinOp(OpTernaryArms, then, else))
  →  (ite <cond> <then> <else>)
```

OR if the parser already gives a flat `ETernary(cond, then, else)`
AST node, use that. Check the AST first.

Compose into `compiler/compiler.ev` so that `claim foo\n x ∈ Int = (1 ? 2 : 3)`
emits `(declare-fun x () Int)` `(assert (= x (ite 1 2 3)))` —
exact syntax depends on what `compiler.ev`'s body translator wires
up.

Test fixture: `tests/kernel/test_compiler_driver_ternary.ev`.

### Item 2: comparisons (`< ≤ > ≥ ≠`)

Translator: extend `compiler/translate_bool.ev` to handle these
binops. Lowering is direct: `(< l r)`, `(<= l r)`, `(not (= l r))`.

Test fixture: `tests/kernel/test_compiler_driver_comparisons.ev`.

### Item 3: boolean connectives (`∧ ∨ ¬ ⇒`)

Translator: extend `compiler/translate_bool.ev` to handle these.
Lowerings: `(and l r)`, `(or l r)`, `(not e)`, `(=> l r)`.

Test fixture: `tests/kernel/test_compiler_driver_bool_ops.ev`.

### Item 4: ASCII String literals

Translator: extend the existing translate path to emit string
literals. Lowering: `"hello"` → SMT-LIB `"hello"` literal. Watch
the UTF-8 escape pattern from the kernel UTF-8 fix (task #20):
non-ASCII codepoints in string literals should be escaped as
`\u{hex}`. ASCII pass-through is fine; non-ASCII out of scope for
wave 2.

Test fixture: `tests/kernel/test_compiler_driver_strings.ev`.

### Item 5: `is_first_tick` + `_<name>` state-carry

`is_first_tick` is auto-injected by `emit.rs::auto_inject` in
bootstrap. Mirror this behavior in `compiler.ev`: if a claim's body
doesn't declare `is_first_tick`, the manifest header includes it
and the compiler emits an `(assert (= is_first_tick true))` /
`(assert (not is_first_tick))` constraint chain (or whatever
bootstrap actually does — check).

`_<name>` state carry: when a body declares `x ∈ Int` and `_x ∈ Int`,
the compiler recognizes the pairing. No translator change needed
beyond emitting the `declare-fun` for both. The kernel handles the
actual pinning.

Test fixture: `tests/kernel/test_compiler_driver_state_carry.ev`
with at least one state-carried `Int` variable proving the manifest
includes it as a state field.

## Acceptance

1. All 5 wave-2 shapes work end-to-end via `compiler.ev`.
2. 5 new test fixtures pass.
3. All wave-1 + MVP fixtures still pass byte-identical.
4. `./test.sh` is fully green in all 3 functionizer modes.
5. Functionizer summary line shows reasonable counts.
6. Diff scoped to `compiler/*.ev` + new test fixtures + new
   `docs/plans/grammar-wave2.md`.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `stdlib/`.
- Editing `compiler/lexer.ev` / `parser.ev` core enums unless
  genuinely required for a wave-2 shape — and if so, document
  why. (Ternary may need `OpTernary` if not already there.)
- Adding Python.
- Tackling wave 3+ shapes (enum, match, Seq ops, generics).
- Hardcoding ASTs in test fixtures — they must lex+parse strings.

## Known footgun (from wave 1)

> Evident claim composition leaks a callee's body-local variable
> names into the caller and unifies them by name → silent UNSAT
> (no error, `sample` returns `[]`).

Prefix all body-local variable names in your new pass claims to
avoid name collisions with the caller's passes. See
`compiler/parse_body.ev`'s `ms_` prefix as the pattern.

## Reporting back

- Branch pushed (`agent-27-compiler-grammar-wave2`).
- The 5 new test fixtures' emitted SMT-LIB (1-2 lines each).
- `./test.sh` final line.
- Test count delta (current: 76).
- Any new `Expr`/`Op` variants you added in `parser.ev`.
- Cite the survey, wave-1 doc, and any other docs.

Be terse.
