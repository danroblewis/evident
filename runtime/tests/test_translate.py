"""
Tests for runtime/src/translate.py — Phase 3: Basic constraint translation.

Every test calls Z3 and verifies satisfiability / unsatisfiability of the
translated assertions.
"""

import pytest
import z3

from runtime.src.translate import translate_constraint, translate_expr
from runtime.src.env import Environment
from runtime.src.sorts import SortRegistry
from runtime.src.ast_types import (
    ArithmeticConstraint,
    MembershipConstraint,
    LogicConstraint,
    BindingConstraint,
    Identifier,
    NatLiteral,
    IntLiteral,
    RealLiteral,
    StringLiteral,
    BoolLiteral,
    BinaryExpr,
    UnaryExpr,
    TupleLiteral,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def make_env_registry():
    """Return a fresh (env, registry) pair with an isolated Z3 context."""
    registry = SortRegistry()
    env = Environment()
    return env, registry


def sat_with(assertions):
    """Return True if the conjunction of assertions is satisfiable."""
    s = z3.Solver()
    s.add(*assertions)
    return s.check() == z3.sat


def unsat_with(assertions):
    """Return True if the conjunction of assertions is unsatisfiable."""
    s = z3.Solver()
    s.add(*assertions)
    return s.check() == z3.unsat


# ---------------------------------------------------------------------------
# translate_expr tests
# ---------------------------------------------------------------------------


class TestTranslateExpr:
    def test_nat_literal(self):
        env, reg = make_env_registry()
        result = translate_expr(NatLiteral(42), env, reg)
        assert z3.is_int_value(result)
        assert result.as_long() == 42

    def test_int_literal(self):
        env, reg = make_env_registry()
        result = translate_expr(IntLiteral(-7), env, reg)
        assert z3.is_int_value(result)
        assert result.as_long() == -7

    def test_real_literal(self):
        env, reg = make_env_registry()
        result = translate_expr(RealLiteral(3.14), env, reg)
        assert z3.is_rational_value(result)

    def test_string_literal(self):
        env, reg = make_env_registry()
        result = translate_expr(StringLiteral("hello"), env, reg)
        # Z3 string value — just check it is a string expression
        assert result.sort() == z3.StringSort()

    def test_bool_literal_true(self):
        env, reg = make_env_registry()
        result = translate_expr(BoolLiteral(True), env, reg)
        assert z3.is_true(result)

    def test_bool_literal_false(self):
        env, reg = make_env_registry()
        result = translate_expr(BoolLiteral(False), env, reg)
        assert z3.is_false(result)

    def test_identifier_lookup(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        result = translate_expr(Identifier("x"), env, reg)
        assert z3.eq(result, x)

    def test_identifier_missing_raises(self):
        env, reg = make_env_registry()
        with pytest.raises(KeyError):
            translate_expr(Identifier("missing"), env, reg)

    def test_binary_add(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        expr = BinaryExpr("+", Identifier("x"), NatLiteral(3))
        result = translate_expr(expr, env, reg)
        # Check: x + 3 == 8 when x == 5
        s = z3.Solver()
        s.add(x == 5)
        s.add(result == 8)
        assert s.check() == z3.sat

    def test_binary_sub(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        expr = BinaryExpr("-", Identifier("x"), NatLiteral(3))
        result = translate_expr(expr, env, reg)
        s = z3.Solver()
        s.add(x == 10)
        s.add(result == 7)
        assert s.check() == z3.sat

    def test_binary_mul(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        expr = BinaryExpr("*", Identifier("x"), NatLiteral(4))
        result = translate_expr(expr, env, reg)
        s = z3.Solver()
        s.add(x == 3)
        s.add(result == 12)
        assert s.check() == z3.sat

    def test_unary_not(self):
        env, reg = make_env_registry()
        b = z3.Bool("b")
        env = env.bind("b", b)
        expr = UnaryExpr("¬", Identifier("b"))
        result = translate_expr(expr, env, reg)
        # Not(b) should be true when b is false
        s = z3.Solver()
        s.add(b == False)
        s.add(result)
        assert s.check() == z3.sat
        # Not(b) should be false when b is true → unsat
        s2 = z3.Solver()
        s2.add(b == True)
        s2.add(result)
        assert s2.check() == z3.unsat

    def test_tuple_literal(self):
        env, reg = make_env_registry()
        expr = TupleLiteral([NatLiteral(1), BoolLiteral(True)])
        result = translate_expr(expr, env, reg)
        # Should produce a Z3 tuple expression without error
        assert result is not None
        assert "Tuple" in str(result.sort())

    def test_binary_div(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        expr = BinaryExpr("/", Identifier("x"), NatLiteral(2))
        result = translate_expr(expr, env, reg)
        # Integer division: 10 / 2 == 5
        s = z3.Solver()
        s.add(x == 10)
        s.add(result == 5)
        assert s.check() == z3.sat


# ---------------------------------------------------------------------------
# ArithmeticConstraint tests
# ---------------------------------------------------------------------------


class TestArithmeticConstraint:
    def test_eq_sat(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        # x = 5
        c = translate_constraint(
            ArithmeticConstraint("=", Identifier("x"), NatLiteral(5)), env, reg
        )
        assert sat_with([x == 5, c])

    def test_eq_unsat(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        # x = 5 AND x = 6 → unsat
        c = translate_constraint(
            ArithmeticConstraint("=", Identifier("x"), NatLiteral(5)), env, reg
        )
        assert unsat_with([x == 6, c])

    def test_neq(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        # x ≠ 5, x = 6 → sat
        c = translate_constraint(
            ArithmeticConstraint("≠", Identifier("x"), NatLiteral(5)), env, reg
        )
        assert sat_with([x == 6, c])
        # x ≠ 5, x = 5 → unsat
        assert unsat_with([x == 5, c])

    def test_leq_sat(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        # x ≤ 10, x = 3 → sat
        c = translate_constraint(
            ArithmeticConstraint("≤", Identifier("x"), NatLiteral(10)), env, reg
        )
        assert sat_with([x == 3, c])

    def test_leq_unsat(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        # x ≤ 10, x = 15 → unsat
        c = translate_constraint(
            ArithmeticConstraint("≤", Identifier("x"), NatLiteral(10)), env, reg
        )
        assert unsat_with([x == 15, c])

    def test_lt(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        c = translate_constraint(
            ArithmeticConstraint("<", Identifier("x"), NatLiteral(10)), env, reg
        )
        assert sat_with([x == 9, c])
        assert unsat_with([x == 10, c])

    def test_gt(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        c = translate_constraint(
            ArithmeticConstraint(">", Identifier("x"), NatLiteral(5)), env, reg
        )
        assert sat_with([x == 6, c])
        assert unsat_with([x == 5, c])

    def test_geq(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        c = translate_constraint(
            ArithmeticConstraint("≥", Identifier("x"), NatLiteral(5)), env, reg
        )
        assert sat_with([x == 5, c])
        assert sat_with([x == 10, c])
        assert unsat_with([x == 4, c])

    def test_literal_eq_literal(self):
        env, reg = make_env_registry()
        # 3 = 3 → always sat
        c = translate_constraint(
            ArithmeticConstraint("=", NatLiteral(3), NatLiteral(3)), env, reg
        )
        assert sat_with([c])

    def test_literal_neq_literal_unsat(self):
        env, reg = make_env_registry()
        # 3 = 4 → always unsat
        c = translate_constraint(
            ArithmeticConstraint("=", NatLiteral(3), NatLiteral(4)), env, reg
        )
        assert unsat_with([c])


# ---------------------------------------------------------------------------
# MembershipConstraint tests
# ---------------------------------------------------------------------------


class TestMembershipConstraint:
    def test_in_nat_positive(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        # x ∈ Nat → x ≥ 0
        c = translate_constraint(
            MembershipConstraint("∈", Identifier("x"), Identifier("Nat")), env, reg
        )
        # x = 5 satisfies x ∈ Nat
        assert sat_with([x == 5, c])

    def test_in_nat_negative_unsat(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        c = translate_constraint(
            MembershipConstraint("∈", Identifier("x"), Identifier("Nat")), env, reg
        )
        # x = -1 violates x ∈ Nat
        assert unsat_with([x == -1, c])

    def test_in_int_is_trivially_true(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        c = translate_constraint(
            MembershipConstraint("∈", Identifier("x"), Identifier("Int")), env, reg
        )
        # x ∈ Int always holds (no extra constraint)
        assert sat_with([x == -100, c])

    def test_in_bool_is_trivially_true(self):
        env, reg = make_env_registry()
        b = z3.Bool("b")
        env = env.bind("b", b)
        c = translate_constraint(
            MembershipConstraint("∈", Identifier("b"), Identifier("Bool")), env, reg
        )
        assert sat_with([c])

    def test_in_set(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        # Build a set S = {1, 2, 3} as a Z3 array
        S = z3.Array("S", z3.IntSort(), z3.BoolSort())
        env = env.bind("x", x).bind("S", S)
        # x ∈ S
        c = translate_constraint(
            MembershipConstraint("∈", Identifier("x"), Identifier("S")), env, reg
        )
        # Define S to contain 1, 2, 3
        s_def = z3.And(
            z3.Select(S, z3.IntVal(1)) == True,
            z3.Select(S, z3.IntVal(2)) == True,
            z3.Select(S, z3.IntVal(3)) == True,
            z3.Select(S, z3.IntVal(4)) == False,
        )
        # x = 2 ∈ S → sat
        assert sat_with([s_def, x == 2, c])
        # x = 4 ∈ S → unsat (4 not in S)
        assert unsat_with([s_def, x == 4, c])

    def test_not_in_set(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        S = z3.Array("S", z3.IntSort(), z3.BoolSort())
        env = env.bind("x", x).bind("S", S)
        # x ∉ S
        c = translate_constraint(
            MembershipConstraint("∉", Identifier("x"), Identifier("S")), env, reg
        )
        # S contains only 1
        s_def = z3.And(
            z3.Select(S, z3.IntVal(1)) == True,
            z3.Select(S, z3.IntVal(2)) == False,
        )
        # x = 2 ∉ S → sat
        assert sat_with([s_def, x == 2, c])
        # x = 1 ∉ S → unsat
        assert unsat_with([s_def, x == 1, c])

    def test_subset(self):
        env, reg = make_env_registry()
        A = z3.Array("A", z3.IntSort(), z3.BoolSort())
        B = z3.Array("B", z3.IntSort(), z3.BoolSort())
        env = env.bind("A", A).bind("B", B)
        # A ⊆ B
        c = translate_constraint(
            MembershipConstraint("⊆", Identifier("A"), Identifier("B")), env, reg
        )
        # A = {1}, B = {1, 2} → A ⊆ B → sat
        a_def = z3.And(
            z3.Select(A, z3.IntVal(1)) == True,
            z3.Select(A, z3.IntVal(2)) == False,
        )
        b_def = z3.And(
            z3.Select(B, z3.IntVal(1)) == True,
            z3.Select(B, z3.IntVal(2)) == True,
        )
        assert sat_with([a_def, b_def, c])

    def test_subset_violation_unsat(self):
        env, reg = make_env_registry()
        A = z3.Array("A", z3.IntSort(), z3.BoolSort())
        B = z3.Array("B", z3.IntSort(), z3.BoolSort())
        env = env.bind("A", A).bind("B", B)
        # A ⊆ B
        c = translate_constraint(
            MembershipConstraint("⊆", Identifier("A"), Identifier("B")), env, reg
        )
        # A = {1, 2}, B = {1} → A ⊄ B → unsat
        a_def = z3.And(
            z3.Select(A, z3.IntVal(1)) == True,
            z3.Select(A, z3.IntVal(2)) == True,
        )
        b_def = z3.And(
            z3.Select(B, z3.IntVal(1)) == True,
            z3.Select(B, z3.IntVal(2)) == False,
        )
        assert unsat_with([a_def, b_def, c])


# ---------------------------------------------------------------------------
# LogicConstraint tests
# ---------------------------------------------------------------------------


class TestLogicConstraint:
    def test_negation(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        # ¬(x = 5)
        inner = ArithmeticConstraint("=", Identifier("x"), NatLiteral(5))
        c = translate_constraint(LogicConstraint("¬", right=inner), env, reg)
        # x = 6 satisfies ¬(x = 5)
        assert sat_with([x == 6, c])
        # x = 5 violates ¬(x = 5)
        assert unsat_with([x == 5, c])

    def test_conjunction_sat(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        # x > 3 ∧ x < 10
        left = ArithmeticConstraint(">", Identifier("x"), NatLiteral(3))
        right = ArithmeticConstraint("<", Identifier("x"), NatLiteral(10))
        c = translate_constraint(LogicConstraint("∧", right=right, left=left), env, reg)
        # x = 5 satisfies both
        assert sat_with([x == 5, c])

    def test_conjunction_unsat(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        # x > 10 ∧ x < 5 → unsat (no such x)
        left = ArithmeticConstraint(">", Identifier("x"), NatLiteral(10))
        right = ArithmeticConstraint("<", Identifier("x"), NatLiteral(5))
        c = translate_constraint(LogicConstraint("∧", right=right, left=left), env, reg)
        assert unsat_with([c])

    def test_disjunction_sat(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        # x = 1 ∨ x = 2
        left = ArithmeticConstraint("=", Identifier("x"), NatLiteral(1))
        right = ArithmeticConstraint("=", Identifier("x"), NatLiteral(2))
        c = translate_constraint(LogicConstraint("∨", right=right, left=left), env, reg)
        assert sat_with([x == 1, c])
        assert sat_with([x == 2, c])

    def test_disjunction_unsat(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        # (x = 1 ∨ x = 2) ∧ x = 3 → unsat
        left = ArithmeticConstraint("=", Identifier("x"), NatLiteral(1))
        right = ArithmeticConstraint("=", Identifier("x"), NatLiteral(2))
        c = translate_constraint(LogicConstraint("∨", right=right, left=left), env, reg)
        assert unsat_with([x == 3, c])

    def test_implication_sat(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        # x > 5 ⇒ x > 3  (always true when x > 5)
        left = ArithmeticConstraint(">", Identifier("x"), NatLiteral(5))
        right = ArithmeticConstraint(">", Identifier("x"), NatLiteral(3))
        c = translate_constraint(LogicConstraint("⇒", right=right, left=left), env, reg)
        # x = 7 → 7 > 5 ⇒ 7 > 3 → True
        assert sat_with([x == 7, c])
        # x = 4 → 4 > 5 is False → implication True by vacuity
        assert sat_with([x == 4, c])

    def test_implication_violated(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        # x > 5 ⇒ x > 10, but x = 7 makes premise true and conclusion false
        left = ArithmeticConstraint(">", Identifier("x"), NatLiteral(5))
        right = ArithmeticConstraint(">", Identifier("x"), NatLiteral(10))
        c = translate_constraint(LogicConstraint("⇒", right=right, left=left), env, reg)
        assert unsat_with([x == 7, c])

    def test_nested_logic(self):
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)
        # ¬(x > 5 ∧ x < 3)  — the inner conjunction is always false, so ¬False = True
        inner_left = ArithmeticConstraint(">", Identifier("x"), NatLiteral(5))
        inner_right = ArithmeticConstraint("<", Identifier("x"), NatLiteral(3))
        inner = LogicConstraint("∧", right=inner_right, left=inner_left)
        c = translate_constraint(LogicConstraint("¬", right=inner), env, reg)
        assert sat_with([c])  # ¬False = True — always sat


# ---------------------------------------------------------------------------
# BindingConstraint tests
# ---------------------------------------------------------------------------


class TestBindingConstraint:
    def test_binding_eq_literal(self):
        env, reg = make_env_registry()
        y = z3.Int("y")
        env = env.bind("y", y)
        # y = 7
        c = translate_constraint(BindingConstraint("y", NatLiteral(7)), env, reg)
        s = z3.Solver()
        s.add(c)
        assert s.check() == z3.sat
        model = s.model()
        assert model[y].as_long() == 7

    def test_binding_unsat_when_contradicted(self):
        env, reg = make_env_registry()
        y = z3.Int("y")
        env = env.bind("y", y)
        # y = 7, but also assert y = 8 externally → unsat
        c = translate_constraint(BindingConstraint("y", NatLiteral(7)), env, reg)
        assert unsat_with([c, y == 8])

    def test_binding_with_expression(self):
        env, reg = make_env_registry()
        y = z3.Int("y")
        x = z3.Int("x")
        env = env.bind("y", y).bind("x", x)
        # y = x + 3
        c = translate_constraint(
            BindingConstraint("y", BinaryExpr("+", Identifier("x"), NatLiteral(3))),
            env, reg,
        )
        # x = 4 → y must be 7
        assert sat_with([c, x == 4, y == 7])
        assert unsat_with([c, x == 4, y == 8])

    def test_binding_missing_variable_raises(self):
        env, reg = make_env_registry()
        with pytest.raises(KeyError):
            translate_constraint(BindingConstraint("z", NatLiteral(1)), env, reg)


# ---------------------------------------------------------------------------
# Combined / multi-constraint tests
# ---------------------------------------------------------------------------


class TestCombinedConstraints:
    def test_nat_membership_and_upper_bound_sat(self):
        """x ∈ Nat, x > 5, x < 10 → sat (x could be 6..9)."""
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)

        nat_c = translate_constraint(
            MembershipConstraint("∈", Identifier("x"), Identifier("Nat")), env, reg
        )
        lower_c = translate_constraint(
            ArithmeticConstraint(">", Identifier("x"), NatLiteral(5)), env, reg
        )
        upper_c = translate_constraint(
            ArithmeticConstraint("<", Identifier("x"), NatLiteral(10)), env, reg
        )
        assert sat_with([nat_c, lower_c, upper_c])

    def test_nat_membership_and_bounds_unsat(self):
        """x ∈ Nat, x > 5, x < 3 → unsat."""
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)

        nat_c = translate_constraint(
            MembershipConstraint("∈", Identifier("x"), Identifier("Nat")), env, reg
        )
        lower_c = translate_constraint(
            ArithmeticConstraint(">", Identifier("x"), NatLiteral(5)), env, reg
        )
        upper_c = translate_constraint(
            ArithmeticConstraint("<", Identifier("x"), NatLiteral(3)), env, reg
        )
        assert unsat_with([nat_c, lower_c, upper_c])

    def test_negative_nat_unsat(self):
        """x ∈ Nat rules out x = -1."""
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)

        nat_c = translate_constraint(
            MembershipConstraint("∈", Identifier("x"), Identifier("Nat")), env, reg
        )
        assert unsat_with([nat_c, x == -1])

    def test_two_variables_constrained_together(self):
        """x ≤ y, y = 5 → x ≤ 5 is satisfiable for various x."""
        env, reg = make_env_registry()
        x = z3.Int("x")
        y = z3.Int("y")
        env = env.bind("x", x).bind("y", y)

        c1 = translate_constraint(
            ArithmeticConstraint("≤", Identifier("x"), Identifier("y")), env, reg
        )
        c2 = translate_constraint(
            ArithmeticConstraint("=", Identifier("y"), NatLiteral(5)), env, reg
        )
        assert sat_with([c1, c2, x == 3])
        assert sat_with([c1, c2, x == 5])
        assert unsat_with([c1, c2, x == 6])

    def test_disjunction_with_membership(self):
        """(x = 1 ∨ x = 2) ∧ x ∈ Nat → sat."""
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)

        disj = translate_constraint(
            LogicConstraint(
                "∨",
                right=ArithmeticConstraint("=", Identifier("x"), NatLiteral(2)),
                left=ArithmeticConstraint("=", Identifier("x"), NatLiteral(1)),
            ),
            env, reg,
        )
        nat_c = translate_constraint(
            MembershipConstraint("∈", Identifier("x"), Identifier("Nat")), env, reg
        )
        assert sat_with([disj, nat_c])

    def test_solver_finds_correct_model(self):
        """After adding constraints, the model must reflect the unique solution."""
        env, reg = make_env_registry()
        x = z3.Int("x")
        env = env.bind("x", x)

        # x = 5 uniquely determines x
        c = translate_constraint(
            ArithmeticConstraint("=", Identifier("x"), NatLiteral(5)), env, reg
        )
        s = z3.Solver()
        s.add(c)
        assert s.check() == z3.sat
        model = s.model()
        assert model[x].as_long() == 5
