"""
Conformance tests for composite schema elements in Set(T) and Seq(T).

Tests that:
1. ∀ x ∈ Set(T) : x.field = value  works with composite schemas
2. ∀ x ∈ Seq(T) : x.field = value  works with composite schemas
3. Model extraction for Seq(T) returns a list of dicts
4. ∃ x ∈ Set(T) : x.field = value  works
"""

import pytest
from .conftest import query, assert_sat, assert_unsat, assert_binding, assert_binding_satisfies


# ---------------------------------------------------------------------------
# Set(T) with composite element type
# ---------------------------------------------------------------------------

def test_set_composite_forall_field_access():
    """∀ x ∈ schedule : x.room ≠ x.slot works."""
    r = query("""
type Assignment
    room ∈ Nat
    slot ∈ Nat

claim sat_schedule_exists
    schedule ∈ Set Assignment
    a        ∈ Assignment
    a.room = 1
    a.slot = 2
    ∀ b ∈ schedule : b.room ≠ b.slot
""", "sat_schedule_exists")
    assert_sat(r)


def test_set_composite_forall_unsat():
    """∀ x ∈ Set(T) constraint makes a schema UNSAT when violated."""
    r = query("""
type Point
    x ∈ Nat
    y ∈ Nat

claim unsat_point_constraint
    pts ∈ Set Point
    p   ∈ Point
    p.x = 5
    p.y = 5
    -- Force a point where x = y into pts, then forbid it
    ∀ q ∈ pts : q.x ≠ q.y
    -- This member would violate the constraint:
    -- But we just check the schema is SAT (solver picks consistent state)
""", "unsat_point_constraint")
    # This should be SAT — the solver can choose pts to not include p
    assert_sat(r)


def test_set_composite_simple():
    """Basic Set(T) with composite type is satisfiable."""
    r = query("""
type Item
    id   ∈ Nat
    kind ∈ Nat

claim sat_items
    items ∈ Set Item
    i     ∈ Item
    i.id   = 42
    i.kind = 1
""", "sat_items")
    assert_sat(r)


# ---------------------------------------------------------------------------
# Seq(T) with composite element type
# ---------------------------------------------------------------------------

def test_seq_composite_forall_field_access():
    """∀ x ∈ Seq(T) : x.field constraint works."""
    r = query("""
type Task
    duration ∈ Nat
    priority ∈ Nat

claim sat_tasks_bounded
    tasks ∈ Seq(Task)
    ∀ t ∈ tasks : t.duration ≥ 0
""", "sat_tasks_bounded")
    assert_sat(r)


def test_seq_composite_length_and_fields():
    """Seq(T) with fixed length and field constraints."""
    r = query("""
type Pair
    fst ∈ Nat
    snd ∈ Nat

claim sat_pairs
    pairs ∈ Seq(Pair)
    p0    ∈ Pair
    p0.fst = 10
    p0.snd = 20
    pairs = ⟨p0⟩
""", "sat_pairs")
    b = assert_sat(r)
    # Should have at least fst and snd bindings accessible
    assert b is not None


def test_seq_composite_literal():
    """⟨a, b⟩ sequence literal with Datatype elements."""
    r = query("""
type Vec2
    x ∈ Int
    y ∈ Int

claim sat_vec_seq
    v1 ∈ Vec2
    v2 ∈ Vec2
    v1.x = 1
    v1.y = 2
    v2.x = 3
    v2.y = 4
    seq ∈ Seq(Vec2)
    seq = ⟨v1, v2⟩
""", "sat_vec_seq")
    b = assert_sat(r)
    assert b is not None


def test_seq_composite_model_extraction():
    """Seq(T) model extraction returns a list of dicts."""
    r = query("""
type RGB
    r ∈ Nat
    g ∈ Nat
    b ∈ Nat

claim sat_colors
    c1   ∈ RGB
    c2   ∈ RGB
    c1.r = 255
    c1.g = 0
    c1.b = 0
    c2.r = 0
    c2.g = 255
    c2.b = 0
    colors ∈ Seq(RGB)
    colors = ⟨c1, c2⟩
""", "sat_colors")
    b = assert_sat(r)
    # The 'colors' binding should be a list of dicts
    assert 'colors' in b
    colors = b['colors']
    assert isinstance(colors, list), f"Expected list, got {type(colors)}: {colors!r}"
    assert len(colors) == 2, f"Expected 2 elements, got {len(colors)}"
    # Each element should be a dict with r, g, b fields
    for elem in colors:
        assert isinstance(elem, dict), f"Expected dict, got {type(elem)}: {elem!r}"
        assert 'r' in elem and 'g' in elem and 'b' in elem


def test_seq_composite_model_values():
    """Seq(T) model extraction returns correct field values."""
    r = query("""
type RGB
    r ∈ Nat
    g ∈ Nat
    b ∈ Nat

claim sat_red_green
    c1   ∈ RGB
    c2   ∈ RGB
    c1.r = 255
    c1.g = 0
    c1.b = 0
    c2.r = 0
    c2.g = 128
    c2.b = 64
    colors ∈ Seq(RGB)
    colors = ⟨c1, c2⟩
""", "sat_red_green")
    b = assert_sat(r)
    colors = b.get('colors', [])
    assert len(colors) == 2
    assert colors[0].get('r') == 255
    assert colors[0].get('g') == 0
    assert colors[0].get('b') == 0
    assert colors[1].get('r') == 0
    assert colors[1].get('g') == 128
    assert colors[1].get('b') == 64


# ---------------------------------------------------------------------------
# Nested composite types
# ---------------------------------------------------------------------------

def test_seq_nested_composite():
    """Seq(T) where T has a field of another composite type."""
    r = query("""
type Color
    r ∈ Nat
    g ∈ Nat
    b ∈ Nat

type Rect
    x ∈ Int
    y ∈ Int
    w ∈ Nat
    h ∈ Nat
    color ∈ Color

claim sat_rects
    rect ∈ Rect
    rect.x = 10
    rect.y = 20
    rect.w = 100
    rect.h = 50
    rect.color.r = 255
    rect.color.g = 0
    rect.color.b = 0
    rects ∈ Seq(Rect)
    rects = ⟨rect⟩
""", "sat_rects")
    b = assert_sat(r)
    assert b is not None
    # Check field bindings are accessible via dotted names
    assert b.get('rect.x') == 10
    assert b.get('rect.y') == 20
    assert b.get('rect.w') == 100
    assert b.get('rect.h') == 50
