"""
Conformance tests for:
  - subclaim keyword: nested claim definitions scoped to a parent claim
  - ⟸ reverse implication: A ⟸ B means B ⇒ A
"""

import pytest
from .conftest import query, assert_sat, assert_unsat


# ---------------------------------------------------------------------------
# 1. ⟸ reverse implication
# ---------------------------------------------------------------------------

def test_reverse_implies_basic_sat():
    """A ⟸ B is B ⇒ A. When B is true, A must hold."""
    src = """
claim Foo
    x ∈ Nat
    y ∈ Nat
    x > 0 ⟸ y = 1
"""
    # y=1 triggers x > 0
    assert_sat(query(src, 'Foo', {'x': 5, 'y': 1}))


def test_reverse_implies_basic_unsat():
    """When B is true and A fails, UNSAT."""
    src = """
claim Foo
    x ∈ Nat
    y ∈ Nat
    x > 0 ⟸ y = 1
"""
    # y=1 triggers x > 0, but x=0 fails it
    assert_unsat(query(src, 'Foo', {'x': 0, 'y': 1}))


def test_reverse_implies_vacuous():
    """When B is false, A is not enforced."""
    src = """
claim Foo
    x ∈ Nat
    y ∈ Nat
    x > 0 ⟸ y = 1
"""
    # y≠1, so x > 0 is not enforced — x=0 is fine
    assert_sat(query(src, 'Foo', {'x': 0, 'y': 2}))


def test_reverse_implies_with_claim_composition():
    """A claim name as consequent with ⟸: ClaimName ⟸ condition."""
    src = """
claim IsPositive
    n ∈ Nat
    n > 0

claim Wrapper
    n    ∈ Nat
    mode ∈ String
    IsPositive ⟸ mode = "strict"
"""
    # mode=strict ⇒ IsPositive enforced
    assert_sat(query(src, 'Wrapper', {'n': 5, 'mode': 'strict'}))
    assert_unsat(query(src, 'Wrapper', {'n': 0, 'mode': 'strict'}))
    # mode≠strict ⇒ not enforced
    assert_sat(query(src, 'Wrapper', {'n': 0, 'mode': 'loose'}))


# ---------------------------------------------------------------------------
# 2. subclaim: basic behaviour
# ---------------------------------------------------------------------------

def test_subclaim_body_enforced_when_triggered():
    """Subclaim constraints fire when the triggering condition holds."""
    src = """
claim Outer
    x ∈ Nat
    y ∈ Nat

    subclaim MustBePositive
        x > 0

    MustBePositive ⟸ y = 1
"""
    # y=1 ⇒ MustBePositive ⇒ x > 0
    assert_sat(query(src, 'Outer', {'x': 5, 'y': 1}))
    assert_unsat(query(src, 'Outer', {'x': 0, 'y': 1}))


def test_subclaim_not_enforced_when_not_triggered():
    """Subclaim constraints are vacuous when the condition is false."""
    src = """
claim Outer
    x ∈ Nat
    y ∈ Nat

    subclaim MustBePositive
        x > 0

    MustBePositive ⟸ y = 1
"""
    # y≠1 ⇒ MustBePositive not enforced — x=0 OK
    assert_sat(query(src, 'Outer', {'x': 0, 'y': 2}))


def test_subclaim_internal_variable():
    """Subclaim can declare internal variables not in the parent scope."""
    src = """
claim Outer
    x ∈ Nat
    result ∈ String

    subclaim Classify
        tmp ∈ Nat
        tmp = x + 1
        tmp > 5
        result = "big"

    Classify ⟸ x > 4
"""
    # x=5: 5 > 4 ⇒ Classify: tmp=6, 6>5 ✓, result="big"
    b = assert_sat(query(src, 'Outer', {'x': 5}))
    assert b.get('result') == 'big'


def test_subclaim_internal_variable_not_leaked():
    """Internal subclaim variables are not visible in parent scope."""
    src = """
claim Outer
    x ∈ Nat

    subclaim Inner
        internal ∈ Nat
        internal = x + 10

    Inner ⟸ x = 1
"""
    # Should be SAT; `internal` is private to Inner
    b = assert_sat(query(src, 'Outer', {'x': 1}))
    # internal is not a top-level binding
    assert 'internal' not in b


def test_subclaim_inherits_parent_variables():
    """Subclaim can reference parent-declared variables by name."""
    src = """
claim Outer
    a ∈ Nat
    b ∈ Nat

    subclaim SumCheck
        a + b > 10

    SumCheck ⟸ a = 3
"""
    # a=3 ⇒ SumCheck: a+b > 10 ⇒ b > 7
    assert_sat(query(src, 'Outer', {'a': 3, 'b': 8}))
    assert_unsat(query(src, 'Outer', {'a': 3, 'b': 7}))


# ---------------------------------------------------------------------------
# 3. subclaim used unconditionally
# ---------------------------------------------------------------------------

def test_subclaim_unconditional_use():
    """A subclaim referenced without ⟸ (bare form) is always enforced."""
    src = """
claim Outer
    x ∈ Nat
    y ∈ Nat

    subclaim BothPositive
        x > 0
        y > 0

    BothPositive
"""
    assert_sat(query(src, 'Outer', {'x': 1, 'y': 1}))
    assert_unsat(query(src, 'Outer', {'x': 0, 'y': 1}))
    assert_unsat(query(src, 'Outer', {'x': 1, 'y': 0}))


# ---------------------------------------------------------------------------
# 4. Realistic dispatch pattern with subclaims + ⟸
# ---------------------------------------------------------------------------

def test_dispatch_with_subclaims():
    """Verb-dispatch via subclaims: only the matching branch fires."""
    src = """
type Op = Add | Sub

claim Calculator
    op     ∈ Op
    a      ∈ Nat
    b      ∈ Nat
    result ∈ Nat

    subclaim DoAdd
        result = a + b

    subclaim DoSub
        result = a - b

    DoAdd ⟸ op = Add
    DoSub ⟸ op = Sub
"""
    # Add: 3 + 4 = 7
    b = assert_sat(query(src, 'Calculator', {'op': 'Add', 'a': 3, 'b': 4}))
    assert b['result'] == 7

    # Sub: 10 - 3 = 7
    b = assert_sat(query(src, 'Calculator', {'op': 'Sub', 'a': 10, 'b': 3}))
    assert b['result'] == 7

    # Wrong result
    assert_unsat(query(src, 'Calculator', {'op': 'Add', 'a': 3, 'b': 4, 'result': 8}))
    assert_unsat(query(src, 'Calculator', {'op': 'Sub', 'a': 10, 'b': 3, 'result': 8}))
