"""
Tests for enum type declarations and constraint solving.

Syntax:
    type Color = Red | Green | Blue

    schema Pixel
        c ∈ Color
        c ≠ Blue
"""
import pytest
from runtime.src.runtime import EvidentRuntime


# ---------------------------------------------------------------------------
# Parser / AST
# ---------------------------------------------------------------------------

def test_enum_parses():
    from parser.src.parser import parse
    from parser.src.ast import EnumDecl
    prog = parse("type Color = Red | Green | Blue\n")
    assert len(prog.statements) == 1
    decl = prog.statements[0]
    assert isinstance(decl, EnumDecl)
    assert decl.name == "Color"
    assert decl.variants == ["Red", "Green", "Blue"]


def test_enum_two_variants():
    from parser.src.parser import parse
    from parser.src.ast import EnumDecl
    prog = parse("type Bit = Zero | One\n")
    decl = prog.statements[0]
    assert isinstance(decl, EnumDecl)
    assert decl.variants == ["Zero", "One"]


def test_enum_many_variants():
    from parser.src.parser import parse
    from parser.src.ast import EnumDecl
    prog = parse("type Day = Mon | Tue | Wed | Thu | Fri | Sat | Sun\n")
    decl = prog.statements[0]
    assert len(decl.variants) == 7


# ---------------------------------------------------------------------------
# Sort registry
# ---------------------------------------------------------------------------

def test_declare_algebraic_registers_sort():
    from runtime.src.sorts import SortRegistry
    reg = SortRegistry()
    sort = reg.declare_algebraic("Color", ["Red", "Green", "Blue"])
    assert reg.get("Color") is sort


def test_declare_algebraic_registers_constructors():
    from runtime.src.sorts import SortRegistry
    reg = SortRegistry()
    reg.declare_algebraic("Color", ["Red", "Green", "Blue"])
    assert reg.get_constructor("Red") is not None
    assert reg.get_constructor("Green") is not None
    assert reg.get_constructor("Blue") is not None
    assert reg.get_constructor("Yellow") is None


def test_declare_algebraic_idempotent():
    from runtime.src.sorts import SortRegistry
    reg = SortRegistry()
    s1 = reg.declare_algebraic("Color", ["Red", "Green"])
    s2 = reg.declare_algebraic("Color", ["Red", "Green"])
    assert s1 is s2


def test_constructors_are_distinct():
    import z3
    from runtime.src.sorts import SortRegistry
    reg = SortRegistry()
    reg.declare_algebraic("Color", ["Red", "Green", "Blue"])
    red = reg.get_constructor("Red")
    green = reg.get_constructor("Green")
    blue = reg.get_constructor("Blue")
    s = z3.Solver()
    s.add(red == green)
    assert s.check() == z3.unsat
    s2 = z3.Solver()
    s2.add(red != blue)
    assert s2.check() == z3.sat


# ---------------------------------------------------------------------------
# Runtime: basic satisfiability
# ---------------------------------------------------------------------------

def _rt(source):
    rt = EvidentRuntime()
    rt.load_source(source)
    return rt


def test_enum_sat_free_variable():
    rt = _rt("""
type Color = Red | Green | Blue

schema Pixel
    c ∈ Color
""")
    r = rt.query("Pixel")
    assert r.satisfied
    assert r.bindings["c"] in ("Red", "Green", "Blue")


def test_enum_neq_constraint_sat():
    rt = _rt("""
type Color = Red | Green | Blue

schema Pixel
    c ∈ Color
    c ≠ Blue
""")
    r = rt.query("Pixel")
    assert r.satisfied
    assert r.bindings["c"] in ("Red", "Green")


def test_enum_neq_all_but_one():
    rt = _rt("""
type Color = Red | Green | Blue

schema Pixel
    c ∈ Color
    c ≠ Red
    c ≠ Green
""")
    r = rt.query("Pixel")
    assert r.satisfied
    assert r.bindings["c"] == "Blue"


def test_enum_neq_all_variants_unsat():
    rt = _rt("""
type Color = Red | Green | Blue

schema Pixel
    c ∈ Color
    c ≠ Red
    c ≠ Green
    c ≠ Blue
""")
    r = rt.query("Pixel")
    assert not r.satisfied


def test_enum_eq_constraint():
    rt = _rt("""
type Status = Active | Inactive | Pending

schema Job
    s ∈ Status
    s = Active
""")
    r = rt.query("Job")
    assert r.satisfied
    assert r.bindings["s"] == "Active"


# ---------------------------------------------------------------------------
# Runtime: given bindings
# ---------------------------------------------------------------------------

def test_given_valid_variant():
    rt = _rt("""
type Color = Red | Green | Blue

schema Pixel
    c ∈ Color
    c ≠ Blue
""")
    r = rt.query("Pixel", given={"c": "Green"})
    assert r.satisfied
    assert r.bindings["c"] == "Green"


def test_given_excluded_variant_is_unsat():
    rt = _rt("""
type Color = Red | Green | Blue

schema Pixel
    c ∈ Color
    c ≠ Blue
""")
    r = rt.query("Pixel", given={"c": "Blue"})
    assert not r.satisfied


def test_given_any_valid_variant():
    rt = _rt("""
type Status = Active | Inactive | Pending

schema Task
    s ∈ Status
    s ≠ Inactive
""")
    for variant in ("Active", "Pending"):
        r = rt.query("Task", given={"s": variant})
        assert r.satisfied, f"Expected sat for s={variant}"
    r = rt.query("Task", given={"s": "Inactive"})
    assert not r.satisfied


# ---------------------------------------------------------------------------
# Runtime: enums mixed with numeric variables
# ---------------------------------------------------------------------------

def test_enum_and_nat():
    rt = _rt("""
type Status = Active | Inactive | Pending

schema Task
    id     ∈ Nat
    status ∈ Status
    id > 0
    id < 10
    status ≠ Inactive
""")
    r = rt.query("Task")
    assert r.satisfied
    assert 0 < r.bindings["id"] < 10
    assert r.bindings["status"] in ("Active", "Pending")


def test_enum_and_nat_given_status():
    rt = _rt("""
type Priority = Low | Medium | High

schema Item
    score    ∈ Nat
    priority ∈ Priority
    score > 5
    score < 20
    priority ≠ Low
""")
    r = rt.query("Item", given={"priority": "High"})
    assert r.satisfied
    assert r.bindings["priority"] == "High"
    assert 5 < r.bindings["score"] < 20


# ---------------------------------------------------------------------------
# Runtime: multiple enum types in one program
# ---------------------------------------------------------------------------

def test_multiple_enum_types():
    rt = _rt("""
type Color = Red | Green | Blue
type Size  = Small | Medium | Large

schema Widget
    c ∈ Color
    s ∈ Size
    c ≠ Red
    s ≠ Large
""")
    r = rt.query("Widget")
    assert r.satisfied
    assert r.bindings["c"] in ("Green", "Blue")
    assert r.bindings["s"] in ("Small", "Medium")


def test_multiple_enum_types_unsat():
    rt = _rt("""
type Bit = Zero | One

schema Both
    a ∈ Bit
    b ∈ Bit
    a = Zero
    b = One
    a = b
""")
    r = rt.query("Both")
    assert not r.satisfied
