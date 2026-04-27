"""
Variable scoping tests for the Evident runtime.

Documents the scoping rules:
  1. Variables are local to the schema they are declared in.
  2. Queries on independent schemas do not share variables.
  3. Composed sub-schemas have their own variable scope (internal prefix).
  4. Enum variant names are global and must be unique across all enum types.
  5. A type name (e.g. Color) can be reused as a variable name inside a schema
     without conflicting with the type declaration.
"""
import pytest
from runtime.src.runtime import EvidentRuntime
from runtime.src.sorts import SortRegistry


# ---------------------------------------------------------------------------
# 1. Variables are local to their schema
# ---------------------------------------------------------------------------

def test_variable_local_to_schema():
    """x in schema A is not visible when querying schema B."""
    rt = EvidentRuntime()
    rt.load_source("""
schema A
    x ∈ Nat
    x > 10

schema B
    x ∈ Nat
    x < 5
""")
    rA = rt.query("A")
    rB = rt.query("B")
    assert rA.bindings["x"] > 10
    assert rB.bindings["x"] < 5


def test_same_var_name_independent_queries():
    """Querying A then B does not bleed A's x into B's evaluation."""
    rt = EvidentRuntime()
    rt.load_source("""
schema A
    n ∈ Nat
    n > 100

schema B
    n ∈ Nat
    n < 3
""")
    rt.query("A")            # establishes n > 100 in A's env — must not leak
    rB = rt.query("B")
    assert rB.satisfied
    assert rB.bindings["n"] < 3


def test_given_only_affects_target_schema():
    """given={'x': 3} applies to the queried schema, not any other."""
    rt = EvidentRuntime()
    rt.load_source("""
schema A
    x ∈ Nat
    x > 10

schema B
    x ∈ Nat
    x < 5
""")
    rA = rt.query("A", given={"x": 3})   # violates A's x > 10
    rB = rt.query("B", given={"x": 3})   # satisfies B's x < 5

    assert not rA.satisfied
    assert rB.satisfied
    assert rB.bindings["x"] == 3


# ---------------------------------------------------------------------------
# 2. Conflicting constraints on the same name in parent + child are isolated
# ---------------------------------------------------------------------------

def test_composed_child_var_does_not_leak_into_parent():
    """
    Parent has n < 5, Child has n > 100. They share the name 'n' but must
    remain separate variables. If they leaked, the system would be UNSAT.
    """
    rt = EvidentRuntime()
    rt.load_source("""
schema Child
    n ∈ Nat
    n > 100

schema Parent
    n ∈ Nat
    n < 5
    child ∈ Child
""")
    r = rt.query("Parent")
    assert r.satisfied
    # Parent's n satisfies its own constraint, not Child's
    assert r.bindings["n"] < 5


def test_composed_schemas_independent_nat_vars():
    """Two composed schemas with conflicting bounds on the same name stay sat."""
    rt = EvidentRuntime()
    rt.load_source("""
schema Inner
    x ∈ Nat
    x > 50

schema Outer
    x ∈ Nat
    x < 10
    inner ∈ Inner
""")
    r = rt.query("Outer")
    assert r.satisfied
    assert r.bindings["x"] < 10


# ---------------------------------------------------------------------------
# 3. Type name reused as variable name inside a schema
# ---------------------------------------------------------------------------

def test_type_name_reused_as_variable():
    """
    'Color' is declared as an enum type, but inside a schema body a variable
    can also be named 'Color'. The variable is local and Nat-typed; the type
    name is unaffected.
    """
    rt = EvidentRuntime()
    rt.load_source("""
type Color = Blue | Red | Green

claim some_thing
    Color ∈ Nat
    Color > 0
""")
    r = rt.query("some_thing")
    assert r.satisfied
    assert r.bindings["Color"] > 0


def test_type_name_variable_does_not_corrupt_enum():
    """After using 'Color' as a variable, the Color sort still works."""
    rt = EvidentRuntime()
    rt.load_source("""
type Color = Blue | Red | Green

claim uses_var
    Color ∈ Nat
    Color > 0

schema uses_enum
    c ∈ Color
    c ≠ Blue
""")
    rv = rt.query("uses_var")
    re = rt.query("uses_enum")

    assert rv.satisfied and rv.bindings["Color"] > 0
    assert re.satisfied and re.bindings["c"] in ("Red", "Green")


# ---------------------------------------------------------------------------
# 4. Enum variant names are global and must be unique
# ---------------------------------------------------------------------------

def test_duplicate_variant_across_enums_raises():
    """Registering the same variant name in two different enum types is an error."""
    reg = SortRegistry()
    reg.declare_algebraic("Color", ["Red", "Green", "Blue"])
    with pytest.raises(ValueError, match="Red"):
        reg.declare_algebraic("TrafficLight", ["Red", "Yellow"])


def test_duplicate_variant_error_names_both_types():
    """The error message identifies both the conflicting type and the new one."""
    reg = SortRegistry()
    reg.declare_algebraic("Color", ["Red", "Green", "Blue"])
    with pytest.raises(ValueError, match="Color"):
        reg.declare_algebraic("Palette", ["Red", "Purple"])


def test_disjoint_variant_names_allowed():
    """Two enums with fully disjoint variant names coexist without error."""
    reg = SortRegistry()
    reg.declare_algebraic("Color", ["Red", "Green", "Blue"])
    reg.declare_algebraic("TrafficLight", ["SignalRed", "Yellow", "SignalGreen"])
    assert reg.get("Color") is not None
    assert reg.get("TrafficLight") is not None


def test_duplicate_variant_in_source_raises():
    """Duplicate variant name caught when loading source."""
    rt = EvidentRuntime()
    with pytest.raises(ValueError, match="Red"):
        rt.load_source("""
type Color       = Red | Green | Blue
type TrafficLight = Red | Yellow
""")


def test_same_enum_declared_twice_is_idempotent():
    """Re-declaring the exact same enum type is a no-op (idempotent)."""
    reg = SortRegistry()
    s1 = reg.declare_algebraic("Color", ["Red", "Green", "Blue"])
    s2 = reg.declare_algebraic("Color", ["Red", "Green", "Blue"])
    assert s1 is s2


def test_variant_uniqueness_across_inline_and_named():
    """Inline enum variants conflict with top-level enum variants of the same name."""
    rt = EvidentRuntime()
    with pytest.raises(ValueError, match="Red"):
        rt.load_source("""
type Color = Red | Green | Blue

schema Thing
    x ∈ Red | Purple
""")


# ---------------------------------------------------------------------------
# 5. Enum variant lookup is unambiguous post-registration
# ---------------------------------------------------------------------------

def test_variant_resolves_to_correct_sort():
    """After registration, Red resolves to Color.Red and has Color sort."""
    import z3
    reg = SortRegistry()
    reg.declare_algebraic("Color", ["Red", "Green", "Blue"])
    red = reg.get_constructor("Red")
    color = reg.get("Color")
    assert red.sort().eq(color)


def test_unknown_variant_returns_none():
    reg = SortRegistry()
    reg.declare_algebraic("Color", ["Red", "Green", "Blue"])
    assert reg.get_constructor("Purple") is None
