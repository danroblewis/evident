"""
Error case conformance tests.

Tests that Evident produces the correct behavior for invalid or
unsatisfiable inputs. An implementation must fail correctly.
"""

import pytest
from .conftest import query, assert_unsat


# ---------------------------------------------------------------------------
# Unsatisfiable constraints
# ---------------------------------------------------------------------------

def test_nat_cannot_be_negative():
    assert_unsat(query("schema S\n    x ∈ Nat\n    x = -1\n", "S"))

def test_contradictory_equality():
    assert_unsat(query("schema S\n    x ∈ Nat\n    x = 3\n    x = 4\n", "S"))

def test_contradictory_inequality():
    assert_unsat(query("schema S\n    x ∈ Nat\n    x > 5\n    x < 5\n    x = 5\n", "S"))

def test_empty_set_membership():
    assert_unsat(query("schema S\n    x ∈ Nat\n    x ∈ {}\n", "S"))

def test_contradictory_bool():
    assert_unsat(query("schema S\n    b ∈ Bool\n    b = true\n    b = false\n", "S"))

def test_string_constraint_contradiction():
    assert_unsat(query(
        'schema S\n    s ∈ String\n    s = "abc"\n    s = "xyz"\n', "S"
    ))

def test_seq_length_contradiction():
    assert_unsat(query(
        "schema S\n    s ∈ Seq(Nat)\n    #s = 3\n    #s = 5\n", "S"
    ))

def test_sub_schema_inherits_unsat():
    src = """
schema Inner
    x ∈ Nat
    x < 0

schema Outer
    i ∈ Inner
"""
    assert_unsat(query(src, "Outer"))

def test_passthrough_unsat():
    src = """
schema Base
    x ∈ Nat
    x > 10

schema Child
    ..Base
    x < 5
"""
    assert_unsat(query(src, "Child"))

def test_forall_forces_unsat():
    src = """
schema S
    x ∈ Nat
    x = 0
    ∀ v ∈ {1, 2, 3}: v < x
"""
    assert_unsat(query(src, "S"))

def test_exists_unique_unsat_no_match():
    src = """
schema S
    ∃! v ∈ {1, 2, 3}: v > 10
"""
    assert_unsat(query(src, "S"))

def test_exists_unique_unsat_multiple():
    src = """
schema S
    ∃! v ∈ {1, 2, 3}: v > 0
"""
    assert_unsat(query(src, "S"))

def test_enum_impossible_constraint():
    src = """
type Color = Red | Green | Blue

schema S
    c ∈ Color
    c = Red
    c = Green
"""
    assert_unsat(query(src, "S"))

def test_string_starts_with_unsat():
    src = """
schema S
    s ∈ String
    s = "world"
    s ⊑ "hello"
"""
    assert_unsat(query(src, "S"))

def test_regex_unsat():
    src = """
schema S
    s ∈ String
    s = "123"
    s ∈ /[a-z]+/
"""
    assert_unsat(query(src, "S"))

def test_given_causes_unsat():
    src = "schema S\n    x ∈ Nat\n    x < 5\n"
    assert_unsat(query(src, "S", given={'x': 10}))
