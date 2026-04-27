"""
Phase 4 tests: Set operations encoded as Z3 Array theory.

All tests use a Z3 Solver to verify correctness — we never just inspect
Z3 expressions structurally, we always ask the solver whether a property
holds (sat) or doesn't (unsat).
"""

import pytest
from z3 import (
    And,
    BoolSort,
    BoolVal,
    Exists,
    FreshConst,
    Int,
    IntSort,
    IntVal,
    Lambda,
    Not,
    Solver,
    Select,
    Store,
    TupleSort,
    sat,
    unsat,
)

from runtime.src.sets import (
    empty_set,
    set_cartesian_product,
    set_complement,
    set_difference,
    set_from_list,
    set_image,
    set_intersection,
    set_member,
    set_not_member,
    set_subset,
    set_union,
    singleton,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _is_member(val, s):
    """Return True if val is provably a member of s (solver confirms sat)."""
    solver = Solver()
    solver.add(Select(s, val))
    return solver.check() == sat


def _not_member(val, s):
    """Return True if val is provably NOT a member of s (solver confirms unsat)."""
    solver = Solver()
    solver.add(Select(s, val))
    return solver.check() == unsat


# ---------------------------------------------------------------------------
# empty_set
# ---------------------------------------------------------------------------

class TestEmptySet:
    def test_no_element_is_a_member(self):
        """∀ n : ¬(n ∈ ∅) — no integer is in the empty set."""
        s = empty_set(IntSort())
        x = Int("x")
        solver = Solver()
        # Assert that some integer IS a member — should be unsat.
        solver.add(Select(s, x))
        assert solver.check() == unsat

    def test_specific_values_not_members(self):
        s = empty_set(IntSort())
        for v in [IntVal(0), IntVal(1), IntVal(-1), IntVal(100)]:
            assert _not_member(v, s), f"{v} should not be in the empty set"


# ---------------------------------------------------------------------------
# singleton
# ---------------------------------------------------------------------------

class TestSingleton:
    def test_element_is_member(self):
        s = singleton(IntVal(5), IntSort())
        assert _is_member(IntVal(5), s), "5 should be in singleton {5}"

    def test_other_elements_not_members(self):
        s = singleton(IntVal(5), IntSort())
        for v in [IntVal(0), IntVal(4), IntVal(6), IntVal(-1)]:
            assert _not_member(v, s), f"{v} should not be in singleton {{5}}"

    def test_only_member_is_the_element(self):
        """The solver can find no x ≠ 5 that is a member."""
        s = singleton(IntVal(5), IntSort())
        x = Int("x")
        solver = Solver()
        solver.add(Select(s, x))
        solver.add(x != IntVal(5))
        assert solver.check() == unsat


# ---------------------------------------------------------------------------
# set_from_list
# ---------------------------------------------------------------------------

class TestSetFromList:
    def test_members_present(self):
        s = set_from_list([IntVal(1), IntVal(2), IntVal(3)], IntSort())
        for v in [IntVal(1), IntVal(2), IntVal(3)]:
            assert _is_member(v, s), f"{v} should be in {{1,2,3}}"

    def test_non_member_absent(self):
        s = set_from_list([IntVal(1), IntVal(2), IntVal(3)], IntSort())
        assert _not_member(IntVal(4), s), "4 should not be in {1,2,3}"

    def test_empty_list_gives_empty_set(self):
        s = set_from_list([], IntSort())
        x = Int("x")
        solver = Solver()
        solver.add(Select(s, x))
        assert solver.check() == unsat


# ---------------------------------------------------------------------------
# set_union
# ---------------------------------------------------------------------------

class TestSetUnion:
    def test_members_of_either_in_union(self):
        s = set_from_list([IntVal(1), IntVal(2)], IntSort())
        t = set_from_list([IntVal(3), IntVal(4)], IntSort())
        u = set_union(s, t, IntSort())
        for v in [IntVal(1), IntVal(2), IntVal(3), IntVal(4)]:
            assert _is_member(v, u), f"{v} should be in union"

    def test_non_member_absent_from_union(self):
        s = set_from_list([IntVal(1), IntVal(2)], IntSort())
        t = set_from_list([IntVal(3), IntVal(4)], IntSort())
        u = set_union(s, t, IntSort())
        assert _not_member(IntVal(5), u), "5 should not be in union of {1,2} and {3,4}"

    def test_overlapping_union(self):
        s = set_from_list([IntVal(1), IntVal(2)], IntSort())
        t = set_from_list([IntVal(2), IntVal(3)], IntSort())
        u = set_union(s, t, IntSort())
        for v in [IntVal(1), IntVal(2), IntVal(3)]:
            assert _is_member(v, u)
        assert _not_member(IntVal(4), u)


# ---------------------------------------------------------------------------
# set_intersection
# ---------------------------------------------------------------------------

class TestSetIntersection:
    def test_common_members_in_intersection(self):
        s = set_from_list([IntVal(1), IntVal(2), IntVal(3)], IntSort())
        t = set_from_list([IntVal(2), IntVal(3), IntVal(4)], IntSort())
        i = set_intersection(s, t, IntSort())
        for v in [IntVal(2), IntVal(3)]:
            assert _is_member(v, i), f"{v} should be in intersection"

    def test_non_common_absent_from_intersection(self):
        s = set_from_list([IntVal(1), IntVal(2), IntVal(3)], IntSort())
        t = set_from_list([IntVal(2), IntVal(3), IntVal(4)], IntSort())
        i = set_intersection(s, t, IntSort())
        assert _not_member(IntVal(1), i), "1 only in s — not in intersection"
        assert _not_member(IntVal(4), i), "4 only in t — not in intersection"

    def test_disjoint_sets_empty_intersection(self):
        s = set_from_list([IntVal(1), IntVal(2)], IntSort())
        t = set_from_list([IntVal(3), IntVal(4)], IntSort())
        i = set_intersection(s, t, IntSort())
        x = Int("x")
        solver = Solver()
        solver.add(Select(i, x))
        assert solver.check() == unsat, "Intersection of disjoint sets must be empty"


# ---------------------------------------------------------------------------
# set_difference
# ---------------------------------------------------------------------------

class TestSetDifference:
    def test_difference_correct(self):
        """{1,2,3} \\ {2,3} = {1}"""
        s = set_from_list([IntVal(1), IntVal(2), IntVal(3)], IntSort())
        t = set_from_list([IntVal(2), IntVal(3)], IntSort())
        d = set_difference(s, t, IntSort())
        assert _is_member(IntVal(1), d), "1 should be in {1,2,3} \\ {2,3}"
        assert _not_member(IntVal(2), d), "2 should not be in difference"
        assert _not_member(IntVal(3), d), "3 should not be in difference"
        assert _not_member(IntVal(4), d), "4 should not be in difference"

    def test_difference_with_empty_subtrahend(self):
        """{1,2} \\ {} = {1,2}"""
        s = set_from_list([IntVal(1), IntVal(2)], IntSort())
        t = empty_set(IntSort())
        d = set_difference(s, t, IntSort())
        assert _is_member(IntVal(1), d)
        assert _is_member(IntVal(2), d)

    def test_difference_gives_empty_when_subset_removed(self):
        """{1,2} \\ {1,2} = {}"""
        s = set_from_list([IntVal(1), IntVal(2)], IntSort())
        d = set_difference(s, s, IntSort())
        x = Int("x")
        solver = Solver()
        solver.add(Select(d, x))
        assert solver.check() == unsat


# ---------------------------------------------------------------------------
# set_member / set_not_member
# ---------------------------------------------------------------------------

class TestMembership:
    def test_set_member_true(self):
        s = set_from_list([IntVal(1), IntVal(2)], IntSort())
        solver = Solver()
        solver.add(set_member(IntVal(1), s))
        assert solver.check() == sat

    def test_set_member_false(self):
        s = set_from_list([IntVal(1), IntVal(2)], IntSort())
        solver = Solver()
        solver.add(set_member(IntVal(99), s))
        assert solver.check() == unsat

    def test_set_not_member_true(self):
        s = set_from_list([IntVal(1), IntVal(2)], IntSort())
        solver = Solver()
        solver.add(set_not_member(IntVal(99), s))
        assert solver.check() == sat

    def test_set_not_member_false(self):
        s = set_from_list([IntVal(1), IntVal(2)], IntSort())
        solver = Solver()
        solver.add(set_not_member(IntVal(1), s))
        assert solver.check() == unsat


# ---------------------------------------------------------------------------
# set_subset
# ---------------------------------------------------------------------------

class TestSetSubset:
    def test_subset_holds(self):
        """{1,2} ⊆ {1,2,3} should be satisfiable (the subset property holds)."""
        s = set_from_list([IntVal(1), IntVal(2)], IntSort())
        t = set_from_list([IntVal(1), IntVal(2), IntVal(3)], IntSort())
        # set_subset returns a Z3 BoolRef (a ForAll). We add it and check sat.
        solver = Solver()
        solver.add(set_subset(s, t, IntSort()))
        assert solver.check() == sat

    def test_subset_does_not_hold(self):
        """{1,4} ⊆ {1,2,3}: the solver must find a counterexample — unsat when
        we assert BOTH the subset claim AND ask for a counterexample witness."""
        s = set_from_list([IntVal(1), IntVal(4)], IntSort())
        t = set_from_list([IntVal(1), IntVal(2), IntVal(3)], IntSort())
        # To check that the subset does NOT hold, we negate it:
        # ∃x. S[x] ∧ ¬T[x]. We just assert the witness directly.
        x = Int("x")
        solver = Solver()
        # A witness that violates the subset: x ∈ s and x ∉ t.
        solver.add(Select(s, x))
        solver.add(Not(Select(t, x)))
        assert solver.check() == sat  # 4 is a witness

    def test_empty_subset_of_everything(self):
        """∅ ⊆ S for any S."""
        s = empty_set(IntSort())
        t = set_from_list([IntVal(1), IntVal(2)], IntSort())
        solver = Solver()
        solver.add(set_subset(s, t, IntSort()))
        assert solver.check() == sat

    def test_set_subset_of_itself(self):
        """S ⊆ S always holds."""
        s = set_from_list([IntVal(1), IntVal(2)], IntSort())
        solver = Solver()
        solver.add(set_subset(s, s, IntSort()))
        assert solver.check() == sat

    def test_subset_property_is_refutable(self):
        """The negation of {1,4} ⊆ {1,2,3} is provable by counterexample."""
        s = set_from_list([IntVal(1), IntVal(4)], IntSort())
        t = set_from_list([IntVal(1), IntVal(2), IntVal(3)], IntSort())
        # subset holds iff no counterexample exists. Here 4 is a counterexample.
        solver = Solver()
        solver.add(Not(set_subset(s, t, IntSort())))
        assert solver.check() == sat  # negation is satisfiable (subset does NOT hold)


# ---------------------------------------------------------------------------
# set_complement
# ---------------------------------------------------------------------------

class TestSetComplement:
    def test_complement_excludes_original_members(self):
        s = set_from_list([IntVal(1), IntVal(2)], IntSort())
        c = set_complement(s, IntSort())
        # Elements in s should NOT be in complement
        assert _not_member(IntVal(1), c)
        assert _not_member(IntVal(2), c)

    def test_complement_includes_non_members(self):
        s = set_from_list([IntVal(1), IntVal(2)], IntSort())
        c = set_complement(s, IntSort())
        assert _is_member(IntVal(99), c)
        assert _is_member(IntVal(-5), c)


# ---------------------------------------------------------------------------
# set_cartesian_product
# ---------------------------------------------------------------------------

class TestSetCartesianProduct:
    def _get_tuple_mk(self):
        """Return the mk function for Tuple_Int_Int."""
        _ts, mk, _accs = TupleSort("Tuple_Int_Int", [IntSort(), IntSort()])
        return mk

    def test_valid_pair_in_product(self):
        """(1, 2) ∈ {1} × {2}"""
        s = singleton(IntVal(1), IntSort())
        t = singleton(IntVal(2), IntSort())
        product = set_cartesian_product(s, t, IntSort(), IntSort())
        mk = self._get_tuple_mk()
        solver = Solver()
        solver.add(Select(product, mk(IntVal(1), IntVal(2))))
        assert solver.check() == sat

    def test_invalid_pair_not_in_product(self):
        """(1, 3) ∉ {1} × {2}"""
        s = singleton(IntVal(1), IntSort())
        t = singleton(IntVal(2), IntSort())
        product = set_cartesian_product(s, t, IntSort(), IntSort())
        mk = self._get_tuple_mk()
        solver = Solver()
        solver.add(Select(product, mk(IntVal(1), IntVal(3))))
        assert solver.check() == unsat

    def test_wrong_first_component_not_in_product(self):
        """(3, 2) ∉ {1} × {2}"""
        s = singleton(IntVal(1), IntSort())
        t = singleton(IntVal(2), IntSort())
        product = set_cartesian_product(s, t, IntSort(), IntSort())
        mk = self._get_tuple_mk()
        solver = Solver()
        solver.add(Select(product, mk(IntVal(3), IntVal(2))))
        assert solver.check() == unsat

    def test_larger_product(self):
        """{1,2} × {3,4}: all four combinations are members."""
        s = set_from_list([IntVal(1), IntVal(2)], IntSort())
        t = set_from_list([IntVal(3), IntVal(4)], IntSort())
        product = set_cartesian_product(s, t, IntSort(), IntSort())
        mk = self._get_tuple_mk()
        for a in [IntVal(1), IntVal(2)]:
            for b in [IntVal(3), IntVal(4)]:
                solver = Solver()
                solver.add(Select(product, mk(a, b)))
                assert solver.check() == sat, f"({a},{b}) should be in product"

    def test_cross_combinations_not_in_product(self):
        """{1,2} × {3,4}: (1,5) not in product."""
        s = set_from_list([IntVal(1), IntVal(2)], IntSort())
        t = set_from_list([IntVal(3), IntVal(4)], IntSort())
        product = set_cartesian_product(s, t, IntSort(), IntSort())
        mk = self._get_tuple_mk()
        solver = Solver()
        solver.add(Select(product, mk(IntVal(1), IntVal(5))))
        assert solver.check() == unsat


# ---------------------------------------------------------------------------
# set_image
# ---------------------------------------------------------------------------

class TestSetImage:
    def test_image_contains_mapped_values(self):
        """{ x + 10 | x ∈ {1,2,3} } contains 11, 12, 13."""
        s = set_from_list([IntVal(1), IntVal(2), IntVal(3)], IntSort())
        x = FreshConst(IntSort(), "x")
        f_array = Lambda([x], x + IntVal(10))
        img = set_image(f_array, s, IntSort(), IntSort())
        for v in [IntVal(11), IntVal(12), IntVal(13)]:
            assert _is_member(v, img), f"{v} should be in image"

    def test_image_excludes_non_mapped_values(self):
        """{ x + 10 | x ∈ {1,2,3} } does not contain 5."""
        s = set_from_list([IntVal(1), IntVal(2), IntVal(3)], IntSort())
        x = FreshConst(IntSort(), "x")
        f_array = Lambda([x], x + IntVal(10))
        img = set_image(f_array, s, IntSort(), IntSort())
        assert _not_member(IntVal(5), img), "5 should not be in image"

    def test_image_of_empty_set_is_empty(self):
        """Image of empty set is empty."""
        s = empty_set(IntSort())
        x = FreshConst(IntSort(), "x")
        f_array = Lambda([x], x + IntVal(10))
        img = set_image(f_array, s, IntSort(), IntSort())
        y = Int("y")
        solver = Solver()
        solver.add(Select(img, y))
        assert solver.check() == unsat
