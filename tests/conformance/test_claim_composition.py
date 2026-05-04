"""
Conformance tests for claim composition as a constraint.

Three forms:
  1. Bare:    ClaimName               — names-match composition
  2. Mapped:  ClaimName (x mapsto y)  — with variable renaming
  3. Passthrough: ..ClaimName         — flat mixin at body level
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

RANGE = """
claim InRange
    lo ∈ Nat
    hi ∈ Nat
    n  ∈ Nat
    n ≥ lo
    n ≤ hi
"""

CONTAINS_BANG = """
claim ContainsBang
    text ∈ String
    text ∋ "!"
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
    src = POSITIVE + CONTAINS_BANG + """
type T
    n    ∈ Nat
    text ∈ String
    IsPositive
    ContainsBang
"""
    assert_sat(query(src, 'T', {'n': 1, 'text': 'hello!'}))
    assert_unsat(query(src, 'T', {'n': 0, 'text': 'hello!'}))
    assert_unsat(query(src, 'T', {'n': 1, 'text': 'hello'}))


# ---------------------------------------------------------------------------
# 2. Mapped form:  ClaimName (x mapsto y)
# ---------------------------------------------------------------------------

def test_mapped_renames_variable_sat():
    src = CONTAINS_BANG + """
type T
    greeting ∈ String
    greeting ∋ "hi" ⇒ ContainsBang(text mapsto greeting)
"""
    assert_sat(query(src, 'T', {'greeting': 'hi!'}))

def test_mapped_renames_variable_unsat():
    src = CONTAINS_BANG + """
type T
    greeting ∈ String
    greeting ∋ "hi" ⇒ ContainsBang(text mapsto greeting)
"""
    assert_unsat(query(src, 'T', {'greeting': 'hi'}))

def test_mapped_vacuous_when_antecedent_false():
    src = CONTAINS_BANG + """
type T
    greeting ∈ String
    greeting ∋ "hi" ⇒ ContainsBang(text mapsto greeting)
"""
    # greeting doesn't contain "hi" — ContainsBang not enforced
    assert_sat(query(src, 'T', {'greeting': 'bye'}))

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
    src = CONTAINS_BANG + """
type T
    msg ∈ String
    ContainsBang(text mapsto msg)
"""
    assert_sat(query(src, 'T', {'msg': 'hello!'}))
    assert_unsat(query(src, 'T', {'msg': 'hello'}))


# ---------------------------------------------------------------------------
# 3. Passthrough form:  ..ClaimName  as a body line
# ---------------------------------------------------------------------------

def test_passthrough_unconditional_sat():
    src = CONTAINS_BANG + """
type T
    text ∈ String
    ..ContainsBang
"""
    assert_sat(query(src, 'T', {'text': 'hi!'}))

def test_passthrough_unconditional_unsat():
    src = CONTAINS_BANG + """
type T
    text ∈ String
    ..ContainsBang
"""
    assert_unsat(query(src, 'T', {'text': 'hi'}))

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
    src = POSITIVE + CONTAINS_BANG + """
type T
    n    ∈ Nat
    text ∈ String
    ..IsPositive
    ..ContainsBang
"""
    assert_sat(query(src, 'T', {'n': 3, 'text': 'wow!'}))
    assert_unsat(query(src, 'T', {'n': 0, 'text': 'wow!'}))
    assert_unsat(query(src, 'T', {'n': 3, 'text': 'wow'}))


# ---------------------------------------------------------------------------
# Integration: claim composition in a realistic dispatch pattern
# ---------------------------------------------------------------------------

def test_dispatch_via_claim_consequent():
    """Verb-dispatch pattern: only the matching verb's claim is enforced."""
    src = """
type Verb = Add | Remove

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
    verb       ∈ Verb
    budget     ∈ Nat
    new_budget ∈ Nat
    amount     ∈ Nat

    verb = Add    ⇒ AddsBudget
    verb = Remove ⇒ RemovesBudget
"""
    # Add: 10 + 5 = 15
    b = assert_sat(query(src, 'BudgetStep', {'verb': 'Add', 'budget': 10, 'amount': 5}))
    assert b['new_budget'] == 15

    # Remove: 10 - 3 = 7
    b = assert_sat(query(src, 'BudgetStep', {'verb': 'Remove', 'budget': 10, 'amount': 3}))
    assert b['new_budget'] == 7

    # Add: wrong result
    assert_unsat(query(src, 'BudgetStep', {'verb': 'Add', 'budget': 10, 'amount': 5, 'new_budget': 14}))
