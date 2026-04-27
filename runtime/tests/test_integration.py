"""
Phase 12 integration tests: full-stack evaluation through EvidentRuntime.

Tests cover:
- Basic schema query (sat / unsat)
- Pre-bound query with computed variable
- Contradictory schema returns satisfied=False
- Evidence is populated when sat
- evidence.claim and evidence.bindings
- Program loading (multiple schemas + asserts)
- Query by name after loading
- Session monotonicity (ground facts survive across queries)
- Session.is_established
- QueryResult fields
- query_schema (inline schema, not pre-registered)
- load_program with ForwardRule (no crash)
"""

import pytest

from runtime.src.runtime import EvidentRuntime, QueryResult
from runtime.src.session import Session
from runtime.src.evidence import Evidence
from runtime.src.ast_types import (
    Program,
    SchemaDecl,
    Param,
    AssertStmt,
    ForwardRule,
    ApplicationConstraint,
    Identifier,
    ArithmeticConstraint,
    MembershipConstraint,
    BinaryExpr,
    NatLiteral,
    IntLiteral,
    StringLiteral,
    BoolLiteral,
    LogicConstraint,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def nat_param(*names: str) -> Param:
    """Shorthand: Param declaring ``names ∈ Nat``."""
    return Param(names=list(names), set=Identifier(name="Nat"))


def int_param(*names: str) -> Param:
    return Param(names=list(names), set=Identifier(name="Int"))


def mk_schema(name: str, params: list, body: list) -> SchemaDecl:
    return SchemaDecl(keyword="schema", name=name, params=params, body=body)


# ---------------------------------------------------------------------------
# 1. Basic schema query — sat
# ---------------------------------------------------------------------------


class TestBasicSchemaQuery:
    """
    schema SimpleNat
        n ∈ Nat
        n > 5
    """

    def _schema(self) -> SchemaDecl:
        return mk_schema(
            "SimpleNat",
            params=[nat_param("n")],
            body=[
                MembershipConstraint("∈", Identifier("n"), Identifier("Nat")),
                ArithmeticConstraint(">", Identifier("n"), NatLiteral(5)),
            ],
        )

    def test_returns_query_result(self):
        rt = EvidentRuntime()
        rt.load_schema(self._schema())
        result = rt.query("SimpleNat")
        assert isinstance(result, QueryResult)

    def test_satisfied(self):
        rt = EvidentRuntime()
        rt.load_schema(self._schema())
        result = rt.query("SimpleNat")
        assert result.satisfied is True

    def test_bindings_has_n_gt_5(self):
        rt = EvidentRuntime()
        rt.load_schema(self._schema())
        result = rt.query("SimpleNat")
        assert "n" in result.bindings
        assert result.bindings["n"] > 5

    def test_n_is_nat(self):
        rt = EvidentRuntime()
        rt.load_schema(self._schema())
        result = rt.query("SimpleNat")
        assert result.bindings["n"] >= 0


# ---------------------------------------------------------------------------
# 2. Pre-bound query — x ∈ Nat, y ∈ Nat, x + y = 10; query with x=3 → y=7
# ---------------------------------------------------------------------------


class TestPreBoundQuery:
    """
    schema Sum10
        x ∈ Nat
        y ∈ Nat
        x + y = 10
    Query with x=3: expect y=7.
    """

    def _schema(self) -> SchemaDecl:
        return mk_schema(
            "Sum10",
            params=[nat_param("x"), nat_param("y")],
            body=[
                ArithmeticConstraint(
                    "=",
                    BinaryExpr("+", Identifier("x"), Identifier("y")),
                    NatLiteral(10),
                ),
            ],
        )

    def test_x3_yields_y7(self):
        rt = EvidentRuntime()
        rt.load_schema(self._schema())
        result = rt.query("Sum10", given={"x": 3})
        assert result.satisfied is True
        assert result.bindings["x"] == 3
        assert result.bindings["y"] == 7

    def test_assert_ground_then_query(self):
        rt = EvidentRuntime()
        rt.load_schema(self._schema())
        rt.assert_ground("x", 3)
        # Query without passing given — the asserted fact is in the solver env
        result = rt.query("Sum10")
        assert result.satisfied is True

    def test_y_given_determines_x(self):
        rt = EvidentRuntime()
        rt.load_schema(self._schema())
        result = rt.query("Sum10", given={"y": 4})
        assert result.satisfied is True
        assert result.bindings["x"] == 6
        assert result.bindings["y"] == 4


# ---------------------------------------------------------------------------
# 3. Unsat query — contradictory constraints
# ---------------------------------------------------------------------------


class TestUnsatQuery:
    """
    schema Impossible
        n ∈ Nat
        n > 10
        n < 5
    Expected: satisfied=False.
    """

    def _schema(self) -> SchemaDecl:
        return mk_schema(
            "Impossible",
            params=[nat_param("n")],
            body=[
                ArithmeticConstraint(">", Identifier("n"), NatLiteral(10)),
                ArithmeticConstraint("<", Identifier("n"), NatLiteral(5)),
            ],
        )

    def test_is_unsat(self):
        rt = EvidentRuntime()
        rt.load_schema(self._schema())
        result = rt.query("Impossible")
        assert result.satisfied is False

    def test_bindings_empty_when_unsat(self):
        rt = EvidentRuntime()
        rt.load_schema(self._schema())
        result = rt.query("Impossible")
        assert result.bindings == {}

    def test_evidence_none_when_unsat(self):
        rt = EvidentRuntime()
        rt.load_schema(self._schema())
        result = rt.query("Impossible")
        assert result.evidence is None


# ---------------------------------------------------------------------------
# 4. Evidence in result
# ---------------------------------------------------------------------------


class TestEvidenceInResult:
    """When sat, result.evidence is populated with claim name and bindings."""

    def _schema(self) -> SchemaDecl:
        return mk_schema(
            "HasEvidence",
            params=[nat_param("x")],
            body=[
                ArithmeticConstraint(">", Identifier("x"), NatLiteral(0)),
            ],
        )

    def test_evidence_is_not_none_when_sat(self):
        rt = EvidentRuntime()
        rt.load_schema(self._schema())
        result = rt.query("HasEvidence")
        assert result.satisfied is True
        assert result.evidence is not None

    def test_evidence_claim_equals_schema_name(self):
        rt = EvidentRuntime()
        rt.load_schema(self._schema())
        result = rt.query("HasEvidence")
        assert result.evidence.claim == "HasEvidence"

    def test_evidence_bindings_contain_variables(self):
        rt = EvidentRuntime()
        rt.load_schema(self._schema())
        result = rt.query("HasEvidence")
        assert isinstance(result.evidence.bindings, dict)
        assert "x" in result.evidence.bindings

    def test_evidence_bindings_match_result_bindings(self):
        rt = EvidentRuntime()
        rt.load_schema(self._schema())
        result = rt.query("HasEvidence")
        assert result.evidence.bindings["x"] == result.bindings["x"]


# ---------------------------------------------------------------------------
# 5. Program loading
# ---------------------------------------------------------------------------


class TestProgramLoading:
    """load_program accepts a Program AST and registers all schemas/asserts."""

    def _program(self) -> Program:
        schema_a = mk_schema(
            "Alpha",
            params=[nat_param("a")],
            body=[ArithmeticConstraint(">", Identifier("a"), NatLiteral(0))],
        )
        schema_b = mk_schema(
            "Beta",
            params=[nat_param("b")],
            body=[ArithmeticConstraint("<", Identifier("b"), NatLiteral(100))],
        )
        assert_stmt = AssertStmt(
            name="x",
            value=NatLiteral(42),
            member_of=None,
            args=[],
        )
        return Program(statements=[schema_a, schema_b, assert_stmt])

    def test_both_schemas_registered(self):
        rt = EvidentRuntime()
        rt.load_program(self._program())
        assert "Alpha" in rt.schemas
        assert "Beta" in rt.schemas

    def test_can_query_alpha(self):
        rt = EvidentRuntime()
        rt.load_program(self._program())
        result = rt.query("Alpha")
        assert result.satisfied is True
        assert result.bindings["a"] > 0

    def test_can_query_beta(self):
        rt = EvidentRuntime()
        rt.load_program(self._program())
        result = rt.query("Beta")
        assert result.satisfied is True
        assert result.bindings["b"] < 100

    def test_assert_registers_fact(self):
        rt = EvidentRuntime()
        rt.load_program(self._program())
        # The assert x = 42 should be in the solver env
        assert "x" in rt.solver.env.bindings


# ---------------------------------------------------------------------------
# 6. Query by name after loading
# ---------------------------------------------------------------------------


class TestQueryByNameAfterLoading:
    """Load two schemas, query each by name — results are independent."""

    def test_two_schemas_independent(self):
        rt = EvidentRuntime()
        schema_a = mk_schema(
            "SchemaA",
            params=[nat_param("a")],
            body=[ArithmeticConstraint("=", Identifier("a"), NatLiteral(7))],
        )
        schema_b = mk_schema(
            "SchemaB",
            params=[nat_param("b")],
            body=[ArithmeticConstraint("=", Identifier("b"), NatLiteral(99))],
        )
        rt.load_schema(schema_a)
        rt.load_schema(schema_b)

        result_a = rt.query("SchemaA")
        result_b = rt.query("SchemaB")

        assert result_a.satisfied is True
        assert result_a.bindings["a"] == 7

        assert result_b.satisfied is True
        assert result_b.bindings["b"] == 99

    def test_unknown_schema_raises(self):
        rt = EvidentRuntime()
        with pytest.raises(KeyError, match="Unknown schema"):
            rt.query("DoesNotExist")


# ---------------------------------------------------------------------------
# 7. Session monotonicity
# ---------------------------------------------------------------------------


class TestSessionMonotonicity:
    """Asserted facts persist and do not interfere between queries."""

    def test_asserted_fact_persists(self):
        rt = EvidentRuntime()
        rt.assert_ground("deadline", 100)
        # The fact should be retrievable from the solver env
        assert "deadline" in rt.solver.env.bindings

    def test_two_queries_do_not_overwrite_each_other(self):
        rt = EvidentRuntime()

        schema_1 = mk_schema(
            "Q1",
            params=[nat_param("n")],
            body=[ArithmeticConstraint(">", Identifier("n"), NatLiteral(0))],
        )
        schema_2 = mk_schema(
            "Q2",
            params=[nat_param("m")],
            body=[ArithmeticConstraint("<", Identifier("m"), NatLiteral(50))],
        )
        rt.load_schema(schema_1)
        rt.load_schema(schema_2)

        r1 = rt.query("Q1")
        r2 = rt.query("Q2")

        assert r1.satisfied is True
        assert r2.satisfied is True
        # Each query returned independent bindings
        assert "n" in r1.bindings
        assert "m" in r2.bindings

    def test_evidence_base_grows_monotonically(self):
        rt = EvidentRuntime()
        schema = mk_schema(
            "Grow",
            params=[nat_param("x")],
            body=[ArithmeticConstraint(">", Identifier("x"), NatLiteral(0))],
        )
        rt.load_schema(schema)

        assert len(rt.evidence_base) == 0
        rt.query("Grow")
        assert len(rt.evidence_base) == 1
        rt.query("Grow")
        assert len(rt.evidence_base) == 2


# ---------------------------------------------------------------------------
# 8. Session class directly
# ---------------------------------------------------------------------------


class TestSession:
    def test_add_evidence_stores_it(self):
        session = Session()
        ev = Evidence(claim="MySchema", bindings={"x": 5})
        session.add_evidence(ev)
        assert len(session) == 1

    def test_is_established_true(self):
        session = Session()
        ev = Evidence(claim="MySchema", bindings={"x": 5})
        session.add_evidence(ev)
        assert session.is_established("MySchema") is True

    def test_is_established_false(self):
        session = Session()
        assert session.is_established("MySchema") is False

    def test_is_established_with_matching_bindings(self):
        session = Session()
        ev = Evidence(claim="MySchema", bindings={"x": 5, "y": 10})
        session.add_evidence(ev)
        assert session.is_established("MySchema", bindings={"x": 5}) is True

    def test_is_established_with_nonmatching_bindings(self):
        session = Session()
        ev = Evidence(claim="MySchema", bindings={"x": 5})
        session.add_evidence(ev)
        assert session.is_established("MySchema", bindings={"x": 99}) is False

    def test_assert_fact_monotonic(self):
        session = Session()
        session.assert_fact("n", 42)
        assert session.get_fact("n") == 42
        # Same value again — no error
        session.assert_fact("n", 42)
        assert session.get_fact("n") == 42

    def test_assert_fact_conflict_raises(self):
        session = Session()
        session.assert_fact("n", 42)
        with pytest.raises(ValueError, match="Cannot retract"):
            session.assert_fact("n", 99)

    def test_evidence_for(self):
        session = Session()
        ev1 = Evidence(claim="A", bindings={"x": 1})
        ev2 = Evidence(claim="B", bindings={"y": 2})
        ev3 = Evidence(claim="A", bindings={"x": 3})
        session.add_evidence(ev1)
        session.add_evidence(ev2)
        session.add_evidence(ev3)
        result = session.evidence_for("A")
        assert len(result) == 2
        assert all(e.claim == "A" for e in result)

    def test_all_claims(self):
        session = Session()
        session.add_evidence(Evidence(claim="X", bindings={}))
        session.add_evidence(Evidence(claim="Y", bindings={}))
        assert session.all_claims() == ["X", "Y"]


# ---------------------------------------------------------------------------
# 9. query_schema (inline schema)
# ---------------------------------------------------------------------------


class TestQuerySchema:
    def test_query_inline_schema(self):
        rt = EvidentRuntime()
        schema = mk_schema(
            "Inline",
            params=[nat_param("k")],
            body=[
                ArithmeticConstraint("=", Identifier("k"), NatLiteral(17)),
            ],
        )
        result = rt.query_schema(schema)
        assert result.satisfied is True
        assert result.bindings["k"] == 17

    def test_query_schema_registers_it(self):
        rt = EvidentRuntime()
        schema = mk_schema(
            "AutoReg",
            params=[nat_param("z")],
            body=[],
        )
        rt.query_schema(schema)
        # Now queryable by name
        result2 = rt.query("AutoReg")
        assert result2.satisfied is True


# ---------------------------------------------------------------------------
# 10. load_program with a ForwardRule (smoke test — no crash)
# ---------------------------------------------------------------------------


class TestForwardRuleLoading:
    def test_load_program_with_forward_rule_no_crash(self):
        rt = EvidentRuntime()
        fwd_rule = ForwardRule(
            premises=[ApplicationConstraint(name="node", args=[Identifier("n")])],
            conclusion=ApplicationConstraint(name="reachable", args=[Identifier("n"), Identifier("n")]),
        )
        program = Program(statements=[fwd_rule])
        rt.load_program(program)
        assert len(rt.forward_rules) == 1


# ---------------------------------------------------------------------------
# 11. load_program with assert of literals
# ---------------------------------------------------------------------------


class TestAssertLiteralLoading:
    def test_nat_literal_assert(self):
        rt = EvidentRuntime()
        stmt = AssertStmt(name="count", value=NatLiteral(5), member_of=None, args=[])
        rt.load_program(Program(statements=[stmt]))
        assert "count" in rt.solver.env.bindings

    def test_string_literal_assert(self):
        rt = EvidentRuntime()
        stmt = AssertStmt(name="label", value=StringLiteral("hello"), member_of=None, args=[])
        rt.load_program(Program(statements=[stmt]))
        assert "label" in rt.solver.env.bindings

    def test_bool_literal_assert(self):
        rt = EvidentRuntime()
        stmt = AssertStmt(name="flag", value=BoolLiteral(True), member_of=None, args=[])
        rt.load_program(Program(statements=[stmt]))
        assert "flag" in rt.solver.env.bindings

    def test_int_literal_assert(self):
        rt = EvidentRuntime()
        stmt = AssertStmt(name="offset", value=IntLiteral(-3), member_of=None, args=[])
        rt.load_program(Program(statements=[stmt]))
        assert "offset" in rt.solver.env.bindings


# ---------------------------------------------------------------------------
# 12. Multiple schemas, multiple queries — independence
# ---------------------------------------------------------------------------


class TestSchemaIndependence:
    """Each query gets a fresh Z3 solver; schemas don't pollute each other."""

    def test_unsat_schema_does_not_affect_sat_schema(self):
        rt = EvidentRuntime()
        bad = mk_schema(
            "Bad",
            params=[nat_param("n")],
            body=[
                ArithmeticConstraint(">", Identifier("n"), NatLiteral(10)),
                ArithmeticConstraint("<", Identifier("n"), NatLiteral(5)),
            ],
        )
        good = mk_schema(
            "Good",
            params=[nat_param("m")],
            body=[ArithmeticConstraint(">", Identifier("m"), NatLiteral(0))],
        )
        rt.load_schema(bad)
        rt.load_schema(good)

        assert rt.query("Bad").satisfied is False
        assert rt.query("Good").satisfied is True

    def test_queries_do_not_share_state(self):
        rt = EvidentRuntime()
        schema = mk_schema(
            "Vary",
            params=[nat_param("x")],
            body=[],
        )
        rt.load_schema(schema)
        r1 = rt.query("Vary", given={"x": 10})
        r2 = rt.query("Vary", given={"x": 20})
        assert r1.bindings["x"] == 10
        assert r2.bindings["x"] == 20
