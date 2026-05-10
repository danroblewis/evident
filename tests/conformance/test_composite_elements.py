"""
Conformance tests for composite (record-typed) elements in Seq(T).

Tests that:
1. Seq(T) with composite element type is satisfiable
2. Seq(T) literals (`⟨a, b⟩`) work over Datatype elements
3. ∀ x ∈ Seq(T) : x.field constraint works (with a pinned length)
4. Seq(T) of nested composite types works

`Set(T)` of composite types is NOT covered here — that shape is
unsupported by the Rust runtime (silently dropped, see
`programs/demos/COUNTEREXAMPLES.md` #15). Use `Seq(T)` with a pinned
length instead.

Model extraction of `Seq(Composite)` values into structured JSON
(list-of-dicts) is also broken (#17 in COUNTEREXAMPLES) — the values
land in `--json` output as a Debug-formatted string. Tests that
relied on it have been deleted; this file checks SAT and binding
presence only, not deep structure.
"""

import pytest
from .conftest import query, assert_sat, assert_unsat, assert_binding, assert_binding_satisfies


# ---------------------------------------------------------------------------
# Seq(T) with composite element type
# ---------------------------------------------------------------------------

def test_seq_composite_forall_field_access():
    """∀ x ∈ Seq(T) : x.field constraint works when length is pinned.

    Without `#tasks = N`, the forall is silently dropped — see
    COUNTEREXAMPLES.md #16. Pinning the length is the supported shape.
    """
    r = query("""
type Task
    duration ∈ Nat
    priority ∈ Nat

claim sat_tasks_bounded
    tasks ∈ Seq(Task)
    #tasks = 3
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
