"""
Phase 6 tests: quantifier translation and cardinality constraints.

Every test drives Z3 through the public API and checks satisfiability
(sat / unsat) rather than inspecting Z3 expressions structurally.
"""

import pytest
from z3 import (
    And,
    BoolVal,
    Distinct,
    IntSort,
    IntVal,
    Not,
    PbEq,
    PbGe,
    PbLe,
    Solver,
    sat,
    unsat,
)

from runtime.src.quantifiers import (
    all_different,
    at_least_n,
    at_most_n,
    exactly_n,
    translate_cardinality_constraint,
    translate_existential,
    translate_universal,
)
from runtime.src.env import Environment
from runtime.src.sorts import SortRegistry
from runtime.src.ast_types import (
    ArithmeticConstraint,
    Binding,
    BinaryExpr,
    CardinalityConstraint,
    EmptySet,
    ExistentialConstraint,
    Identifier,
    NatLiteral,
    SetLiteral,
    UniversalConstraint,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _env_reg():
    return Environment(), SortRegistry()


def _check(assertion) -> str:
    """Return 'sat' or 'unsat' for a single Z3 boolean assertion."""
    s = Solver()
    s.add(assertion)
    result = s.check()
    return "sat" if result == sat else "unsat"


def _model_value(assertion, var):
    """Return the Z3 model integer value for `var` given `assertion`."""
    s = Solver()
    s.add(assertion)
    assert s.check() == sat, "Expected sat to extract model"
    return s.model()[var].as_long()


# ---------------------------------------------------------------------------
# Universal quantifier  ∀ x ∈ S : P(x)
# ---------------------------------------------------------------------------

class TestUniversal:
    def test_all_positive_sat(self):
        """∀ x ∈ {1,2,3} : x > 0 — all elements satisfy x > 0."""
        env, reg = _env_reg()
        node = UniversalConstraint(
            bindings=[Binding(names=["x"], set=SetLiteral([NatLiteral(1), NatLiteral(2), NatLiteral(3)]))],
            body=ArithmeticConstraint(">", Identifier("x"), NatLiteral(0)),
        )
        result = translate_universal(node, env, reg)
        assert _check(result) == "sat"

    def test_not_all_satisfy_unsat(self):
        """∀ x ∈ {1,2,3} : x > 2 — 1 and 2 violate, so the assertion is false."""
        env, reg = _env_reg()
        node = UniversalConstraint(
            bindings=[Binding(names=["x"], set=SetLiteral([NatLiteral(1), NatLiteral(2), NatLiteral(3)]))],
            body=ArithmeticConstraint(">", Identifier("x"), NatLiteral(2)),
        )
        result = translate_universal(node, env, reg)
        assert _check(result) == "unsat"

    def test_empty_domain_vacuously_true(self):
        """∀ x ∈ {} : x > 100 — vacuously true."""
        env, reg = _env_reg()
        node = UniversalConstraint(
            bindings=[Binding(names=["x"], set=EmptySet())],
            body=ArithmeticConstraint(">", Identifier("x"), NatLiteral(100)),
        )
        result = translate_universal(node, env, reg)
        assert _check(result) == "sat"

    def test_two_bindings_in_one_clause_sat(self):
        """∀ x, y ∈ {1,2} : x + y ≥ 2

        Combinations: (1,1)→2≥2✓, (1,2)→3≥2✓, (2,1)→3≥2✓, (2,2)→4≥2✓ → sat.
        """
        env, reg = _env_reg()
        node = UniversalConstraint(
            bindings=[Binding(
                names=["x", "y"],
                set=SetLiteral([NatLiteral(1), NatLiteral(2)]),
            )],
            body=ArithmeticConstraint(
                "≥",
                BinaryExpr("+", Identifier("x"), Identifier("y")),
                NatLiteral(2),
            ),
        )
        result = translate_universal(node, env, reg)
        assert _check(result) == "sat"

    def test_two_bindings_unsat(self):
        """∀ x, y ∈ {1,2} : x + y > 3

        Only (2,2)→4>3✓; (1,1)→2>3✗ violates → unsat.
        """
        env, reg = _env_reg()
        node = UniversalConstraint(
            bindings=[Binding(
                names=["x", "y"],
                set=SetLiteral([NatLiteral(1), NatLiteral(2)]),
            )],
            body=ArithmeticConstraint(
                ">",
                BinaryExpr("+", Identifier("x"), Identifier("y")),
                NatLiteral(3),
            ),
        )
        result = translate_universal(node, env, reg)
        assert _check(result) == "unsat"

    def test_single_element_domain(self):
        """∀ x ∈ {5} : x = 5 — trivially sat."""
        env, reg = _env_reg()
        node = UniversalConstraint(
            bindings=[Binding(names=["x"], set=SetLiteral([NatLiteral(5)]))],
            body=ArithmeticConstraint("=", Identifier("x"), NatLiteral(5)),
        )
        result = translate_universal(node, env, reg)
        assert _check(result) == "sat"

    def test_single_element_domain_unsat(self):
        """∀ x ∈ {5} : x = 6 — 5 ≠ 6 → unsat."""
        env, reg = _env_reg()
        node = UniversalConstraint(
            bindings=[Binding(names=["x"], set=SetLiteral([NatLiteral(5)]))],
            body=ArithmeticConstraint("=", Identifier("x"), NatLiteral(6)),
        )
        result = translate_universal(node, env, reg)
        assert _check(result) == "unsat"


# ---------------------------------------------------------------------------
# Existential quantifier  ∃ x ∈ S : P(x)
# ---------------------------------------------------------------------------

class TestExistential:
    def test_exists_some_element_sat(self):
        """∃ x ∈ {1,2,3} : x > 2 — x=3 is a witness → sat."""
        env, reg = _env_reg()
        node = ExistentialConstraint(
            quantifier="∃",
            bindings=[Binding(names=["x"], set=SetLiteral([NatLiteral(1), NatLiteral(2), NatLiteral(3)]))],
            body=ArithmeticConstraint(">", Identifier("x"), NatLiteral(2)),
        )
        result = translate_existential(node, env, reg)
        assert _check(result) == "sat"

    def test_exists_no_element_unsat(self):
        """∃ x ∈ {1,2,3} : x > 10 — no witness → unsat."""
        env, reg = _env_reg()
        node = ExistentialConstraint(
            quantifier="∃",
            bindings=[Binding(names=["x"], set=SetLiteral([NatLiteral(1), NatLiteral(2), NatLiteral(3)]))],
            body=ArithmeticConstraint(">", Identifier("x"), NatLiteral(10)),
        )
        result = translate_existential(node, env, reg)
        assert _check(result) == "unsat"

    def test_exists_empty_domain_unsat(self):
        """∃ x ∈ {} : x > 0 — no elements → unsat."""
        env, reg = _env_reg()
        node = ExistentialConstraint(
            quantifier="∃",
            bindings=[Binding(names=["x"], set=EmptySet())],
            body=ArithmeticConstraint(">", Identifier("x"), NatLiteral(0)),
        )
        result = translate_existential(node, env, reg)
        assert _check(result) == "unsat"

    def test_unique_exists_sat(self):
        """∃! x ∈ {1,2,3} : x = 2 — exactly one element equals 2 → sat."""
        env, reg = _env_reg()
        node = ExistentialConstraint(
            quantifier="∃!",
            bindings=[Binding(names=["x"], set=SetLiteral([NatLiteral(1), NatLiteral(2), NatLiteral(3)]))],
            body=ArithmeticConstraint("=", Identifier("x"), NatLiteral(2)),
        )
        result = translate_existential(node, env, reg)
        assert _check(result) == "sat"

    def test_unique_exists_two_witnesses_unsat(self):
        """∃! x ∈ {1,2,3} : x > 1 — x=2 and x=3 both satisfy → unsat (not unique)."""
        env, reg = _env_reg()
        node = ExistentialConstraint(
            quantifier="∃!",
            bindings=[Binding(names=["x"], set=SetLiteral([NatLiteral(1), NatLiteral(2), NatLiteral(3)]))],
            body=ArithmeticConstraint(">", Identifier("x"), NatLiteral(1)),
        )
        result = translate_existential(node, env, reg)
        assert _check(result) == "unsat"

    def test_unique_exists_no_witness_unsat(self):
        """∃! x ∈ {1,2,3} : x > 10 — no witness → unsat."""
        env, reg = _env_reg()
        node = ExistentialConstraint(
            quantifier="∃!",
            bindings=[Binding(names=["x"], set=SetLiteral([NatLiteral(1), NatLiteral(2), NatLiteral(3)]))],
            body=ArithmeticConstraint(">", Identifier("x"), NatLiteral(10)),
        )
        result = translate_existential(node, env, reg)
        assert _check(result) == "unsat"

    def test_neg_exists_sat(self):
        """¬∃ x ∈ {1,2,3} : x > 10 — no element exceeds 10 → not-exists is sat."""
        env, reg = _env_reg()
        node = ExistentialConstraint(
            quantifier="¬∃",
            bindings=[Binding(names=["x"], set=SetLiteral([NatLiteral(1), NatLiteral(2), NatLiteral(3)]))],
            body=ArithmeticConstraint(">", Identifier("x"), NatLiteral(10)),
        )
        result = translate_existential(node, env, reg)
        assert _check(result) == "sat"

    def test_neg_exists_unsat(self):
        """¬∃ x ∈ {1,2,3} : x > 2 — x=3 exists → not-exists is unsat."""
        env, reg = _env_reg()
        node = ExistentialConstraint(
            quantifier="¬∃",
            bindings=[Binding(names=["x"], set=SetLiteral([NatLiteral(1), NatLiteral(2), NatLiteral(3)]))],
            body=ArithmeticConstraint(">", Identifier("x"), NatLiteral(2)),
        )
        result = translate_existential(node, env, reg)
        assert _check(result) == "unsat"

    def test_neg_exists_empty_domain_sat(self):
        """¬∃ x ∈ {} : x > 0 — vacuously no witness → sat."""
        env, reg = _env_reg()
        node = ExistentialConstraint(
            quantifier="¬∃",
            bindings=[Binding(names=["x"], set=EmptySet())],
            body=ArithmeticConstraint(">", Identifier("x"), NatLiteral(0)),
        )
        result = translate_existential(node, env, reg)
        assert _check(result) == "sat"


# ---------------------------------------------------------------------------
# Cardinality helpers: at_most_n, at_least_n, exactly_n, all_different
# ---------------------------------------------------------------------------

class TestCardinalityHelpers:
    """
    Tests using concrete Z3 BoolVal / IntVal lists directly.
    We pass lists of boolean indicator expressions (each is a BoolRef).
    """

    # -- at_most_n -----------------------------------------------------------

    def test_at_most_2_of_3_elements_unsat(self):
        """at_most 2 {True, True, True} — 3 trues exceeds limit 2 → unsat."""
        indicators = [BoolVal(True), BoolVal(True), BoolVal(True)]
        assertion = at_most_n(2, indicators)
        assert _check(assertion) == "unsat"

    def test_at_most_3_of_3_elements_sat(self):
        """at_most 3 {True, True, True} — 3 ≤ 3 → sat."""
        indicators = [BoolVal(True), BoolVal(True), BoolVal(True)]
        assertion = at_most_n(3, indicators)
        assert _check(assertion) == "sat"

    def test_at_most_0_of_empty_sat(self):
        """at_most 0 {} — trivially sat."""
        assertion = at_most_n(0, [])
        assert _check(assertion) == "sat"

    def test_at_most_works_with_mixed(self):
        """at_most 2 {True, False, True} — 2 ≤ 2 → sat."""
        indicators = [BoolVal(True), BoolVal(False), BoolVal(True)]
        assertion = at_most_n(2, indicators)
        assert _check(assertion) == "sat"

    # -- at_least_n ----------------------------------------------------------

    def test_at_least_2_of_3_sat(self):
        """at_least 2 {True, True, True} — 3 ≥ 2 → sat."""
        indicators = [BoolVal(True), BoolVal(True), BoolVal(True)]
        assertion = at_least_n(2, indicators)
        assert _check(assertion) == "sat"

    def test_at_least_2_of_1_true_unsat(self):
        """at_least 2 {True, False, False} — 1 < 2 → unsat."""
        indicators = [BoolVal(True), BoolVal(False), BoolVal(False)]
        assertion = at_least_n(2, indicators)
        assert _check(assertion) == "unsat"

    def test_at_least_0_of_empty_sat(self):
        """at_least 0 {} — trivially sat."""
        assertion = at_least_n(0, [])
        assert _check(assertion) == "sat"

    def test_at_least_1_of_empty_unsat(self):
        """at_least 1 {} — no elements → unsat."""
        assertion = at_least_n(1, [])
        assert _check(assertion) == "unsat"

    # -- exactly_n -----------------------------------------------------------

    def test_exactly_3_of_3_sat(self):
        """exactly 3 {True, True, True} — 3 == 3 → sat."""
        indicators = [BoolVal(True), BoolVal(True), BoolVal(True)]
        assertion = exactly_n(3, indicators)
        assert _check(assertion) == "sat"

    def test_exactly_2_of_3_unsat(self):
        """exactly 2 {True, True, True} — 3 ≠ 2 → unsat."""
        indicators = [BoolVal(True), BoolVal(True), BoolVal(True)]
        assertion = exactly_n(2, indicators)
        assert _check(assertion) == "unsat"

    def test_exactly_0_of_empty_sat(self):
        """exactly 0 {} — 0 == 0 → sat."""
        assertion = exactly_n(0, [])
        assert _check(assertion) == "sat"

    def test_exactly_1_of_empty_unsat(self):
        """exactly 1 {} — 0 ≠ 1 → unsat."""
        assertion = exactly_n(1, [])
        assert _check(assertion) == "unsat"

    # -- all_different -------------------------------------------------------

    def test_all_different_distinct_elements_sat(self):
        """all_different [1, 2, 3] — all distinct → sat."""
        elements = [IntVal(1), IntVal(2), IntVal(3)]
        assertion = all_different(elements)
        assert _check(assertion) == "sat"

    def test_all_different_repeated_element_unsat(self):
        """all_different [1, 2, 1] — 1 appears twice → unsat."""
        elements = [IntVal(1), IntVal(2), IntVal(1)]
        assertion = all_different(elements)
        assert _check(assertion) == "unsat"

    def test_all_different_single_element_sat(self):
        """all_different [5] — trivially sat (one element)."""
        assertion = all_different([IntVal(5)])
        assert _check(assertion) == "sat"

    def test_all_different_empty_sat(self):
        """all_different [] — trivially sat."""
        assertion = all_different([])
        assert _check(assertion) == "sat"


# ---------------------------------------------------------------------------
# CardinalityConstraint AST node translation
# ---------------------------------------------------------------------------

class TestTranslateCardinalityConstraint:
    """Tests using the full translate_cardinality_constraint path."""

    def test_at_most_2_set_of_3_unsat(self):
        """at_most 2 {1,2,3} — cardinality 3 > 2 → unsat."""
        env, reg = _env_reg()
        node = CardinalityConstraint(
            op="at_most",
            count=NatLiteral(2),
            set=SetLiteral([NatLiteral(1), NatLiteral(2), NatLiteral(3)]),
        )
        result = translate_cardinality_constraint(node, env, reg)
        assert _check(result) == "unsat"

    def test_at_most_3_set_of_3_sat(self):
        """at_most 3 {1,2,3} — cardinality 3 ≤ 3 → sat."""
        env, reg = _env_reg()
        node = CardinalityConstraint(
            op="at_most",
            count=NatLiteral(3),
            set=SetLiteral([NatLiteral(1), NatLiteral(2), NatLiteral(3)]),
        )
        result = translate_cardinality_constraint(node, env, reg)
        assert _check(result) == "sat"

    def test_at_least_2_set_of_3_sat(self):
        """at_least 2 {1,2,3} — cardinality 3 ≥ 2 → sat."""
        env, reg = _env_reg()
        node = CardinalityConstraint(
            op="at_least",
            count=NatLiteral(2),
            set=SetLiteral([NatLiteral(1), NatLiteral(2), NatLiteral(3)]),
        )
        result = translate_cardinality_constraint(node, env, reg)
        assert _check(result) == "sat"

    def test_at_least_4_set_of_3_unsat(self):
        """at_least 4 {1,2,3} — cardinality 3 < 4 → unsat."""
        env, reg = _env_reg()
        node = CardinalityConstraint(
            op="at_least",
            count=NatLiteral(4),
            set=SetLiteral([NatLiteral(1), NatLiteral(2), NatLiteral(3)]),
        )
        result = translate_cardinality_constraint(node, env, reg)
        assert _check(result) == "unsat"

    def test_exactly_3_set_of_3_sat(self):
        """exactly 3 {1,2,3} — cardinality 3 == 3 → sat."""
        env, reg = _env_reg()
        node = CardinalityConstraint(
            op="exactly",
            count=NatLiteral(3),
            set=SetLiteral([NatLiteral(1), NatLiteral(2), NatLiteral(3)]),
        )
        result = translate_cardinality_constraint(node, env, reg)
        assert _check(result) == "sat"

    def test_exactly_2_set_of_3_unsat(self):
        """exactly 2 {1,2,3} — cardinality 3 ≠ 2 → unsat."""
        env, reg = _env_reg()
        node = CardinalityConstraint(
            op="exactly",
            count=NatLiteral(2),
            set=SetLiteral([NatLiteral(1), NatLiteral(2), NatLiteral(3)]),
        )
        result = translate_cardinality_constraint(node, env, reg)
        assert _check(result) == "unsat"

    def test_at_most_0_empty_set_sat(self):
        """at_most 0 {} — cardinality 0 ≤ 0 → sat."""
        env, reg = _env_reg()
        node = CardinalityConstraint(
            op="at_most",
            count=NatLiteral(0),
            set=EmptySet(),
        )
        result = translate_cardinality_constraint(node, env, reg)
        assert _check(result) == "sat"
