"""
End-to-end tests: .ev source file → parse → runtime → query result.
"""
import pytest
from pathlib import Path
from runtime.src.runtime import EvidentRuntime
from runtime.src.ast_types import SchemaDecl, MembershipConstraint, ArithmeticConstraint, Identifier, NatLiteral

FIXTURES = Path(__file__).parent.parent.parent / "parser" / "tests" / "fixtures" / "valid"


def test_load_source_basic_schema():
    rt = EvidentRuntime()
    rt.load_source("""
schema SimpleNat
    n ∈ Nat
    n > 5
""")
    result = rt.query("SimpleNat")
    assert result.satisfied
    assert result.bindings["n"] > 5


def test_load_source_unsat():
    rt = EvidentRuntime()
    rt.load_source("""
schema Impossible
    n ∈ Nat
    n > 10
    n < 3
""")
    result = rt.query("Impossible")
    assert not result.satisfied


def test_load_source_with_given():
    rt = EvidentRuntime()
    rt.load_source("""
schema SumTo10
    x ∈ Nat
    y ∈ Nat
    x + y = 10
""")
    result = rt.query("SumTo10", given={"x": 3})
    assert result.satisfied
    assert result.bindings["y"] == 7


def test_load_file_fixture_01():
    """Load the basic Task schema fixture and query it."""
    rt = EvidentRuntime()
    rt.load_file(FIXTURES / "01-basic-schema.ev")
    result = rt.query("Task")
    assert result.satisfied
    assert "id" in result.bindings
    assert "duration" in result.bindings
    assert "deadline" in result.bindings


def test_load_file_fixture_02():
    """ValidTask: duration < deadline should be satisfied."""
    rt = EvidentRuntime()
    rt.load_file(FIXTURES / "02-schema-with-constraint.ev")
    result = rt.query("ValidTask")
    assert result.satisfied
    assert result.bindings["duration"] < result.bindings["deadline"]


def test_load_file_fixture_03():
    """Point type alias."""
    rt = EvidentRuntime()
    rt.load_file(FIXTURES / "03-type-alias.ev")
    result = rt.query("Point")
    assert result.satisfied
    assert "x" in result.bindings
    assert "y" in result.bindings


def test_load_source_multiple_schemas():
    """Multiple schemas loaded, each queryable independently."""
    rt = EvidentRuntime()
    rt.load_source("""
schema A
    x ∈ Nat
    x > 0

schema B
    y ∈ Nat
    y > 100
""")
    a = rt.query("A")
    b = rt.query("B")
    assert a.satisfied and a.bindings["x"] > 0
    assert b.satisfied and b.bindings["y"] > 100


def test_load_source_evidence():
    """Query returns an evidence term."""
    rt = EvidentRuntime()
    rt.load_source("""
schema HasEvidence
    n ∈ Nat
    n > 42
""")
    result = rt.query("HasEvidence")
    assert result.satisfied
    assert result.evidence is not None
    assert result.evidence.claim == "HasEvidence"
    assert result.evidence.bindings["n"] > 42


def test_load_source_assert_then_query():
    """Assert a ground fact, then use it in a query."""
    rt = EvidentRuntime()
    rt.load_source("""
schema CheckBound
    n ∈ Nat
    budget ∈ Nat
    n < budget
""")
    rt.assert_ground("budget", 1000)
    result = rt.query("CheckBound", given={"n": 500})
    assert result.satisfied


def test_load_file_query_unbound():
    """Load fixture 07 (unbound variable assert) and verify it parses."""
    rt = EvidentRuntime()
    rt.load_file(FIXTURES / "07-assert-unbound.ev")
    # Fixture asserts 'result ∈ Nat' and 'schedule ∈ Set Assignment'
    # Just verify it loads without error
    assert True


def test_end_to_end_full_pipeline():
    """Full pipeline: source → parse → load → query → evidence → JSON."""
    source = """
schema Bounded
    lo ∈ Nat
    hi ∈ Nat
    n  ∈ Nat
    lo < n
    n  < hi
"""
    rt = EvidentRuntime()
    rt.load_source(source)
    result = rt.query("Bounded", given={"lo": 10, "hi": 20})
    assert result.satisfied
    assert 10 < result.bindings["n"] < 20
    assert result.evidence is not None
    # Verify evidence serializes
    ev_dict = result.evidence.to_dict()
    assert ev_dict["claim"] == "Bounded"
    ev_json = result.evidence.to_json()
    assert "Bounded" in ev_json
