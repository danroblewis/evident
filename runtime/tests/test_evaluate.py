"""
Phase 7 tests: full schema evaluation — the complete solve loop.

Each test exercises evaluate_schema / EvidentSolver.evaluate end-to-end:
  1. Build a SchemaDecl programmatically (no parser required).
  2. Call evaluate_schema (or EvidentSolver.evaluate).
  3. Assert the result is sat / unsat and that the model contains the
     expected values.
"""

import pytest
import z3

from runtime.src.evaluate import evaluate_schema, EvidentSolver, EvaluationResult
from runtime.src.sorts import SortRegistry
from runtime.src.ast_types import (
    SchemaDecl,
    Param,
    Identifier,
    ArithmeticConstraint,
    MembershipConstraint,
    NatLiteral,
    IntLiteral,
    BinaryExpr,
    UniversalConstraint,
    ExistentialConstraint,
    Binding,
    SetLiteral,
    RangeLiteral,
    LogicConstraint,
    BindingConstraint,
)


# ---------------------------------------------------------------------------
# Helpers — schema builders
# ---------------------------------------------------------------------------


def nat_param(*names: str) -> Param:
    """Return a Param declaring ``names ∈ Nat``."""
    return Param(names=list(names), set=Identifier(name="Nat"))


def int_param(*names: str) -> Param:
    """Return a Param declaring ``names ∈ Int``."""
    return Param(names=list(names), set=Identifier(name="Int"))


def mk_schema(name: str, params: list[Param], body: list) -> SchemaDecl:
    return SchemaDecl(keyword="schema", name=name, params=params, body=body)


# ---------------------------------------------------------------------------
# Test: simple satisfiable schema  (n ∈ Nat, n > 5)
# ---------------------------------------------------------------------------


class TestSimpleNat:
    """schema SimpleNat — n ∈ Nat — n > 5"""

    def _schema(self):
        return mk_schema(
            "SimpleNat",
            params=[nat_param("n")],
            body=[
                ArithmeticConstraint(">", Identifier("n"), NatLiteral(5)),
            ],
        )

    def test_is_satisfiable(self):
        result = evaluate_schema(self._schema())
        assert result.satisfied is True

    def test_model_has_n_greater_than_five(self):
        result = evaluate_schema(self._schema())
        assert result.satisfied is True
        assert "n" in result.bindings
        assert result.bindings["n"] > 5

    def test_model_ref_is_not_none(self):
        result = evaluate_schema(self._schema())
        assert result.model is not None

    def test_explanation_is_none_when_sat(self):
        result = evaluate_schema(self._schema())
        assert result.explanation is None

    def test_n_is_nonneg(self):
        """Nat constraint should also enforce n ≥ 0."""
        result = evaluate_schema(self._schema())
        assert result.bindings["n"] >= 0


# ---------------------------------------------------------------------------
# Test: unsatisfiable schema  (n ∈ Nat, n > 5, n < 3)
# ---------------------------------------------------------------------------


class TestImpossible:
    """schema Impossible — n ∈ Nat — n > 5 — n < 3"""

    def _schema(self):
        return mk_schema(
            "Impossible",
            params=[nat_param("n")],
            body=[
                ArithmeticConstraint(">", Identifier("n"), NatLiteral(5)),
                ArithmeticConstraint("<", Identifier("n"), NatLiteral(3)),
            ],
        )

    def test_is_unsatisfiable(self):
        result = evaluate_schema(self._schema())
        assert result.satisfied is False

    def test_bindings_empty_when_unsat(self):
        result = evaluate_schema(self._schema())
        assert result.bindings == {}

    def test_model_is_none_when_unsat(self):
        result = evaluate_schema(self._schema())
        assert result.model is None

    def test_explanation_is_present_when_unsat(self):
        result = evaluate_schema(self._schema())
        assert result.explanation is not None
        assert len(result.explanation) > 0


# ---------------------------------------------------------------------------
# Test: pre-bound variable  (id=1, duration=60 → deadline > 60)
# ---------------------------------------------------------------------------


class TestTask:
    """schema Task — id ∈ Nat, duration ∈ Nat, deadline ∈ Nat — duration < deadline"""

    def _schema(self):
        return mk_schema(
            "Task",
            params=[nat_param("id"), nat_param("duration"), nat_param("deadline")],
            body=[
                ArithmeticConstraint("<", Identifier("duration"), Identifier("deadline")),
            ],
        )

    def test_pre_bound_id_and_duration(self):
        result = evaluate_schema(self._schema(), given={"id": 1, "duration": 60})
        assert result.satisfied is True
        assert result.bindings["id"] == 1
        assert result.bindings["duration"] == 60
        assert result.bindings["deadline"] > 60

    def test_deadline_gt_duration_in_model(self):
        result = evaluate_schema(self._schema(), given={"duration": 100})
        assert result.satisfied is True
        assert result.bindings["deadline"] > 100

    def test_contradiction_with_given(self):
        """If we pre-bind deadline=10 and duration=60, duration < deadline is unsat."""
        result = evaluate_schema(
            self._schema(),
            given={"duration": 60, "deadline": 10},
        )
        assert result.satisfied is False


# ---------------------------------------------------------------------------
# Test: equality constraint finds value  (x + y = 10, x=3 → y=7)
# ---------------------------------------------------------------------------


class TestFixed:
    """schema Fixed — x ∈ Nat, y ∈ Nat — x + y = 10"""

    def _schema(self):
        return mk_schema(
            "Fixed",
            params=[nat_param("x"), nat_param("y")],
            body=[
                ArithmeticConstraint(
                    "=",
                    BinaryExpr("+", Identifier("x"), Identifier("y")),
                    NatLiteral(10),
                ),
            ],
        )

    def test_x3_implies_y7(self):
        result = evaluate_schema(self._schema(), given={"x": 3})
        assert result.satisfied is True
        assert result.bindings["x"] == 3
        assert result.bindings["y"] == 7

    def test_x0_implies_y10(self):
        result = evaluate_schema(self._schema(), given={"x": 0})
        assert result.satisfied is True
        assert result.bindings["y"] == 10

    def test_x10_implies_y0(self):
        result = evaluate_schema(self._schema(), given={"x": 10})
        assert result.satisfied is True
        assert result.bindings["y"] == 0

    def test_sum_is_ten_without_given(self):
        result = evaluate_schema(self._schema())
        assert result.satisfied is True
        assert result.bindings["x"] + result.bindings["y"] == 10

    def test_x11_unsat(self):
        """x=11 would require y=-1, but y ∈ Nat so y ≥ 0 → unsat."""
        result = evaluate_schema(self._schema(), given={"x": 11})
        assert result.satisfied is False


# ---------------------------------------------------------------------------
# Test: multiple constraints  (n ∈ Nat, n > 5, n < 10)
# ---------------------------------------------------------------------------


class TestBounded:
    """schema Bounded — n ∈ Nat — n > 5 — n < 10"""

    def _schema(self):
        return mk_schema(
            "Bounded",
            params=[nat_param("n")],
            body=[
                ArithmeticConstraint(">", Identifier("n"), NatLiteral(5)),
                ArithmeticConstraint("<", Identifier("n"), NatLiteral(10)),
            ],
        )

    def test_is_satisfiable(self):
        result = evaluate_schema(self._schema())
        assert result.satisfied is True

    def test_n_in_range(self):
        result = evaluate_schema(self._schema())
        assert result.satisfied is True
        n = result.bindings["n"]
        assert 5 < n < 10

    def test_n_is_nat(self):
        result = evaluate_schema(self._schema())
        assert result.bindings["n"] >= 0

    def test_pre_bound_n_in_range_sat(self):
        result = evaluate_schema(self._schema(), given={"n": 7})
        assert result.satisfied is True
        assert result.bindings["n"] == 7

    def test_pre_bound_n_out_of_range_unsat(self):
        result = evaluate_schema(self._schema(), given={"n": 10})
        assert result.satisfied is False


# ---------------------------------------------------------------------------
# Test: universal quantifier  (∀ x ∈ {1,2,3} : x > 0)
# ---------------------------------------------------------------------------


class TestUniversalQuantifier:
    """
    schema AllPositive
        n ∈ Nat
        ∀ x ∈ {1, 2, 3} : x > 0
    """

    def _schema_always_true(self):
        """∀ x ∈ {1,2,3} : x > 0 — always true, sat."""
        return mk_schema(
            "AllPositive",
            params=[nat_param("n")],
            body=[
                UniversalConstraint(
                    bindings=[
                        Binding(
                            names=["x"],
                            set=SetLiteral(elements=[
                                NatLiteral(1), NatLiteral(2), NatLiteral(3),
                            ]),
                        )
                    ],
                    body=ArithmeticConstraint(">", Identifier("x"), NatLiteral(0)),
                ),
            ],
        )

    def _schema_always_false(self):
        """∀ x ∈ {1,2,3} : x > 2 — false (1 and 2 fail), unsat."""
        return mk_schema(
            "SomeNotGt2",
            params=[nat_param("n")],
            body=[
                UniversalConstraint(
                    bindings=[
                        Binding(
                            names=["x"],
                            set=SetLiteral(elements=[
                                NatLiteral(1), NatLiteral(2), NatLiteral(3),
                            ]),
                        )
                    ],
                    body=ArithmeticConstraint(">", Identifier("x"), NatLiteral(2)),
                ),
            ],
        )

    def test_all_positive_is_sat(self):
        result = evaluate_schema(self._schema_always_true())
        assert result.satisfied is True

    def test_not_all_gt_two_is_unsat(self):
        result = evaluate_schema(self._schema_always_false())
        assert result.satisfied is False

    def test_universal_over_range(self):
        """∀ x ∈ 1..3 : x > 0 — sat."""
        schema = mk_schema(
            "RangeUniversal",
            params=[nat_param("n")],
            body=[
                UniversalConstraint(
                    bindings=[
                        Binding(
                            names=["x"],
                            set=RangeLiteral(
                                from_=NatLiteral(1),
                                to=NatLiteral(3),
                            ),
                        )
                    ],
                    body=ArithmeticConstraint(">", Identifier("x"), NatLiteral(0)),
                ),
            ],
        )
        result = evaluate_schema(schema)
        assert result.satisfied is True


# ---------------------------------------------------------------------------
# Test: existential quantifier  (∃ x ∈ {1,2,3} : x > 2)
# ---------------------------------------------------------------------------


class TestExistentialQuantifier:
    """∃ x ∈ {1,2,3} : x > 2  →  sat (x=3)."""

    def _schema_exists(self):
        return mk_schema(
            "ExistsGt2",
            params=[nat_param("n")],
            body=[
                ExistentialConstraint(
                    quantifier="∃",
                    bindings=[
                        Binding(
                            names=["x"],
                            set=SetLiteral(elements=[
                                NatLiteral(1), NatLiteral(2), NatLiteral(3),
                            ]),
                        )
                    ],
                    body=ArithmeticConstraint(">", Identifier("x"), NatLiteral(2)),
                ),
            ],
        )

    def _schema_no_exists(self):
        """∃ x ∈ {1,2,3} : x > 5  →  unsat."""
        return mk_schema(
            "ExistsGt5",
            params=[nat_param("n")],
            body=[
                ExistentialConstraint(
                    quantifier="∃",
                    bindings=[
                        Binding(
                            names=["x"],
                            set=SetLiteral(elements=[
                                NatLiteral(1), NatLiteral(2), NatLiteral(3),
                            ]),
                        )
                    ],
                    body=ArithmeticConstraint(">", Identifier("x"), NatLiteral(5)),
                ),
            ],
        )

    def test_exists_gt_two_sat(self):
        result = evaluate_schema(self._schema_exists())
        assert result.satisfied is True

    def test_exists_gt_five_unsat(self):
        result = evaluate_schema(self._schema_no_exists())
        assert result.satisfied is False


# ---------------------------------------------------------------------------
# Test: convenience function API
# ---------------------------------------------------------------------------


class TestConvenienceFunction:
    def test_returns_evaluation_result(self):
        schema = mk_schema(
            "Trivial",
            params=[nat_param("x")],
            body=[],
        )
        result = evaluate_schema(schema)
        assert isinstance(result, EvaluationResult)

    def test_with_custom_registry(self):
        schema = mk_schema(
            "TrivialReg",
            params=[nat_param("x")],
            body=[ArithmeticConstraint(">", Identifier("x"), NatLiteral(0))],
        )
        reg = SortRegistry()
        result = evaluate_schema(schema, registry=reg)
        assert result.satisfied is True

    def test_given_none_is_same_as_empty_dict(self):
        schema = mk_schema(
            "TrivialNone",
            params=[nat_param("x")],
            body=[],
        )
        r1 = evaluate_schema(schema, given=None)
        r2 = evaluate_schema(schema, given={})
        assert r1.satisfied == r2.satisfied


# ---------------------------------------------------------------------------
# Test: EvidentSolver.register_schema and assert_fact
# ---------------------------------------------------------------------------


class TestEvidentSolverAPI:
    def test_register_schema_stores_schema(self):
        solver = EvidentSolver()
        schema = mk_schema("S", params=[nat_param("x")], body=[])
        solver.register_schema(schema)
        assert "S" in solver.schemas
        assert solver.schemas["S"] is schema

    def test_assert_fact_binds_value(self):
        solver = EvidentSolver()
        solver.assert_fact("deadline", 100)
        assert "deadline" in solver.env.bindings

    def test_evaluate_simple(self):
        solver = EvidentSolver()
        schema = mk_schema(
            "Simple",
            params=[nat_param("x")],
            body=[ArithmeticConstraint(">", Identifier("x"), NatLiteral(3))],
        )
        result = solver.evaluate(schema)
        assert result.satisfied is True
        assert result.bindings["x"] > 3

    def test_evaluate_with_given(self):
        solver = EvidentSolver()
        schema = mk_schema(
            "SimpleGiven",
            params=[nat_param("x"), nat_param("y")],
            body=[
                ArithmeticConstraint("=", Identifier("x"), Identifier("y")),
            ],
        )
        result = solver.evaluate(schema, given={"x": 42})
        assert result.satisfied is True
        assert result.bindings["x"] == 42
        assert result.bindings["y"] == 42


# ---------------------------------------------------------------------------
# Test: multiple independent solves on the same schema
# ---------------------------------------------------------------------------


class TestMultipleSolves:
    """Ensure each call to evaluate_schema is independent."""

    def _schema(self):
        return mk_schema(
            "Vary",
            params=[nat_param("n")],
            body=[
                ArithmeticConstraint("=", Identifier("n"), NatLiteral(7)),
            ],
        )

    def test_first_solve(self):
        result = evaluate_schema(self._schema())
        assert result.satisfied is True
        assert result.bindings["n"] == 7

    def test_second_solve_independent(self):
        # A second call with a different 'given' should not be polluted
        # by the first call.
        result = evaluate_schema(self._schema(), given={"n": 7})
        assert result.satisfied is True
        assert result.bindings["n"] == 7

    def test_conflicting_given_unsat(self):
        result = evaluate_schema(self._schema(), given={"n": 8})
        assert result.satisfied is False


# ---------------------------------------------------------------------------
# Test: two-variable interaction
# ---------------------------------------------------------------------------


class TestTwoVariables:
    """x + y = 10, x = 3 → y must be 7; also x ∈ Nat, y ∈ Nat."""

    def _schema(self):
        return mk_schema(
            "Sum10",
            params=[nat_param("x"), nat_param("y")],
            body=[
                ArithmeticConstraint(
                    "=",
                    BinaryExpr("+", Identifier("x"), Identifier("y")),
                    NatLiteral(10),
                )
            ],
        )

    def test_determines_y(self):
        result = evaluate_schema(self._schema(), given={"x": 3})
        assert result.satisfied is True
        assert result.bindings["y"] == 7

    def test_determines_x(self):
        result = evaluate_schema(self._schema(), given={"y": 4})
        assert result.satisfied is True
        assert result.bindings["x"] == 6

    def test_both_free_sum_is_10(self):
        result = evaluate_schema(self._schema())
        assert result.satisfied is True
        x = result.bindings["x"]
        y = result.bindings["y"]
        assert x + y == 10
        assert x >= 0
        assert y >= 0


# ---------------------------------------------------------------------------
# Test: _z3_to_python value conversion
# ---------------------------------------------------------------------------


class TestZ3ToPython:
    def test_int_value(self):
        solver = EvidentSolver()
        assert solver._z3_to_python(z3.IntVal(42)) == 42

    def test_negative_int(self):
        solver = EvidentSolver()
        assert solver._z3_to_python(z3.IntVal(-5)) == -5

    def test_bool_true(self):
        solver = EvidentSolver()
        assert solver._z3_to_python(z3.BoolVal(True)) is True

    def test_bool_false(self):
        solver = EvidentSolver()
        assert solver._z3_to_python(z3.BoolVal(False)) is False

    def test_string_value(self):
        solver = EvidentSolver()
        assert solver._z3_to_python(z3.StringVal("hello")) == "hello"


# ---------------------------------------------------------------------------
# Test: _python_to_z3_untyped value conversion
# ---------------------------------------------------------------------------


class TestPythonToZ3:
    def test_int(self):
        solver = EvidentSolver()
        val = solver._python_to_z3_untyped(5)
        assert z3.is_int_value(val)
        assert val.as_long() == 5

    def test_bool_true(self):
        solver = EvidentSolver()
        val = solver._python_to_z3_untyped(True)
        assert z3.is_true(val)

    def test_bool_false(self):
        solver = EvidentSolver()
        val = solver._python_to_z3_untyped(False)
        assert z3.is_false(val)

    def test_str(self):
        solver = EvidentSolver()
        val = solver._python_to_z3_untyped("world")
        assert val.sort() == z3.StringSort()

    def test_float(self):
        solver = EvidentSolver()
        val = solver._python_to_z3_untyped(3.14)
        assert z3.is_rational_value(val)

    def test_unsupported_type_raises(self):
        solver = EvidentSolver()
        with pytest.raises(ValueError):
            solver._python_to_z3_untyped([1, 2, 3])
