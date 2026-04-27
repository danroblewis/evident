"""
Phase 5 tests: Set comprehensions and filter sugar.

All tests use a Z3 Solver to verify correctness by checking satisfiability.
"""

import pytest
from z3 import (
    And,
    BoolVal,
    Exists,
    FreshConst,
    IntSort,
    IntVal,
    K,
    Lambda,
    Not,
    Solver,
    Select,
    Store,
    TupleSort,
    sat,
    unsat,
)

from runtime.src.comprehension import (
    translate_filter,
    translate_field_projection,
    translate_grouped_by,
    translate_set_comprehension,
)
from runtime.src.env import Environment
from runtime.src.sets import set_from_list
from runtime.src.sorts import SortRegistry
from runtime.src.ast_types import (
    ArithmeticConstraint,
    Binding,
    BinaryExpr,
    ComprehensionGenerator,
    FieldAccess,
    Identifier,
    IntLiteral,
    SetComprehension,
    TupleLiteral,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _is_member(val, s) -> bool:
    """Return True if val is provably a member of set s."""
    solver = Solver()
    solver.add(Select(s, val))
    return solver.check() == sat


def _not_member(val, s) -> bool:
    """Return True if val is provably NOT a member of set s (unsat)."""
    solver = Solver()
    solver.add(Select(s, val))
    return solver.check() == unsat


def _mk_tuple2():
    """Return (sort, mk, accs) for Tuple_Int_Int."""
    return TupleSort("Tuple_Int_Int", [IntSort(), IntSort()])


def _make_env_registry():
    return Environment(), SortRegistry()


# ---------------------------------------------------------------------------
# Test 1: Simple filter — {1,2,3,4,5}[. > 3]
# ---------------------------------------------------------------------------


class TestSimpleFilter:
    """Filter a plain integer set by a comparison on the current element."""

    def _build_filtered(self):
        elem_sort = IntSort()
        s = set_from_list(
            [IntVal(1), IntVal(2), IntVal(3), IntVal(4), IntVal(5)], elem_sort
        )
        env, registry = _make_env_registry()
        # Condition: . > 3  (ArithmeticConstraint with '.' as the element)
        condition = ArithmeticConstraint(
            op=">", left=Identifier("."), right=IntLiteral(3)
        )
        return translate_filter(s, condition, elem_sort, env, registry)

    def test_four_is_in_result(self):
        filtered = self._build_filtered()
        assert _is_member(IntVal(4), filtered), "4 > 3 so 4 should be in result"

    def test_five_is_in_result(self):
        filtered = self._build_filtered()
        assert _is_member(IntVal(5), filtered), "5 > 3 so 5 should be in result"

    def test_one_not_in_result(self):
        filtered = self._build_filtered()
        assert _not_member(IntVal(1), filtered), "1 ≤ 3 so 1 should not be in result"

    def test_two_not_in_result(self):
        filtered = self._build_filtered()
        assert _not_member(IntVal(2), filtered), "2 ≤ 3 so 2 should not be in result"

    def test_three_not_in_result(self):
        filtered = self._build_filtered()
        assert _not_member(IntVal(3), filtered), "3 ≤ 3 so 3 should not be in result"

    def test_value_not_in_original_not_in_result(self):
        """10 is not in the original set so it cannot be in the filtered set."""
        filtered = self._build_filtered()
        assert _not_member(IntVal(10), filtered), "10 not in original set"


# ---------------------------------------------------------------------------
# Test 2: Filter with field — records {id, val} where .val > 10
# ---------------------------------------------------------------------------


class TestFilterWithField:
    """
    Set of (id: Int, val: Int) tuples; filter where .val > 10.

    The tuple sort is Tuple_Int_Int where slot 0 = id, slot 1 = val.
    A field array `val : Array(tuple_sort, Int)` is stored in env, and
    translate_filter automatically exposes it as `..val` (the dotted-name
    key that translate_expr uses for FieldAccess(Identifier('.'), 'val')).
    """

    def _setup(self):
        ts, mk, accs = _mk_tuple2()
        tuple_sort = ts

        # {(1, 5), (2, 15), (3, 20)}
        s = K(tuple_sort, BoolVal(False))
        s = Store(s, mk(IntVal(1), IntVal(5)), BoolVal(True))
        s = Store(s, mk(IntVal(2), IntVal(15)), BoolVal(True))
        s = Store(s, mk(IntVal(3), IntVal(20)), BoolVal(True))

        # val field accessor: Array(tuple_sort, Int) — accs[1](t)
        dummy = FreshConst(tuple_sort, "dummy_fwf")
        val_array = Lambda([dummy], accs[1](dummy))

        env = Environment({"val": val_array})
        registry = SortRegistry()

        condition = ArithmeticConstraint(
            op=">",
            left=FieldAccess(obj=Identifier("."), field="val"),
            right=IntLiteral(10),
        )
        filtered = translate_filter(s, condition, tuple_sort, env, registry)
        return filtered, mk

    def test_record_with_val_15_in_result(self):
        filtered, mk = self._setup()
        assert _is_member(mk(IntVal(2), IntVal(15)), filtered)

    def test_record_with_val_20_in_result(self):
        filtered, mk = self._setup()
        assert _is_member(mk(IntVal(3), IntVal(20)), filtered)

    def test_record_with_val_5_not_in_result(self):
        filtered, mk = self._setup()
        assert _not_member(mk(IntVal(1), IntVal(5)), filtered)

    def test_record_with_val_10_not_in_result(self):
        """Boundary: .val > 10 is strict, so val=10 is excluded."""
        ts, mk, accs = _mk_tuple2()
        s = K(ts, BoolVal(False))
        s = Store(s, mk(IntVal(99), IntVal(10)), BoolVal(True))
        dummy = FreshConst(ts, "dummy_boundary")
        val_array = Lambda([dummy], accs[1](dummy))
        env = Environment({"val": val_array})
        registry = SortRegistry()
        condition = ArithmeticConstraint(
            op=">",
            left=FieldAccess(obj=Identifier("."), field="val"),
            right=IntLiteral(10),
        )
        filtered = translate_filter(s, condition, ts, env, registry)
        assert _not_member(mk(IntVal(99), IntVal(10)), filtered)


# ---------------------------------------------------------------------------
# Test 3: Field projection — S.field over a concrete set
# ---------------------------------------------------------------------------


class TestFieldProjection:
    """
    S.field projects the 'val' field across a set of tuples.
    Result is the set of field values: { t.val | t ∈ S }.
    """

    def _setup(self):
        ts, mk, accs = _mk_tuple2()
        tuple_sort = ts

        # {(1, 10), (2, 20), (3, 30)}
        s = K(tuple_sort, BoolVal(False))
        s = Store(s, mk(IntVal(1), IntVal(10)), BoolVal(True))
        s = Store(s, mk(IntVal(2), IntVal(20)), BoolVal(True))
        s = Store(s, mk(IntVal(3), IntVal(30)), BoolVal(True))

        dummy = FreshConst(tuple_sort, "dummy_proj")
        val_array = Lambda([dummy], accs[1](dummy))

        env = Environment({"val": val_array})
        registry = SortRegistry()

        projected = translate_field_projection(
            s, "val", tuple_sort, IntSort(), env, registry
        )
        return projected

    def test_projected_contains_10(self):
        projected = self._setup()
        assert _is_member(IntVal(10), projected), "10 should be in S.val"

    def test_projected_contains_20(self):
        projected = self._setup()
        assert _is_member(IntVal(20), projected), "20 should be in S.val"

    def test_projected_contains_30(self):
        projected = self._setup()
        assert _is_member(IntVal(30), projected), "30 should be in S.val"

    def test_projected_excludes_non_field_value(self):
        projected = self._setup()
        assert _not_member(IntVal(5), projected), "5 not a .val in the set"

    def test_projection_of_empty_set_is_empty(self):
        """Projecting from the empty set yields the empty set."""
        ts, mk, accs = _mk_tuple2()
        s = K(ts, BoolVal(False))
        dummy = FreshConst(ts, "dummy_empty")
        val_array = Lambda([dummy], accs[1](dummy))
        env = Environment({"val": val_array})
        registry = SortRegistry()
        projected = translate_field_projection(s, "val", ts, IntSort(), env, registry)
        y = FreshConst(IntSort(), "y_check")
        solver = Solver()
        solver.add(Select(projected, y))
        assert solver.check() == unsat, "Projection of empty set must be empty"


# ---------------------------------------------------------------------------
# Test 4: Set comprehension with binding — { v | (i, v) ∈ entries }
# ---------------------------------------------------------------------------


class TestSetComprehensionSimpleBinding:
    """
    { v | (i, v) ∈ entries }

    Tuple destructuring: each element of entries is (Int, Int); the
    comprehension projects the second component.
    """

    def _setup(self):
        ts, mk, accs = _mk_tuple2()

        # entries = {(0,1), (1,2), (2,3)}
        entries_set = K(ts, BoolVal(False))
        entries_set = Store(entries_set, mk(IntVal(0), IntVal(1)), BoolVal(True))
        entries_set = Store(entries_set, mk(IntVal(1), IntVal(2)), BoolVal(True))
        entries_set = Store(entries_set, mk(IntVal(2), IntVal(3)), BoolVal(True))

        env = Environment({"entries": entries_set})
        registry = SortRegistry()

        # AST: { v | (i, v) ∈ entries }
        node = SetComprehension(
            output=Identifier("v"),
            generators=[
                ComprehensionGenerator(
                    binding=Binding(
                        names=["i", "v"], set=Identifier("entries")
                    )
                )
            ],
        )
        result_set = translate_set_comprehension(node, env, registry)
        return result_set

    def test_value_1_in_result(self):
        result = self._setup()
        assert _is_member(IntVal(1), result), "1 is a second component in entries"

    def test_value_2_in_result(self):
        result = self._setup()
        assert _is_member(IntVal(2), result), "2 is a second component in entries"

    def test_value_3_in_result(self):
        result = self._setup()
        assert _is_member(IntVal(3), result), "3 is a second component in entries"

    def test_value_not_in_entries_excluded(self):
        """Values that don't appear as second components are not in the result."""
        result = self._setup()
        assert _not_member(IntVal(4), result), "4 does not appear in entries"

    def test_value_0_excluded(self):
        """0 appears only as a first component (index), not a value."""
        result = self._setup()
        assert _not_member(IntVal(0), result), "0 is only an index in entries"


# ---------------------------------------------------------------------------
# Test 5: Consecutive pairs — { (v1, v2) | (i, v1) ∈ entries, (i+1, v2) ∈ entries }
# ---------------------------------------------------------------------------


class TestConsecutivePairsComprehension:
    """
    { (v1, v2) | (i, v1) ∈ entries, (i_next, v2) ∈ entries, i_next = i + 1 }

    entries = {(0,1), (1,2), (2,3)}
    Expected result: {(1,2), (2,3)} — consecutive value pairs.

    Implementation note: consecutive pair comprehensions require the
    constraint i_next = i + 1 expressed as a separate constraint generator
    because AST bindings only support simple name lists on the left-hand side.
    """

    def _setup(self):
        ts, mk, accs = _mk_tuple2()

        # entries = {(0,1), (1,2), (2,3)}
        entries_set = K(ts, BoolVal(False))
        entries_set = Store(entries_set, mk(IntVal(0), IntVal(1)), BoolVal(True))
        entries_set = Store(entries_set, mk(IntVal(1), IntVal(2)), BoolVal(True))
        entries_set = Store(entries_set, mk(IntVal(2), IntVal(3)), BoolVal(True))

        env = Environment({"entries": entries_set})
        registry = SortRegistry()

        # { (v1, v2)
        #   | (i,      v1) ∈ entries      -- generator 1: Binding
        #   , (i_next, v2) ∈ entries      -- generator 2: Binding
        #   , i_next = i + 1              -- generator 3: ArithmeticConstraint
        # }
        node = SetComprehension(
            output=TupleLiteral([Identifier("v1"), Identifier("v2")]),
            generators=[
                ComprehensionGenerator(
                    binding=Binding(
                        names=["i", "v1"], set=Identifier("entries")
                    )
                ),
                ComprehensionGenerator(
                    binding=Binding(
                        names=["i_next", "v2"], set=Identifier("entries")
                    )
                ),
                ComprehensionGenerator(
                    constraint=ArithmeticConstraint(
                        op="=",
                        left=Identifier("i_next"),
                        right=BinaryExpr(
                            op="+",
                            left=Identifier("i"),
                            right=IntLiteral(1),
                        ),
                    )
                ),
            ],
        )
        result_set = translate_set_comprehension(node, env, registry)
        return result_set, mk

    def test_pair_1_2_in_result(self):
        """(1, 2) must be in the result: entries has (0,1) and (1,2)."""
        result, mk = self._setup()
        assert _is_member(mk(IntVal(1), IntVal(2)), result), "(1,2) should be in result"

    def test_pair_2_3_in_result(self):
        """(2, 3) must be in the result: entries has (1,2) and (2,3)."""
        result, mk = self._setup()
        assert _is_member(mk(IntVal(2), IntVal(3)), result), "(2,3) should be in result"

    def test_non_consecutive_pair_excluded(self):
        """(1, 3) skips an index so it must NOT be in the result."""
        result, mk = self._setup()
        assert _not_member(mk(IntVal(1), IntVal(3)), result), "(1,3) is not consecutive"

    def test_reversed_pair_excluded(self):
        """(2, 1) is the wrong order so it must NOT be in the result."""
        result, mk = self._setup()
        assert _not_member(mk(IntVal(2), IntVal(1)), result), "(2,1) reversal excluded"

    def test_only_two_pairs_exist(self):
        """
        With entries {(0,1),(1,2),(2,3)} there are exactly two consecutive
        pairs.  We check that no third pair (e.g. (3,4)) can be a member.
        """
        result, mk = self._setup()
        assert _not_member(mk(IntVal(3), IntVal(4)), result), "(3,4) not in entries"
