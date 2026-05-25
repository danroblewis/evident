"""
Conformance tests for claim composition as a constraint.

Three forms:
  1. Bare:    ClaimName               — names-match composition
  2. Mapped:  ClaimName (x mapsto y)  — with variable renaming
  3. Passthrough: ..ClaimName         — flat mixin at body level

Notes on what is intentionally NOT covered here:
  * `cond ⇒ ClaimName(x mapsto y)` — implies-RHS does not currently parse a
    claim-call with `mapsto`. The body-item parser recognises mapsto-call but
    the expression parser used inside an implies RHS does not. See
    `examples/COUNTEREXAMPLES.md` "Conformance gaps surfaced by triage".
  * `verb ∈ Verb` (enum) `--given` from the CLI — the CLI infers `Add` as
    a string, and `run_cached` rejects `(Var::EnumVar, Value::Str)`. The
    dispatch test below uses Bool dispatch instead. Same COUNTEREXAMPLES file.
  * `text ∋ "!"` — string substring membership now translates (session
    GAPC lowered it to Z3 `str.contains`); see `test_string_ops.py`.
    The string-equality claim below is kept as-is since it still
    exercises the composition shape these tests target.
"""

import pytest
from .conftest import query, assert_sat, assert_unsat

# ---------------------------------------------------------------------------
# Shared helpers
# ---------------------------------------------------------------------------

POSITIVE = """
claim IsPositive
    n ∈ Nat
    n > 0
"""

UNDER_TEN = """
claim UnderTen
    n ∈ Nat
    n < 10
"""

RANGE = """
claim InRange
    lo ∈ Nat
    hi ∈ Nat
    n  ∈ Nat
    n ≥ lo
    n ≤ hi
"""

# String-equality claim exercising the composition shape. (`text ∋ "!"`
# substring membership now translates — see test_string_ops.py — but this
# string-equality form is what these composition tests were rewritten to,
# and it still covers the shape under test.)
GREETS_HI = """
claim GreetsHi
    text ∈ String
    text = "hi"
"""


# ---------------------------------------------------------------------------
# 1. Bare form:  ClaimName  in a body (unconditional)
# ---------------------------------------------------------------------------

def test_bare_unconditional_sat():
    src = POSITIVE + """
type T
    n ∈ Nat
    IsPositive
"""
    assert_sat(query(src, 'T', {'n': 5}))

def test_bare_unconditional_unsat():
    src = POSITIVE + """
type T
    n ∈ Nat
    IsPositive
"""
    assert_unsat(query(src, 'T', {'n': 0}))

def test_bare_as_implies_consequent_sat():
    src = POSITIVE + """
type T
    mode ∈ String
    n    ∈ Nat
    mode = "strict" ⇒ IsPositive
"""
    assert_sat(query(src, 'T', {'mode': 'strict', 'n': 3}))

def test_bare_as_implies_consequent_unsat():
    src = POSITIVE + """
type T
    mode ∈ String
    n    ∈ Nat
    mode = "strict" ⇒ IsPositive
"""
    assert_unsat(query(src, 'T', {'mode': 'strict', 'n': 0}))

def test_bare_implies_vacuous_when_condition_false():
    """When the antecedent is false, the claim is not enforced."""
    src = POSITIVE + """
type T
    mode ∈ String
    n    ∈ Nat
    mode = "strict" ⇒ IsPositive
"""
    # mode is "loose" — IsPositive not enforced, n=0 is fine
    assert_sat(query(src, 'T', {'mode': 'loose', 'n': 0}))

def test_bare_multi_claim_conjunction():
    """Multiple bare claims in a body are all enforced simultaneously."""
    src = POSITIVE + UNDER_TEN + """
type T
    n ∈ Nat
    IsPositive
    UnderTen
"""
    assert_sat(query(src, 'T', {'n': 5}))
    assert_unsat(query(src, 'T', {'n': 0}))     # IsPositive fails
    assert_unsat(query(src, 'T', {'n': 10}))    # UnderTen fails


# ---------------------------------------------------------------------------
# 2. Mapped form:  ClaimName (x mapsto y)
# ---------------------------------------------------------------------------

def test_mapped_renames_variable_sat():
    src = GREETS_HI + """
type T
    greeting ∈ String
    GreetsHi(text mapsto greeting)
"""
    assert_sat(query(src, 'T', {'greeting': 'hi'}))

def test_mapped_renames_variable_unsat():
    src = GREETS_HI + """
type T
    greeting ∈ String
    GreetsHi(text mapsto greeting)
"""
    assert_unsat(query(src, 'T', {'greeting': 'bye'}))

def test_mapped_multi_variable_claim():
    src = RANGE + """
type T
    value ∈ Nat
    low   ∈ Nat
    high  ∈ Nat
    InRange(n mapsto value, lo mapsto low, hi mapsto high)
"""
    assert_sat(query(src, 'T', {'value': 5, 'low': 1, 'high': 10}))
    assert_unsat(query(src, 'T', {'value': 0, 'low': 1, 'high': 10}))
    assert_unsat(query(src, 'T', {'value': 11, 'low': 1, 'high': 10}))

def test_mapped_unconditional():
    """Mapped form without an implies — always enforced."""
    src = GREETS_HI + """
type T
    msg ∈ String
    GreetsHi(text mapsto msg)
"""
    assert_sat(query(src, 'T', {'msg': 'hi'}))
    assert_unsat(query(src, 'T', {'msg': 'bye'}))


# ---------------------------------------------------------------------------
# 3. Passthrough form:  ..ClaimName  as a body line
# ---------------------------------------------------------------------------

def test_passthrough_unconditional_sat():
    src = GREETS_HI + """
type T
    text ∈ String
    ..GreetsHi
"""
    assert_sat(query(src, 'T', {'text': 'hi'}))

def test_passthrough_unconditional_unsat():
    src = GREETS_HI + """
type T
    text ∈ String
    ..GreetsHi
"""
    assert_unsat(query(src, 'T', {'text': 'bye'}))

def test_passthrough_uses_names_match():
    """..ClaimName unifies on variable names — no explicit mapping needed."""
    src = POSITIVE + """
type T
    n ∈ Nat
    ..IsPositive
"""
    assert_sat(query(src, 'T', {'n': 7}))
    assert_unsat(query(src, 'T', {'n': 0}))

def test_passthrough_multiple_claims():
    src = POSITIVE + UNDER_TEN + """
type T
    n ∈ Nat
    ..IsPositive
    ..UnderTen
"""
    assert_sat(query(src, 'T', {'n': 5}))
    assert_unsat(query(src, 'T', {'n': 0}))     # IsPositive fails
    assert_unsat(query(src, 'T', {'n': 10}))    # UnderTen fails


# ---------------------------------------------------------------------------
# Integration: claim composition in a realistic dispatch pattern
# ---------------------------------------------------------------------------

def test_dispatch_via_claim_consequent():
    """Conditional-dispatch pattern: only the matching branch's claim is
    enforced. Uses Bool dispatch rather than enum dispatch — the CLI's
    `--given verb=Add` does not pin enum-typed givens (see COUNTEREXAMPLES).
    The shape under test (`cond ⇒ ClaimName`) is identical regardless."""
    src = """
claim AddsBudget
    budget     ∈ Nat
    new_budget ∈ Nat
    amount     ∈ Nat
    new_budget = budget + amount

claim RemovesBudget
    budget     ∈ Nat
    new_budget ∈ Nat
    amount     ∈ Nat
    new_budget = budget - amount

type BudgetStep
    is_add     ∈ Bool
    budget     ∈ Nat
    new_budget ∈ Nat
    amount     ∈ Nat

    is_add    ⇒ AddsBudget
    (¬is_add) ⇒ RemovesBudget
"""
    # Add: 10 + 5 = 15
    # Note: pass `'true'` / `'false'` lowercase — the CLI's `infer_value`
    # parser only accepts lowercase bool literals; Python's `True` would
    # f-string to `"True"` and fall through to a string-typed given.
    b = assert_sat(query(src, 'BudgetStep', {'is_add': 'true', 'budget': 10, 'amount': 5}))
    assert b['new_budget'] == 15

    # Remove: 10 - 3 = 7
    b = assert_sat(query(src, 'BudgetStep', {'is_add': 'false', 'budget': 10, 'amount': 3}))
    assert b['new_budget'] == 7

    # Add: wrong result pinned
    assert_unsat(query(src, 'BudgetStep',
                       {'is_add': 'true', 'budget': 10, 'amount': 5, 'new_budget': 14}))


# ---------------------------------------------------------------------------
# 4. Sub-schema usage:  `instance ∈ UserType` inherits the type's body
# ---------------------------------------------------------------------------
#
# CLAUDE.md states "the type's invariants are automatically enforced" when a
# claim contains `variable ∈ TypeName`. These tests pin that contract: body
# Constraints in the type definition fire on the instance with field
# references prefixed by the instance name.

def test_subschema_inherits_primitive_derived_field():
    """`Foo(p ∈ Int)` with body `derived = p + 1`; instance must satisfy."""
    src = """
type Foo(p ∈ Int)
    derived ∈ Int = p + 1

claim SatDerivedCorrect
    f ∈ Foo (p mapsto 5)
    f.derived = 6

claim UnsatDerivedOverridden
    f ∈ Foo (p mapsto 5)
    f.derived = 99
"""
    assert_sat(query(src, 'SatDerivedCorrect'))
    assert_unsat(query(src, 'UnsatDerivedOverridden'))


def test_subschema_inherits_record_arithmetic():
    """Type body uses record arithmetic; the lift fires after the prefix
    rewrite so `offset_pos = pos + IVec2(10, 20)` becomes
    `f.offset_pos.x = f.pos.x + 10 ∧ f.offset_pos.y = f.pos.y + 20`."""
    src = """
type IVec2(x, y ∈ Int)

type Sprite(pos ∈ IVec2)
    offset_pos ∈ IVec2 = pos + IVec2(10, 20)

claim SatOffsetCorrect
    s ∈ Sprite
    s.pos.x = 5
    s.pos.y = 7
    s.offset_pos.x = 15
    s.offset_pos.y = 27

claim UnsatOffsetXWrong
    s ∈ Sprite
    s.pos.x = 5
    s.pos.y = 7
    s.offset_pos.x = 99
"""
    assert_sat(query(src, 'SatOffsetCorrect'))
    assert_unsat(query(src, 'UnsatOffsetXWrong'))


def test_subschema_shadowing_by_quantifier_bound():
    """A quantifier-bound name inside the type body shadows a same-named
    field — body uses of the bound name shouldn't get the instance prefix."""
    src = """
type Counter(n ∈ Int)
    all_pos ∈ Bool = (∀ i ∈ {0..2} : i ≥ 0)

claim SatAllPos
    c ∈ Counter (n mapsto 7)
    c.all_pos = true
"""
    # The quantifier introduces `i`. The body of the forall just references
    # `i ≥ 0` — `i` is bound, not a field. The rewrite must NOT prefix it.
    # If it did, translation would fail (env has no `c.i`).
    assert_sat(query(src, 'SatAllPos'))


def test_subschema_inheritance_unrelated_external_name_untouched():
    """An identifier in the type body that doesn't name a field of the type
    is left as-is and resolves against the outer scope. Verifies the rewrite
    is field-set-driven, not blanket-prefix."""
    src = """
claim Outer
    shared_const ∈ Int = 42

type Holder(local ∈ Int)
    -- `shared_const` is NOT a field of Holder; should resolve to the
    -- caller's scope. `local` IS a field; gets prefixed.
    sum ∈ Int = local + shared_const

claim SatSum
    ..Outer
    h ∈ Holder (local mapsto 8)
    h.sum = 50
"""
    assert_sat(query(src, 'SatSum'))
