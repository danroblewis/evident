"""
Tests for runtime/src/optimize.py — Phase 13: Performance optimizations.

Covers:
  - infer_domain: bounds extraction from schema bodies
  - should_unroll_quantifier: heuristic for quantifier unrolling
  - add_timeout / check_with_timeout: solver timeout helpers
"""

import pytest
import z3

from runtime.src.optimize import (
    infer_domain,
    should_unroll_quantifier,
    add_timeout,
    check_with_timeout,
)
from runtime.src.ast_types import (
    SchemaDecl,
    ArithmeticConstraint,
    MembershipConstraint,
    Identifier,
    NatLiteral,
    SetLiteral,
    RangeLiteral,
)


# ---------------------------------------------------------------------------
# Helpers to build minimal AST nodes
# ---------------------------------------------------------------------------


def make_schema(*body_items) -> SchemaDecl:
    """Build a SchemaDecl with an empty param list and the given body items."""
    return SchemaDecl(keyword="schema", name="test", params=[], body=list(body_items))


def arith(op, var_name: str, value: int) -> ArithmeticConstraint:
    """Shorthand: <var_name> <op> <value>."""
    return ArithmeticConstraint(op=op, left=Identifier(var_name), right=NatLiteral(value))


def membership_in_nat(var_name: str) -> MembershipConstraint:
    """Shorthand: <var_name> ∈ Nat."""
    return MembershipConstraint(op="∈", left=Identifier(var_name), right=Identifier("Nat"))


# ---------------------------------------------------------------------------
# infer_domain tests
# ---------------------------------------------------------------------------


class TestInferDomain:
    def test_bounds_from_strict_inequalities(self):
        """n > 5 and n < 10 should give lower=6, upper=9."""
        schema = make_schema(arith(">", "n", 5), arith("<", "n", 10))
        domain = infer_domain(schema)
        assert "n" in domain
        lo, hi = domain["n"]
        assert lo == 6
        assert hi == 9

    def test_nat_membership_only(self):
        """n ∈ Nat alone gives lower=0, upper=None (no upper bound)."""
        schema = make_schema(membership_in_nat("n"))
        domain = infer_domain(schema)
        assert "n" in domain
        lo, hi = domain["n"]
        assert lo == 0
        assert hi is None

    def test_no_numeric_vars(self):
        """A schema with no arithmetic constraints returns an empty dict."""
        schema = make_schema()
        domain = infer_domain(schema)
        assert domain == {}

    def test_inclusive_bounds(self):
        """n >= 3 and n <= 7 should give lower=3, upper=7 (inclusive)."""
        schema = make_schema(arith("≥", "n", 3), arith("≤", "n", 7))
        domain = infer_domain(schema)
        lo, hi = domain["n"]
        assert lo == 3
        assert hi == 7

    def test_nat_membership_then_upper_bound(self):
        """n ∈ Nat plus n < 50 should merge to lower=0, upper=49."""
        schema = make_schema(membership_in_nat("n"), arith("<", "n", 50))
        domain = infer_domain(schema)
        lo, hi = domain["n"]
        assert lo == 0
        assert hi == 49

    def test_multiple_variables(self):
        """Constraints on two different variables are tracked independently."""
        schema = make_schema(
            arith(">", "x", 0),
            arith("<", "x", 10),
            arith("≥", "y", 5),
        )
        domain = infer_domain(schema)
        assert domain["x"] == (1, 9)
        lo_y, hi_y = domain["y"]
        assert lo_y == 5
        assert hi_y is None


# ---------------------------------------------------------------------------
# should_unroll_quantifier tests
# ---------------------------------------------------------------------------


class TestShouldUnrollQuantifier:
    def test_small_set_literal_unrolls(self):
        """A SetLiteral with 5 elements should be unrolled."""
        elements = [NatLiteral(i) for i in range(5)]
        assert should_unroll_quantifier(SetLiteral(elements=elements)) is True

    def test_large_set_literal_no_unroll(self):
        """A SetLiteral with 1001 elements exceeds the default bound → False."""
        elements = [NatLiteral(i) for i in range(1001)]
        assert should_unroll_quantifier(SetLiteral(elements=elements)) is False

    def test_small_range_unrolls(self):
        """RangeLiteral(0, 99) has 100 elements — should unroll."""
        r = RangeLiteral(from_=NatLiteral(0), to=NatLiteral(99))
        assert should_unroll_quantifier(r) is True

    def test_large_range_no_unroll(self):
        """RangeLiteral(0, 2000) has 2001 elements — should not unroll."""
        r = RangeLiteral(from_=NatLiteral(0), to=NatLiteral(2000))
        assert should_unroll_quantifier(r) is False

    def test_symbolic_identifier_no_unroll(self):
        """An Identifier (symbolic set) should never be unrolled."""
        assert should_unroll_quantifier(Identifier("some_symbolic_set")) is False

    def test_exactly_at_bound_unrolls(self):
        """A SetLiteral with exactly domain_bound elements should still unroll."""
        elements = [NatLiteral(i) for i in range(1000)]
        assert should_unroll_quantifier(SetLiteral(elements=elements)) is True

    def test_custom_domain_bound(self):
        """Passing a custom domain_bound of 10 rejects a 15-element set."""
        elements = [NatLiteral(i) for i in range(15)]
        assert should_unroll_quantifier(SetLiteral(elements=elements), domain_bound=10) is False

    def test_single_element_set_unrolls(self):
        """A single-element SetLiteral trivially unrolls."""
        assert should_unroll_quantifier(SetLiteral(elements=[NatLiteral(42)])) is True

    def test_empty_set_literal_unrolls(self):
        """An empty SetLiteral (0 elements) should also unroll."""
        assert should_unroll_quantifier(SetLiteral(elements=[])) is True


# ---------------------------------------------------------------------------
# add_timeout tests
# ---------------------------------------------------------------------------


class TestAddTimeout:
    def test_add_timeout_does_not_raise(self):
        """add_timeout should set the parameter without raising an exception."""
        s = z3.Solver()
        add_timeout(s, ms=3000)  # no exception expected

    def test_add_timeout_custom_value(self):
        """add_timeout with various ms values should not raise."""
        s = z3.Solver()
        for ms in (0, 1, 100, 10_000, 60_000):
            add_timeout(s, ms=ms)


# ---------------------------------------------------------------------------
# check_with_timeout tests
# ---------------------------------------------------------------------------


class TestCheckWithTimeout:
    def test_sat_simple(self):
        """A trivially satisfiable solver returns sat."""
        s = z3.Solver()
        x = z3.Int("x")
        s.add(x > 0)
        result = check_with_timeout(s, timeout_ms=5000)
        assert result == z3.sat

    def test_unsat_simple(self):
        """A trivially unsatisfiable solver returns unsat."""
        s = z3.Solver()
        x = z3.Int("x")
        s.add(x > 5)
        s.add(x < 3)
        result = check_with_timeout(s, timeout_ms=5000)
        assert result == z3.unsat

    def test_returns_valid_result(self):
        """check_with_timeout returns one of the three valid Z3 results."""
        s = z3.Solver()
        result = check_with_timeout(s, timeout_ms=100)
        assert result in (z3.sat, z3.unsat, z3.unknown)

    def test_timeout_parameter_accepted(self):
        """Passing timeout_ms does not raise even with an extreme value."""
        s = z3.Solver()
        x = z3.Int("x")
        s.add(x == 1)
        result = check_with_timeout(s, timeout_ms=1)
        # Result is sat or unknown depending on machine speed — just verify it's valid.
        assert result in (z3.sat, z3.unsat, z3.unknown)
