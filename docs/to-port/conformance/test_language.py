"""
Language conformance tests.

One test per language feature. All tests are black-box — they run the
evident CLI and check behavior. No imports from runtime.src or parser.src.

These tests specify what a correct Evident implementation must do,
regardless of how it is implemented (Python, Rust, etc.).
"""

import pytest
from .conftest import query, assert_sat, assert_unsat, assert_binding, assert_binding_satisfies


# ---------------------------------------------------------------------------
# 1. Basic membership constraints
# ---------------------------------------------------------------------------

def test_nat_membership():
    r = query("schema S\n    x ∈ Nat\n    x = 5\n", "S")
    b = assert_sat(r)
    assert_binding(b, 'x', 5)


def test_int_membership():
    r = query("schema S\n    x ∈ Int\n    x = -3\n", "S")
    b = assert_sat(r)
    assert_binding(b, 'x', -3)


def test_string_membership():
    r = query('schema S\n    s ∈ String\n    s = "hello"\n', "S")
    b = assert_sat(r)
    assert_binding(b, 's', 'hello')


def test_bool_membership():
    r = query("schema S\n    b ∈ Bool\n    b = true\n", "S")
    b = assert_sat(r)
    assert_binding(b, 'b', True)


def test_real_membership():
    r = query("schema S\n    x ∈ Real\n    x = 3.14\n", "S")
    b = assert_sat(r)
    assert_binding_satisfies(b, 'x', lambda v: abs(v - 3.14) < 0.01)


# ---------------------------------------------------------------------------
# 2. Arithmetic constraints
# ---------------------------------------------------------------------------

def test_equality():
    r = query("schema S\n    x ∈ Nat\n    x = 7\n", "S")
    assert_binding(assert_sat(r), 'x', 7)


def test_inequality():
    r = query("schema S\n    x ∈ Nat\n    x ≠ 0\n    x < 5\n", "S")
    b = assert_sat(r)
    assert_binding_satisfies(b, 'x', lambda v: v != 0 and v < 5)


def test_less_than():
    r = query("schema S\n    x ∈ Nat\n    x < 3\n", "S")
    assert_binding_satisfies(assert_sat(r), 'x', lambda v: v < 3)


def test_greater_than():
    r = query("schema S\n    x ∈ Nat\n    x > 10\n    x < 20\n", "S")
    assert_binding_satisfies(assert_sat(r), 'x', lambda v: 10 < v < 20)


def test_lte_gte():
    r = query("schema S\n    x ∈ Nat\n    x ≤ 5\n    x ≥ 5\n", "S")
    assert_binding(assert_sat(r), 'x', 5)


def test_arithmetic_add():
    r = query("schema S\n    x ∈ Nat\n    y ∈ Nat\n    x = 3\n    y = x + 2\n", "S")
    assert_binding(assert_sat(r), 'y', 5)


def test_arithmetic_unsat():
    r = query("schema S\n    x ∈ Nat\n    x < 0\n", "S")
    assert_unsat(r)


# ---------------------------------------------------------------------------
# 3. Chained comparisons
# ---------------------------------------------------------------------------

def test_chained_comparison():
    r = query("schema S\n    x ∈ Nat\n    0 < x < 5\n    x = 3\n", "S")
    assert_binding(assert_sat(r), 'x', 3)


def test_chained_comparison_unsat():
    r = query("schema S\n    x ∈ Nat\n    5 < x < 3\n", "S")
    assert_unsat(r)


# ---------------------------------------------------------------------------
# 4. Logic operators
# ---------------------------------------------------------------------------

def test_and():
    r = query("schema S\n    x ∈ Nat\n    x > 0 ∧ x < 10\n    x = 5\n", "S")
    assert_binding(assert_sat(r), 'x', 5)


def test_or():
    r = query("schema S\n    x ∈ Nat\n    x = 1 ∨ x = 2\n", "S")
    assert_binding_satisfies(assert_sat(r), 'x', lambda v: v in (1, 2))


def test_not():
    r = query("schema S\n    b ∈ Bool\n    b = false\n    ¬b = true\n", "S")
    assert_sat(r)


def test_implies():
    r = query("schema S\n    x ∈ Nat\n    x = 5\n    x > 3 ⇒ x < 10\n", "S")
    assert_binding(assert_sat(r), 'x', 5)


def test_implies_vacuous():
    r = query("schema S\n    x ∈ Nat\n    x = 0\n    x > 3 ⇒ x < 1\n", "S")
    assert_sat(r)


def test_implies_forces_consequent():
    r = query("schema S\n    x ∈ Nat\n    x = 5\n    x > 3 ⇒ x < 4\n", "S")
    assert_unsat(r)


# ---------------------------------------------------------------------------
# 5. Implies block (indented multi-consequent)
# ---------------------------------------------------------------------------

def test_implies_block():
    src = """
schema S
    x ∈ Nat
    y ∈ Nat
    x = 5
    x > 3 ⇒
        y = 10
        x < 100
"""
    assert_binding(assert_sat(query(src, "S")), 'y', 10)


def test_nested_implies_block():
    src = """
schema S
    x ∈ Nat
    y ∈ Nat
    x = 5
    x > 3 ⇒
        (x < 10) ⇒
            y = 99
"""
    assert_binding(assert_sat(query(src, "S")), 'y', 99)


# ---------------------------------------------------------------------------
# 6. Quantifiers
# ---------------------------------------------------------------------------

def test_forall_range():
    src = """
schema S
    x ∈ Nat
    x ∈ {1..5}
    ∀ i ∈ {1..5}: i ≥ 1
"""
    assert_sat(query(src, "S"))


def test_forall_set_sat():
    src = """
schema S
    x ∈ Nat
    ∀ v ∈ {1, 2, 3}: v > 0
"""
    assert_sat(query(src, "S"))


def test_forall_unsat():
    src = """
schema S
    x ∈ Nat
    x = 0
    ∀ v ∈ {1, 2, 3}: v < x
"""
    assert_unsat(query(src, "S"))


def test_exists():
    src = """
schema S
    ∃ x ∈ {1..10}: x > 5
"""
    assert_sat(query(src, "S"))


def test_exists_unsat():
    src = """
schema S
    ∃ x ∈ {1..10}: x > 20
"""
    assert_unsat(query(src, "S"))


def test_exists_unique():
    src = """
schema S
    ∃! x ∈ {1, 2, 3}: x = 2
"""
    assert_sat(query(src, "S"))


def test_none_quantifier():
    src = """
schema S
    ¬∃ x ∈ {1, 2, 3}: x > 10
"""
    assert_sat(query(src, "S"))


# ---------------------------------------------------------------------------
# 7. Set literals, ranges, comprehensions
# ---------------------------------------------------------------------------

def test_set_literal_membership():
    src = "schema S\n    x ∈ Nat\n    x ∈ {2, 4, 6}\n"
    assert_binding_satisfies(assert_sat(query(src, "S")), 'x', lambda v: v in (2, 4, 6))


def test_set_not_member():
    src = "schema S\n    x ∈ Nat\n    x ∉ {1, 2, 3}\n    x < 5\n"
    assert_binding_satisfies(assert_sat(query(src, "S")), 'x', lambda v: v not in (1, 2, 3))


def test_range_membership():
    src = "schema S\n    x ∈ Nat\n    x ∈ {5..10}\n"
    assert_binding_satisfies(assert_sat(query(src, "S")), 'x', lambda v: 5 <= v <= 10)


def test_set_comprehension():
    src = """
schema S
    evens = {x | x ∈ {1..10}, x ∈ {2, 4, 6, 8, 10}}
    4 ∈ evens
"""
    assert_sat(query(src, "S"))


def test_subset():
    src = "schema S\n    A ⊆ B\n    A = {1, 2}\n    B = {1, 2, 3}\n"
    assert_sat(query(src, "S"))


def test_subset_unsat():
    src = "schema S\n    A ⊆ B\n    A = {1, 2, 3}\n    B = {1, 2}\n"
    assert_unsat(query(src, "S"))


# ---------------------------------------------------------------------------
# 8. Tuple membership
# ---------------------------------------------------------------------------

def test_tuple_membership():
    src = """
assert pairs = {(1, "a"), (2, "b"), (3, "c")}

schema S
    n ∈ Nat
    s ∈ String
    (n, s) ∈ pairs
    n = 2
"""
    assert_binding(assert_sat(query(src, "S")), 's', 'b')


# ---------------------------------------------------------------------------
# 9. Enum types
# ---------------------------------------------------------------------------

def test_enum_declaration():
    src = """
type Color = Red | Green | Blue

schema S
    c ∈ Color
    c = Red
"""
    assert_binding(assert_sat(query(src, "S")), 'c', 'Red')


def test_enum_constraint():
    src = """
type Dir = North | South | East | West

schema S
    d ∈ Dir
    d ≠ North
    d ≠ South
    d ≠ West
"""
    assert_binding(assert_sat(query(src, "S")), 'd', 'East')


def test_inline_enum():
    src = """
schema S
    c ∈ Red | Green | Blue
    c = Green
"""
    assert_binding(assert_sat(query(src, "S")), 'c', 'Green')


# ---------------------------------------------------------------------------
# 10. Sub-schema expansion
# ---------------------------------------------------------------------------

def test_sub_schema_expansion():
    src = """
schema Point
    x ∈ Int
    y ∈ Int

schema S
    p ∈ Point
    p.x = 3
    p.y = 4
"""
    b = assert_sat(query(src, "S"))
    assert_binding(b, 'p.x', 3)
    assert_binding(b, 'p.y', 4)


def test_sub_schema_constraint():
    src = """
schema Interval
    lo ∈ Int
    hi ∈ Int
    lo < hi

schema S
    i ∈ Interval
    i.lo = 0
    i.hi = 10
"""
    assert_sat(query(src, "S"))


def test_sub_schema_unsat():
    src = """
schema Interval
    lo ∈ Int
    hi ∈ Int
    lo < hi

schema S
    i ∈ Interval
    i.lo = 10
    i.hi = 0
"""
    assert_unsat(query(src, "S"))


# ---------------------------------------------------------------------------
# 11. Passthrough ..SubSchema
# ---------------------------------------------------------------------------

def test_passthrough():
    src = """
schema Base
    x ∈ Nat
    x > 0

schema Derived
    ..Base
    y ∈ Nat
    y = x + 1
"""
    b = assert_sat(query(src, "Derived"))
    assert_binding_satisfies(b, 'x', lambda v: v > 0)
    assert_binding_satisfies(b, 'y', lambda v: v > 1)


def test_passthrough_inherits_constraints():
    src = """
schema Base
    x ∈ Nat
    x > 5

schema Derived
    ..Base
    x < 3
"""
    assert_unsat(query(src, "Derived"))


# ---------------------------------------------------------------------------
# 12. Multi-name declarations
# ---------------------------------------------------------------------------

def test_multi_name():
    src = """
schema S
    x, y, z ∈ Nat
    x = 1
    y = 2
    z = 3
"""
    b = assert_sat(query(src, "S"))
    assert_binding(b, 'x', 1)
    assert_binding(b, 'y', 2)
    assert_binding(b, 'z', 3)


# ---------------------------------------------------------------------------
# 13. Assert statements (named sets)
# ---------------------------------------------------------------------------

def test_assert_named_set():
    src = """
assert primes = {2, 3, 5, 7, 11}

schema S
    p ∈ Nat
    p ∈ primes
    p > 4
"""
    assert_binding_satisfies(assert_sat(query(src, "S")), 'p', lambda v: v in (5, 7, 11))


def test_assert_named_relation():
    src = """
assert edges = {(1, 2), (2, 3), (3, 4)}

schema S
    a ∈ Nat
    b ∈ Nat
    (a, b) ∈ edges
    a = 2
"""
    assert_binding(assert_sat(query(src, "S")), 'b', 3)


# ---------------------------------------------------------------------------
# 14. String operations
# ---------------------------------------------------------------------------

def test_string_concat():
    src = """
schema S
    s ∈ String
    s = "hello" ++ " " ++ "world"
"""
    assert_binding(assert_sat(query(src, "S")), 's', 'hello world')


def test_string_contains():
    src = """
schema S
    s ∈ String
    s = "hello world"
    s ∋ "world"
"""
    assert_sat(query(src, "S"))


def test_string_contains_unsat():
    src = """
schema S
    s ∈ String
    s = "hello"
    s ∋ "world"
"""
    assert_unsat(query(src, "S"))


def test_string_starts_with():
    src = """
schema S
    s ∈ String
    s = "hello world"
    s ⊑ "hello"
"""
    assert_sat(query(src, "S"))


def test_string_ends_with():
    src = """
schema S
    s ∈ String
    s = "hello world"
    s ⊒ "world"
"""
    assert_sat(query(src, "S"))


def test_string_length():
    src = """
schema S
    s ∈ String
    s = "abc"
    #s = 3
"""
    assert_sat(query(src, "S"))


def test_string_length_constraint():
    src = """
schema S
    s ∈ String
    s = "hello"
    #s > 3
"""
    assert_sat(query(src, "S"))


def test_int_to_str():
    src = """
schema S
    n ∈ Nat
    s ∈ String
    n = 42
    s = int_to_str n
"""
    assert_binding(assert_sat(query(src, "S")), 's', '42')


def test_int_to_str_reverse():
    src = """
schema S
    n ∈ Nat
    s ∈ String
    s = "42"
    s = int_to_str n
"""
    assert_binding(assert_sat(query(src, "S")), 'n', 42)


# ---------------------------------------------------------------------------
# 15. Regex membership
# ---------------------------------------------------------------------------

def test_regex_membership():
    src = """
schema S
    s ∈ /[a-z]+/
"""
    b = assert_sat(query(src, "S"))
    assert_binding_satisfies(b, 's', lambda v: isinstance(v, str) and v.islower())


def test_regex_membership_unsat():
    src = """
schema S
    s ∈ String
    s = "HELLO"
    s ∈ /[a-z]+/
"""
    assert_unsat(query(src, "S"))


# ---------------------------------------------------------------------------
# 16. Sequence types
# ---------------------------------------------------------------------------

def test_seq_type():
    src = """
schema S
    s ∈ Seq(Nat)
    #s = 3
    s[0] = 1
    s[1] = 2
    s[2] = 3
"""
    b = assert_sat(query(src, "S"))
    assert_binding(b, 's.0', 1)
    assert_binding(b, 's.1', 2)
    assert_binding(b, 's.2', 3)


def test_seq_element_membership():
    src = """
schema S
    s ∈ Seq(Nat)
    #s = 3
    5 ∈ s
"""
    b = assert_sat(query(src, "S"))
    elements = [b.get(f's.{i}') for i in range(3)]
    assert 5 in elements


def test_seq_concat():
    src = """
schema S
    a ∈ Seq(Nat)
    b ∈ Seq(Nat)
    c ∈ Seq(Nat)
    a = ⟨1, 2⟩
    b = ⟨3, 4⟩
    c = a ++ b
    #c = 4
"""
    b = assert_sat(query(src, "S"))
    assert_binding(b, 'c.0', 1)
    assert_binding(b, 'c.3', 4)


def test_seq_literal():
    src = """
schema S
    s ∈ Seq(Nat)
    s = ⟨10, 20, 30⟩
"""
    b = assert_sat(query(src, "S"))
    assert_binding(b, 's.0', 10)
    assert_binding(b, 's.1', 20)
    assert_binding(b, 's.2', 30)


# ---------------------------------------------------------------------------
# 17. Notation declarations
# ---------------------------------------------------------------------------

def test_notation_basic():
    src = """
notation double x = x + x

schema S
    n ∈ Nat
    n = 5
    m ∈ Nat
    m = double n
"""
    assert_binding(assert_sat(query(src, "S")), 'm', 10)


def test_notation_adjacent():
    src = """
notation adjacent seq = {(seq[i], seq[i+1]) | i ∈ {0..#seq-2}}

schema S
    s ∈ Seq(Nat)
    s = ⟨1, 2, 3⟩
    ∀ (a, b) ∈ adjacent s: b = a + 1
"""
    assert_sat(query(src, "S"))


def test_notation_adjacent_unsat():
    src = """
notation adjacent seq = {(seq[i], seq[i+1]) | i ∈ {0..#seq-2}}

schema S
    s ∈ Seq(Nat)
    s = ⟨1, 3, 2⟩
    ∀ (a, b) ∈ adjacent s: b = a + 1
"""
    assert_unsat(query(src, "S"))


# ---------------------------------------------------------------------------
# 18. Bool-as-constraint (bare Bool variables in implies position)
# ---------------------------------------------------------------------------

def test_bool_as_constraint():
    src = """
schema S
    flag ∈ Bool
    x ∈ Nat
    flag = true
    flag ⇒ x = 42
"""
    assert_binding(assert_sat(query(src, "S")), 'x', 42)


def test_not_bool_as_constraint():
    src = """
schema S
    flag ∈ Bool
    x ∈ Nat
    flag = false
    ¬flag ⇒ x = 99
"""
    assert_binding(assert_sat(query(src, "S")), 'x', 99)


# ---------------------------------------------------------------------------
# 19. Schema params syntax
# ---------------------------------------------------------------------------

def test_schema_params():
    src = """
schema S(n ∈ Nat, m ∈ Nat)
    n < m
"""
    r = query(src, "S", given={'n': 3, 'm': 7})
    assert_sat(r)


def test_schema_params_unsat():
    src = """
schema S(n ∈ Nat, m ∈ Nat)
    n < m
"""
    r = query(src, "S", given={'n': 7, 'm': 3})
    assert_unsat(r)


# ---------------------------------------------------------------------------
# 20. Forward rules
# ---------------------------------------------------------------------------

def test_forward_rule():
    src = """
assert even = {0, 2, 4, 6, 8}
assert odd  = {1, 3, 5, 7, 9}

schema S
    x ∈ Nat
    x ∈ even
    x = 4
"""
    assert_binding(assert_sat(query(src, "S")), 'x', 4)


# ---------------------------------------------------------------------------
# 21. Cardinality expressions
# ---------------------------------------------------------------------------

def test_string_cardinality():
    src = """
schema S
    s ∈ String
    s = "hello"
    #s = 5
"""
    assert_sat(query(src, "S"))


def test_seq_cardinality():
    src = """
schema S
    s ∈ Seq(Nat)
    s = ⟨1, 2, 3, 4⟩
    #s = 4
"""
    assert_sat(query(src, "S"))


# ---------------------------------------------------------------------------
# 22. Given values (bidirectional solving)
# ---------------------------------------------------------------------------

def test_given_pins_value():
    src = """
schema S
    x ∈ Nat
    y ∈ Nat
    x + y = 10
"""
    b = assert_sat(query(src, "S", given={'x': 3}))
    assert_binding(b, 'x', 3)
    assert_binding(b, 'y', 7)


def test_given_causes_unsat():
    src = """
schema S
    x ∈ Nat
    x < 5
"""
    r = query(src, "S", given={'x': 10})
    assert_unsat(r)


# ---------------------------------------------------------------------------
# 23. Enum with constraint
# ---------------------------------------------------------------------------

def test_enum_in_tuple():
    src = """
type Status = Active | Inactive | Pending

assert status_map = {(Active, "active"), (Inactive, "inactive"), (Pending, "pending")}

schema S
    s ∈ Status
    label ∈ String
    (s, label) ∈ status_map
    s = Active
"""
    assert_binding(assert_sat(query(src, "S")), 'label', 'active')
