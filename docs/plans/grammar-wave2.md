# compiler.ev grammar coverage — wave 2

Status: **landed.** Extends `compiler/compiler.ev` from wave 1's
multi-membership + arithmetic pins (docs/plans/grammar-wave1.md) to the
**scalar bodies & flow primitives** the survey
(docs/plans/grammar-coverage-survey.md §3 "Wave 2") flagged as the cheap
self-host blockers.

## What landed (all five wave-2 items)

Every shape is lexed+parsed from a real source string — nothing hardcoded
as tokens/AST. MembershipStep builds the pin RHS `Expr` and routes it
through the per-pass translators.

1. **Ternary `c ? t : e` → `(ite c t e)`** — the single highest-value gap
   (191 corpus uses, a self-host blocker; survey §1 "Control / value
   forms", Open Q #3 "no `translate_ternary.ev` exists"). New pass
   `compiler/translate_ternary.ev` (`TernaryExprSmtlib` one-level +
   `TernaryTranslateStep` work-stack walker, mirroring
   `translate_arith.ev`). The ITE form maps directly onto the
   functionizer's `ite` shape category.
2. **Comparisons `< ≤ > ≥ ≠`** — `BoolExprSmtlib` (translate_bool.ev)
   widened: `(< l r)` `(<= l r)` `(> l r)` `(>= l r)`, and `≠` lowered as
   `(not (= l r))` per spec.
3. **Boolean connectives `∧ ∨ ¬ ⇒`** — `(and l r)` `(or l r)` `(=> l r)`
   and unary `¬ e → (not e)` (an `ENot` node). Also in `BoolExprSmtlib`.
4. **ASCII string literals** — driver's consolidated lexer gains a
   string-literal mode (`"` opens, chars accumulate, closing `"` emits
   `StringLit`; escapes out of scope). `AtomSmtlib`/`ExprAtomSmtlib`
   render `EStr(s) → "s"` (ASCII pass-through; non-ASCII escaping out of
   scope).
5. **`is_first_tick` auto-inject + `_<name>` state-carry** — mirrors
   `bootstrap/runtime/src/emit.rs`: when the source doesn't declare
   `is_first_tick`, the compiler prepends `(declare-fun is_first_tick ()
   Bool)` (tracked via the `ift` accumulator during the parse walk). A
   `_<name>` carry var (and `is_first_tick`) gets a declare-fun but is
   **excluded from the manifest state-fields**
   (`discover_state_fields`); MembershipStep emits `""` for its field
   and the driver skips empty fields.

## New AST (compiler/parser.ev + lexer.ev)

Editing the core enums was genuinely required (documented inline):

- `lexer.ev`: `Question` token (`?`) — ternary needs it; the lexer had no
  `?`. Also added `_` to `IsAlphaChar` so identifiers like `_count` /
  `is_first_tick` lex as one Ident (state-carry needs it).
- `parser.ev` `Op`: added `OpLt OpGt OpLeq OpGeq OpNeq OpConj OpDisj`.
  (Variant names are globally unique, so the `Op` members can't reuse the
  `Token` spellings `OpLe`/`OpGe`/`OpNe`/`OpAnd`/`OpOr` — hence the
  distinct names.) `OpImpl` already existed.
- `parser.ev` `Expr`: added `ETernary(Expr,Expr,Expr)`, `ENot(Expr)`,
  `EStr(String)`.

## Files

- `compiler/translate_ternary.ev` — NEW pass (ternary → ite).
- `compiler/translate_bool.ev` — `BoolExprSmtlib` widened to the full
  comparison + boolean-connective set + `ENot`; `ExprAtomSmtlib` renders
  `EStr`.
- `compiler/translate_arith.ev` — `AtomSmtlib` renders `EStr` (additive).
- `compiler/parse_body.ev` — `MembershipStep` peels up to 9 tokens,
  classifies the RHS shape (string / unary-not / ternary / binop / bare
  atom), builds the matching `Expr`, and composes all four renderers
  (`ArithExprSmtlib`/`BoolExprSmtlib`/`TernaryExprSmtlib`/`AtomSmtlib`) on
  it; carry/is_first_tick field exclusion; `saw_ift` output.
- `compiler/compiler.ev` — string-lit lexer mode, `is_first_tick`
  injection, empty-field skip, new imports.
- `compiler/lexer.ev`, `compiler/parser.ev` — the AST additions above.
- `tests/kernel/test_compiler_driver_{ternary,comparisons,bool_ops,strings,state_carry}.ev`
  — five new fixtures.

## Footgun confirmed harmless (wave-1 finding refined)

Wave 1 warned that claim composition leaks a callee's body-local names.
Refinement proven this session: **distinct composition SITES are
α-renamed independently** (test_translate_arith.ev composes
`ArithExprSmtlib` 3× on different inputs and works). The hazard is only a
**caller-declared** top-level name colliding with a composed callee's
body-local. So MembershipStep can compose four renderers on the same
`ms_rhs_expr` safely — all its own scratch stays `ms_`-prefixed. (The
first attempt failed for an unrelated reason: `OpLe`/`OpGe`/… already
exist as `Token` variants and enum variant names are globally unique.)

## Verification

- `./test.sh`: **all phases passed**.
- Kernel tests: **81 (was 76), 0 failed**, green under default /
  `EVIDENT_FUNCTIONIZE=0` / `EVIDENT_FUNCTIONIZE_JIT=1`.
- Wave-1 + MVP fixtures (mvp, readfile, multi_member, arith) emit
  byte-identical output — MembershipStep / BoolExprSmtlib changes are
  additive for the wave-1 shapes.
- `compiler/compiler.ev` (reading from disk) emits correct `.smt2` for a
  mixed body (`b ∈ Bool = x ∧ y` + `s ∈ String = "hi"`), ternary, and
  state-carry.
- Functionizer line: all-residual (string-heavy match/ternary shapes
  don't extract — same class as the wave-1 / MVP driver). Runs
  correctly; not silently broken.

## No frozen files touched

No `bootstrap/`, no `kernel/`, no `stdlib/`, no Python. Diff is
`compiler/*.ev` + five new `tests/kernel/*.ev` + this doc.

## Out of scope (wave 3+)

Enum decls + match + `matches`, `Seq`/`⟨⟩`/`++`/`#`, `substr`/string ops,
claim composition (`↦`/`..`), `effects`/`LibCall`/`Exit` as enum-payload
shapes, `import` resolution — the structural self-hosting core.
