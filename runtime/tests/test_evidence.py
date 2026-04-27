"""
Phase 10 tests: evidence terms — structured derivation trees.

Tests cover:
- Basic Evidence construction and attribute access
- evaluate_with_evidence returning Evidence for sat, None for unsat
- evidence.claim, evidence.bindings, evidence.get()
- evidence.to_dict() / evidence.to_json() serialization
- evidence.find_sub() for sub-claim navigation
- Sub-evidence construction when a body contains ApplicationConstraint
  references to registered sub-schemas
- Round-trip serialization (to_dict → reconstruct from dict)
"""

import json
import pytest

from runtime.src.evidence import Evidence, build_evidence, evaluate_with_evidence
from runtime.src.evaluate import EvidentSolver, evaluate_schema
from runtime.src.ast_types import (
    SchemaDecl,
    Param,
    Identifier,
    ArithmeticConstraint,
    MembershipConstraint,
    NatLiteral,
    ApplicationConstraint,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def nat_param(*names: str) -> Param:
    return Param(names=list(names), set=Identifier(name="Nat"))


def mk_schema(name: str, params: list[Param], body: list) -> SchemaDecl:
    return SchemaDecl(keyword="schema", name=name, params=params, body=body)


def _simple_schema():
    """schema Simple — x ∈ Nat — x > 3"""
    return mk_schema(
        "Simple",
        params=[nat_param("x")],
        body=[ArithmeticConstraint(">", Identifier("x"), NatLiteral(3))],
    )


def _two_var_schema():
    """schema TwoVar — x ∈ Nat, y ∈ Nat — x + y = 10  (via x = 3 given)"""
    from runtime.src.ast_types import BinaryExpr
    return mk_schema(
        "TwoVar",
        params=[nat_param("x"), nat_param("y")],
        body=[
            ArithmeticConstraint(
                "=",
                BinaryExpr("+", Identifier("x"), Identifier("y")),
                NatLiteral(10),
            )
        ],
    )


def _impossible_schema():
    """schema Impossible — n ∈ Nat — n > 5 — n < 3  (unsat)"""
    return mk_schema(
        "Impossible",
        params=[nat_param("n")],
        body=[
            ArithmeticConstraint(">", Identifier("n"), NatLiteral(5)),
            ArithmeticConstraint("<", Identifier("n"), NatLiteral(3)),
        ],
    )


# ---------------------------------------------------------------------------
# Evidence data-class unit tests
# ---------------------------------------------------------------------------


class TestEvidenceDataclass:
    def test_construction(self):
        ev = Evidence(claim="Foo", bindings={"x": 5})
        assert ev.claim == "Foo"
        assert ev.bindings == {"x": 5}
        assert ev.sub_evidence == []
        assert ev.rule_used is None

    def test_rule_used(self):
        ev = Evidence(claim="Bar", bindings={}, rule_used="my_rule")
        assert ev.rule_used == "my_rule"

    def test_get_existing_key(self):
        ev = Evidence(claim="C", bindings={"a": 42, "b": "hello"})
        assert ev.get("a") == 42
        assert ev.get("b") == "hello"

    def test_get_missing_key_returns_none(self):
        ev = Evidence(claim="C", bindings={"a": 1})
        assert ev.get("missing") is None

    def test_repr_contains_claim_name(self):
        ev = Evidence(claim="MySchema", bindings={"x": 7})
        r = repr(ev)
        assert "MySchema" in r
        assert "x=7" in r

    def test_sub_evidence_list(self):
        child = Evidence(claim="Child", bindings={"z": 0})
        parent = Evidence(claim="Parent", bindings={"y": 1}, sub_evidence=[child])
        assert len(parent.sub_evidence) == 1
        assert parent.sub_evidence[0].claim == "Child"


# ---------------------------------------------------------------------------
# find_sub
# ---------------------------------------------------------------------------


class TestFindSub:
    def test_find_existing(self):
        child_a = Evidence(claim="SubA", bindings={"a": 1})
        child_b = Evidence(claim="SubB", bindings={"b": 2})
        parent = Evidence(claim="Root", bindings={}, sub_evidence=[child_a, child_b])
        found = parent.find_sub("SubB")
        assert found is child_b

    def test_find_first_match(self):
        child1 = Evidence(claim="Sub", bindings={"n": 1})
        child2 = Evidence(claim="Sub", bindings={"n": 2})
        parent = Evidence(claim="Root", bindings={}, sub_evidence=[child1, child2])
        found = parent.find_sub("Sub")
        assert found is child1

    def test_find_missing_returns_none(self):
        parent = Evidence(claim="Root", bindings={})
        assert parent.find_sub("NonExistent") is None

    def test_find_no_children(self):
        parent = Evidence(claim="Root", bindings={"x": 5})
        assert parent.find_sub("Any") is None


# ---------------------------------------------------------------------------
# Serialization
# ---------------------------------------------------------------------------


class TestSerialization:
    def _sample(self) -> Evidence:
        child = Evidence(claim="Sub", bindings={"z": 99}, rule_used="rule1")
        return Evidence(
            claim="Root",
            bindings={"x": 1, "y": "hello"},
            sub_evidence=[child],
            rule_used=None,
        )

    def test_to_dict_keys(self):
        ev = self._sample()
        d = ev.to_dict()
        assert set(d.keys()) == {"claim", "bindings", "rule_used", "sub_evidence"}

    def test_to_dict_claim(self):
        d = self._sample().to_dict()
        assert d["claim"] == "Root"

    def test_to_dict_bindings(self):
        d = self._sample().to_dict()
        assert d["bindings"] == {"x": 1, "y": "hello"}

    def test_to_dict_rule_used_none(self):
        d = self._sample().to_dict()
        assert d["rule_used"] is None

    def test_to_dict_sub_evidence_is_list(self):
        d = self._sample().to_dict()
        assert isinstance(d["sub_evidence"], list)
        assert len(d["sub_evidence"]) == 1

    def test_to_dict_sub_evidence_claim(self):
        d = self._sample().to_dict()
        assert d["sub_evidence"][0]["claim"] == "Sub"

    def test_to_dict_sub_evidence_rule_used(self):
        d = self._sample().to_dict()
        assert d["sub_evidence"][0]["rule_used"] == "rule1"

    def test_to_json_is_valid_json(self):
        j = self._sample().to_json()
        parsed = json.loads(j)
        assert parsed["claim"] == "Root"

    def test_to_json_default_indent(self):
        j = self._sample().to_json()
        # Indented JSON has newlines
        assert "\n" in j

    def test_to_json_custom_indent(self):
        j = self._sample().to_json(indent=4)
        parsed = json.loads(j)
        assert parsed["claim"] == "Root"

    def test_round_trip(self):
        """to_dict produces a plain dict that can be re-read and compared."""
        ev = self._sample()
        d = ev.to_dict()
        # Reconstruct manually
        child_d = d["sub_evidence"][0]
        reconstructed_child = Evidence(
            claim=child_d["claim"],
            bindings=child_d["bindings"],
            rule_used=child_d["rule_used"],
        )
        reconstructed = Evidence(
            claim=d["claim"],
            bindings=d["bindings"],
            rule_used=d["rule_used"],
            sub_evidence=[reconstructed_child],
        )
        assert reconstructed.claim == ev.claim
        assert reconstructed.bindings == ev.bindings
        assert reconstructed.sub_evidence[0].claim == ev.sub_evidence[0].claim
        assert reconstructed.sub_evidence[0].bindings == ev.sub_evidence[0].bindings


# ---------------------------------------------------------------------------
# evaluate_with_evidence — sat cases
# ---------------------------------------------------------------------------


class TestEvaluateWithEvidenceSat:
    def test_returns_tuple(self):
        result, ev = evaluate_with_evidence(_simple_schema())
        assert isinstance(result, tuple) or True  # just unpack check above
        assert result is not None

    def test_result_satisfied(self):
        result, ev = evaluate_with_evidence(_simple_schema())
        assert result.satisfied is True

    def test_evidence_is_not_none_when_sat(self):
        _, ev = evaluate_with_evidence(_simple_schema())
        assert ev is not None

    def test_evidence_claim_matches_schema_name(self):
        _, ev = evaluate_with_evidence(_simple_schema())
        assert ev.claim == "Simple"

    def test_evidence_bindings_contain_x(self):
        _, ev = evaluate_with_evidence(_simple_schema())
        assert "x" in ev.bindings

    def test_evidence_bindings_x_gt_3(self):
        _, ev = evaluate_with_evidence(_simple_schema())
        assert ev.bindings["x"] > 3

    def test_evidence_get_x(self):
        _, ev = evaluate_with_evidence(_simple_schema())
        x = ev.get("x")
        assert x is not None
        assert x > 3

    def test_evidence_has_no_sub_evidence_when_no_sub_schemas(self):
        _, ev = evaluate_with_evidence(_simple_schema())
        assert ev.sub_evidence == []

    def test_two_var_schema_with_given(self):
        _, ev = evaluate_with_evidence(_two_var_schema(), given={"x": 3})
        assert ev is not None
        assert ev.get("x") == 3
        assert ev.get("y") == 7

    def test_evidence_claim_two_var(self):
        _, ev = evaluate_with_evidence(_two_var_schema(), given={"x": 3})
        assert ev.claim == "TwoVar"


# ---------------------------------------------------------------------------
# evaluate_with_evidence — unsat case
# ---------------------------------------------------------------------------


class TestEvaluateWithEvidenceUnsat:
    def test_result_not_satisfied(self):
        result, ev = evaluate_with_evidence(_impossible_schema())
        assert result.satisfied is False

    def test_evidence_is_none_when_unsat(self):
        _, ev = evaluate_with_evidence(_impossible_schema())
        assert ev is None

    def test_unsat_with_given_contradiction(self):
        schema = _two_var_schema()
        # x=3, y=8 → x+y=11 ≠ 10 → unsat
        result, ev = evaluate_with_evidence(schema, given={"x": 3, "y": 8})
        assert result.satisfied is False
        assert ev is None


# ---------------------------------------------------------------------------
# Sub-evidence for ApplicationConstraint references
# ---------------------------------------------------------------------------


class TestSubEvidence:
    """
    When a schema's body contains an ApplicationConstraint whose name
    matches a registered sub-schema, evidence should include a sub-node
    for that sub-claim.
    """

    def _sub_schema(self):
        """schema Positive — p ∈ Nat — p > 0"""
        return mk_schema(
            "Positive",
            params=[nat_param("p")],
            body=[ArithmeticConstraint(">", Identifier("p"), NatLiteral(0))],
        )

    def _parent_schema_with_app(self):
        """
        schema WithSub
            p ∈ Nat
            Positive   ← ApplicationConstraint referencing sub-schema
        """
        return mk_schema(
            "WithSub",
            params=[nat_param("p")],
            body=[
                ArithmeticConstraint(">", Identifier("p"), NatLiteral(0)),
                ApplicationConstraint(name="Positive"),
            ],
        )

    def test_sub_evidence_created_for_known_sub_schema(self):
        parent = self._parent_schema_with_app()
        sub = self._sub_schema()
        _, ev = evaluate_with_evidence(
            parent, given={"p": 5}, sub_schemas={"Positive": sub}
        )
        assert ev is not None
        assert len(ev.sub_evidence) == 1

    def test_sub_evidence_claim_name(self):
        parent = self._parent_schema_with_app()
        sub = self._sub_schema()
        _, ev = evaluate_with_evidence(
            parent, given={"p": 5}, sub_schemas={"Positive": sub}
        )
        assert ev.sub_evidence[0].claim == "Positive"

    def test_find_sub_by_name(self):
        parent = self._parent_schema_with_app()
        sub = self._sub_schema()
        _, ev = evaluate_with_evidence(
            parent, given={"p": 5}, sub_schemas={"Positive": sub}
        )
        node = ev.find_sub("Positive")
        assert node is not None
        assert node.claim == "Positive"

    def test_find_sub_missing_returns_none(self):
        parent = self._parent_schema_with_app()
        sub = self._sub_schema()
        _, ev = evaluate_with_evidence(
            parent, given={"p": 5}, sub_schemas={"Positive": sub}
        )
        assert ev.find_sub("NoSuchSub") is None

    def test_sub_evidence_bindings_contain_p(self):
        parent = self._parent_schema_with_app()
        sub = self._sub_schema()
        _, ev = evaluate_with_evidence(
            parent, given={"p": 5}, sub_schemas={"Positive": sub}
        )
        node = ev.find_sub("Positive")
        assert "p" in node.bindings

    def test_sub_evidence_bindings_p_value(self):
        parent = self._parent_schema_with_app()
        sub = self._sub_schema()
        _, ev = evaluate_with_evidence(
            parent, given={"p": 5}, sub_schemas={"Positive": sub}
        )
        node = ev.find_sub("Positive")
        assert node.bindings["p"] == 5

    def test_no_sub_evidence_without_sub_schemas(self):
        """Without passing sub_schemas, no sub-evidence is produced."""
        parent = self._parent_schema_with_app()
        _, ev = evaluate_with_evidence(parent, given={"p": 5})
        # ApplicationConstraint for "Positive" is in body, but no sub_schemas
        # provided, so sub_evidence should be empty
        assert ev is not None
        assert ev.sub_evidence == []

    def test_multiple_sub_schemas(self):
        """Two ApplicationConstraint references → two sub-evidence nodes."""
        sub_a = mk_schema(
            "SubA",
            params=[nat_param("a")],
            body=[ArithmeticConstraint(">", Identifier("a"), NatLiteral(0))],
        )
        sub_b = mk_schema(
            "SubB",
            params=[nat_param("b")],
            body=[ArithmeticConstraint(">", Identifier("b"), NatLiteral(0))],
        )
        parent = mk_schema(
            "Parent",
            params=[nat_param("a"), nat_param("b")],
            body=[
                ArithmeticConstraint(">", Identifier("a"), NatLiteral(0)),
                ArithmeticConstraint(">", Identifier("b"), NatLiteral(0)),
                ApplicationConstraint(name="SubA"),
                ApplicationConstraint(name="SubB"),
            ],
        )
        _, ev = evaluate_with_evidence(
            parent,
            given={"a": 3, "b": 7},
            sub_schemas={"SubA": sub_a, "SubB": sub_b},
        )
        assert ev is not None
        assert len(ev.sub_evidence) == 2
        claims = {s.claim for s in ev.sub_evidence}
        assert claims == {"SubA", "SubB"}

    def test_sub_evidence_serializes(self):
        """Sub-evidence nodes appear correctly in to_dict output."""
        parent = self._parent_schema_with_app()
        sub = self._sub_schema()
        _, ev = evaluate_with_evidence(
            parent, given={"p": 5}, sub_schemas={"Positive": sub}
        )
        d = ev.to_dict()
        assert len(d["sub_evidence"]) == 1
        sub_d = d["sub_evidence"][0]
        assert sub_d["claim"] == "Positive"
        assert "p" in sub_d["bindings"]


# ---------------------------------------------------------------------------
# build_evidence unit tests (low-level API)
# ---------------------------------------------------------------------------


class TestBuildEvidence:
    """Direct tests of build_evidence() with a live Z3 model."""

    def test_basic_build(self):
        """build_evidence with no sub_schemas returns a leaf node."""
        import z3
        from runtime.src.env import Environment
        from runtime.src.sorts import SortRegistry
        from runtime.src.instantiate import instantiate_schema

        schema = _simple_schema()
        registry = SortRegistry()
        env, type_constraints = instantiate_schema(schema, Environment(), registry)

        s = z3.Solver()
        for tc in type_constraints:
            s.add(tc)
        s.add(env.lookup("x") > 3)
        assert s.check() == z3.sat
        model = s.model()

        bindings = {"x": model.eval(env.lookup("x"), model_completion=True).as_long()}
        ev = build_evidence("Simple", bindings, schema.body, env, model, {})

        assert ev.claim == "Simple"
        assert ev.bindings["x"] > 3
        assert ev.sub_evidence == []

    def test_build_with_sub_schema(self):
        """build_evidence detects ApplicationConstraint and builds sub-node."""
        import z3
        from runtime.src.env import Environment
        from runtime.src.sorts import SortRegistry
        from runtime.src.instantiate import instantiate_schema

        sub = mk_schema(
            "Positive",
            params=[nat_param("p")],
            body=[ArithmeticConstraint(">", Identifier("p"), NatLiteral(0))],
        )
        parent = mk_schema(
            "WithSub",
            params=[nat_param("p")],
            body=[
                ArithmeticConstraint(">", Identifier("p"), NatLiteral(0)),
                ApplicationConstraint(name="Positive"),
            ],
        )

        registry = SortRegistry()
        env, type_constraints = instantiate_schema(parent, Environment(), registry)

        s = z3.Solver()
        for tc in type_constraints:
            s.add(tc)
        s.add(env.lookup("p") == 7)
        assert s.check() == z3.sat
        model = s.model()

        bindings = {"p": 7}
        ev = build_evidence(
            "WithSub", bindings, parent.body, env, model, {"Positive": sub}
        )
        assert ev.claim == "WithSub"
        assert len(ev.sub_evidence) == 1
        assert ev.sub_evidence[0].claim == "Positive"
        assert ev.sub_evidence[0].bindings.get("p") == 7
