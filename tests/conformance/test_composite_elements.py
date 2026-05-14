"""
Conformance tests for composite (record-typed) elements in Seq(T)
and Set(T).

Tests that:
1. Seq(T) with composite element type is satisfiable
2. Seq(T) literals (`⟨a, b⟩`) work over Datatype elements
3. ∀ x ∈ Seq(T) : x.field constraint works (with a pinned length)
4. Seq(T) of nested composite types works
5. Set(T) with composite element type supports literal-set assignment,
   membership, subset (via `∀ x ∈ a : x ∈ b`), and cardinality

Model extraction of `Seq(Composite)` values into structured JSON
(list-of-dicts) is also broken (#17 in COUNTEREXAMPLES) — the values
land in `--json` output as a Debug-formatted string. Tests that
relied on it have been deleted; this file checks SAT and binding
presence only, not deep structure. Set(Composite) extraction is
similarly not exercised here (v1 limit).
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


# ---------------------------------------------------------------------------
# Set(T) with composite element type
# ---------------------------------------------------------------------------

def test_set_composite_membership():
    """Set(T) with composite elements supports `x ∈ s` membership."""
    r = query("""
type Item(id, kind ∈ Nat)

claim sat_set_membership
    a ∈ Item (id ↦ 1, kind ↦ 0)
    b ∈ Item (id ↦ 2, kind ↦ 1)
    items ∈ Set(Item)
    items = {a, b}
    a ∈ items
    b ∈ items
""", "sat_set_membership")
    assert_sat(r)


def test_set_composite_subset_via_forall():
    """`∀ x ∈ a : x ∈ b` over composite-element Sets translates to Z3
    native set_subset and SATs when a is actually a subset of b."""
    r = query("""
type Item(id, kind ∈ Nat)

claim sat_set_subset
    a ∈ Item (id ↦ 1, kind ↦ 0)
    b ∈ Item (id ↦ 2, kind ↦ 1)
    c ∈ Item (id ↦ 3, kind ↦ 2)
    big ∈ Set(Item)
    small ∈ Set(Item)
    big = {a, b, c}
    small = {a, c}
    ∀ x ∈ small : x ∈ big
""", "sat_set_subset")
    assert_sat(r)


def test_set_composite_cardinality():
    """`#s` returns the pinned-literal element count for composite Sets."""
    r = query("""
type Item(id, kind ∈ Nat)

claim sat_set_cardinality
    a ∈ Item (id ↦ 1, kind ↦ 0)
    b ∈ Item (id ↦ 2, kind ↦ 1)
    items ∈ Set(Item)
    items = {a, b}
    #items = 2
""", "sat_set_cardinality")
    assert_sat(r)


def test_set_composite_subset_unsat_when_member_missing():
    """If `small` contains an element not in `big`, subset must UNSAT."""
    r = query("""
type Item(id, kind ∈ Nat)

claim unsat_set_subset_missing
    a ∈ Item (id ↦ 1, kind ↦ 0)
    b ∈ Item (id ↦ 2, kind ↦ 1)
    c ∈ Item (id ↦ 3, kind ↦ 2)
    big ∈ Set(Item)
    small ∈ Set(Item)
    big = {a, b}
    small = {a, c}
    ∀ x ∈ small : x ∈ big
""", "unsat_set_subset_missing")
    assert_unsat(r)


# ---------------------------------------------------------------------------
# Seq fields inside composite types (tree-of-sequences) — was COUNTEREXAMPLES #25
# ---------------------------------------------------------------------------
#
# A composite type whose field type is `Seq(T)` now compiles to a Datatype
# with two accessors per Seq field (Array and Int-length). Lets a type
# declare an internal Seq, and `Seq(Composite)` over that type indirectly
# yields the Seq-of-Seq shape needed for the Mario render refactor.

def test_seq_field_in_type_indexing():
    """A `Seq(T)` field inside a composite is addressable as
    `instance.field[i]` and pinned via the type body's invariants."""
    r = query("""
type Sprite(pos ∈ Int)
    rects ∈ Seq(Int)
    #rects = 3
    rects[0] = pos
    rects[1] = pos + 1
    rects[2] = pos + 2

claim sat_field_index
    s ∈ Sprite (pos mapsto 10)
    s.rects[0] = 10
    s.rects[1] = 11
    s.rects[2] = 12
""", "sat_field_index")
    assert_sat(r)


def test_seq_field_unsat_when_body_constraint_violated():
    """Type-body constraint `rects[1] = pos + 1` should reject any
    inconsistent pin from the caller."""
    r = query("""
type Sprite(pos ∈ Int)
    rects ∈ Seq(Int)
    #rects = 3
    rects[0] = pos
    rects[1] = pos + 1
    rects[2] = pos + 2

claim unsat_field_value
    s ∈ Sprite (pos mapsto 10)
    s.rects[1] = 99
""", "unsat_field_value")
    assert_unsat(r)


def test_seq_field_cardinality_propagation():
    """`#instance.field` reads the inherited length pin."""
    r = query("""
type Sprite(p ∈ Int)
    items ∈ Seq(Int)
    #items = 5

claim sat_card
    s ∈ Sprite (p mapsto 0)
    #s.items = 5
""", "sat_card")
    assert_sat(r)


def test_seq_field_forall_iteration():
    """`∀ x ∈ instance.field : …` unrolls over the field's pinned length."""
    r = query("""
type Sprite(base ∈ Int)
    nums ∈ Seq(Int)
    #nums = 3

claim sat_forall_through_field
    s ∈ Sprite (base mapsto 10)
    s.nums[0] = 5
    s.nums[1] = 7
    s.nums[2] = 11
    ∀ x ∈ s.nums : x ≥ 0
""", "sat_forall_through_field")
    assert_sat(r)


def test_seq_of_composite_with_seq_field():
    """`Seq(Composite)` where Composite has a `Seq(T)` field — the
    full tree-of-sequences shape, reachable as `outer[i].field[j]`."""
    r = query("""
type Group
    items ∈ Seq(Int)
    #items = 2

claim sat_nested_access
    groups ∈ Seq(Group)
    #groups = 3
    groups[0].items[0] = 10
    groups[0].items[1] = 20
    groups[2].items[1] = 60
""", "sat_nested_access")
    assert_sat(r)


def test_seq_of_composite_with_seq_field_unsat():
    """Outer Seq element addressable + per-element pinning."""
    r = query("""
type Group
    items ∈ Seq(Int)
    #items = 2

claim unsat_nested_inconsistent
    groups ∈ Seq(Group)
    #groups = 2
    groups[0].items[0] = 10
    groups[0].items[0] = 99
""", "unsat_nested_inconsistent")
    assert_unsat(r)
