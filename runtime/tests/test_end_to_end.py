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


# ── Parameter syntax tests ────────────────────────────────────────────────────

def _rt(src):
    rt = EvidentRuntime()
    rt.load_source(src)
    return rt


def test_params_basic_query():
    """Param syntax produces same result as body-style declaration."""
    rt_body = _rt("schema S\n    a ∈ Nat\n    b ∈ Nat\n    a < b\n")
    rt_param = _rt("schema S(a ∈ Nat, b ∈ Nat)\n    a < b\n")
    rb = rt_body.query("S")
    rp = rt_param.query("S")
    assert rb.satisfied == rp.satisfied
    assert rp.bindings["a"] < rp.bindings["b"]


def test_params_unsat():
    rt = _rt("schema S(a ∈ Nat, b ∈ Nat)\n    a < b\n    b < a\n")
    assert not rt.query("S").satisfied


def test_params_given():
    rt = _rt("schema S(a ∈ Nat, b ∈ Nat)\n    a + b = 10\n")
    r = rt.query("S", given={"a": 3})
    assert r.satisfied
    assert r.bindings["b"] == 7


def test_params_multiline():
    rt = _rt("schema S(\n    a ∈ Nat,\n    b ∈ Nat\n)\n    a < b\n")
    r = rt.query("S")
    assert r.satisfied
    assert r.bindings["a"] < r.bindings["b"]


def test_params_no_body():
    rt = _rt("schema S(n ∈ Nat)\n")
    r = rt.query("S")
    assert r.satisfied
    assert r.bindings["n"] >= 0


def test_params_enum():
    rt = _rt(
        "type Color = Red | Green | Blue\n"
        "schema S(c ∈ Color)\n    c ≠ Red\n"
    )
    r = rt.query("S")
    assert r.satisfied
    assert r.bindings["c"] in ("Green", "Blue")


def test_params_sampling_is_diverse():
    """Params syntax must produce diverse samples, not always min values."""
    import importlib.util, pathlib
    sampler_path = pathlib.Path(__file__).parent.parent.parent / 'ide' / 'backend' / 'sampler.py'
    spec = importlib.util.spec_from_file_location('sampler', sampler_path)
    sampler_mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(sampler_mod)
    random_seed_sample = sampler_mod.random_seed_sample

    src = "schema S(a ∈ Nat, b ∈ Nat)\n    a < b\n    a < 20\n    b < 30\n"
    results = random_seed_sample(src, "S", {}, 10)
    assert len(results) >= 5, f"Only got {len(results)} samples"
    a_values = {r.bindings["a"] for r in results}
    assert len(a_values) > 1, "All samples have the same 'a' — hints not being applied to params"


def test_params_mix_with_body():
    """Params and body-level declarations can coexist."""
    rt = _rt(
        "schema S(x ∈ Nat)\n"
        "    y ∈ Nat\n"
        "    y = x * 2\n"
        "    x < 10\n"
    )
    r = rt.query("S", given={"x": 4})
    assert r.satisfied
    assert r.bindings["y"] == 8


# ── Multi-name body membership  x, y ∈ Nat ───────────────────────────────────

def test_multi_name_body_two_vars():
    rt = _rt("schema S\n    x, y ∈ Nat\n    x + y = 10\n")
    r = rt.query("S")
    assert r.satisfied
    assert r.bindings["x"] + r.bindings["y"] == 10

def test_multi_name_body_three_vars():
    rt = _rt("schema S\n    x, y, z ∈ Nat\n    x < y\n    y < z\n    z < 10\n")
    r = rt.query("S")
    assert r.satisfied
    b = r.bindings
    assert b["x"] < b["y"] < b["z"] < 10

def test_multi_name_body_real():
    rt = _rt("schema Circle\n    x, y ∈ Real\n    x * x + y * y < 1.0\n")
    r = rt.query("Circle")
    assert r.satisfied
    assert r.bindings["x"] ** 2 + r.bindings["y"] ** 2 < 1.0

def test_multi_name_body_mixed_with_single():
    """Multi-name and single-name declarations can coexist."""
    rt = _rt("schema S\n    a, b ∈ Nat\n    c ∈ Nat\n    a < b\n    b < c\n")
    r = rt.query("S")
    assert r.satisfied
    assert r.bindings["a"] < r.bindings["b"] < r.bindings["c"]

def test_multi_name_body_given():
    rt = _rt("schema S\n    x, y ∈ Nat\n    x + y = 20\n")
    r = rt.query("S", given={"x": 7})
    assert r.satisfied
    assert r.bindings["y"] == 13

def test_multi_name_body_diverse_samples():
    import importlib.util, pathlib
    sampler_path = pathlib.Path(__file__).parent.parent.parent / 'ide' / 'backend' / 'sampler.py'
    spec = importlib.util.spec_from_file_location('sampler', sampler_path)
    mod = importlib.util.module_from_spec(spec); spec.loader.exec_module(mod)

    src = "schema S\n    a, b ∈ Nat\n    a < b\n    a < 20\n    b < 30\n"
    results = mod.random_seed_sample(src, "S", {}, 10)
    assert len(results) >= 5
    assert len({r.bindings["a"] for r in results}) > 1
