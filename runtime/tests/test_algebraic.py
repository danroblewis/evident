"""
Tests for runtime/src/algebraic.py — Phase 11: Algebraic/sum types.
"""

import pytest
import z3

from runtime.src.sorts import SortRegistry
from runtime.src.algebraic import (
    declare_enum,
    get_constructor,
    get_recognizer,
    enum_values,
    translate_pattern_match,
    declare_list_datatype,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def make_registry() -> SortRegistry:
    """Create a registry with an isolated Z3 context."""
    ctx = z3.Context()
    return SortRegistry(ctx=ctx)


# ---------------------------------------------------------------------------
# declare_enum
# ---------------------------------------------------------------------------


class TestDeclareEnum:
    def test_creates_z3_datatype(self):
        reg = make_registry()
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        assert Color is not None
        assert Color.name() == "Color"

    def test_correct_number_of_constructors(self):
        reg = make_registry()
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        assert Color.num_constructors() == 3

    def test_constructor_names(self):
        reg = make_registry()
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        names = {Color.constructor(i).name() for i in range(Color.num_constructors())}
        assert names == {"Red", "Green", "Blue"}

    def test_registered_in_registry(self):
        reg = make_registry()
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        retrieved = reg.get("Color")
        assert Color.eq(retrieved)

    def test_idempotent_same_sort_returned(self):
        reg = make_registry()
        c1 = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        c2 = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        assert c1.eq(c2)

    def test_two_values_enum(self):
        reg = make_registry()
        Bool2 = declare_enum("Bool2", ["T", "F"], reg)
        assert Bool2.num_constructors() == 2

    def test_single_value_enum(self):
        reg = make_registry()
        Unit = declare_enum("Unit", ["unit"], reg)
        assert Unit.num_constructors() == 1
        assert Unit.constructor(0).name() == "unit"


# ---------------------------------------------------------------------------
# Constructor and recognizer distinctness / tautologies
# ---------------------------------------------------------------------------


class TestConstructorDistinctness:
    def test_red_not_green(self):
        """Z3 knows Red != Green is satisfiable (they are distinct)."""
        reg = make_registry()
        ctx = reg._ctx
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        solver = z3.Solver(ctx=ctx)
        solver.add(Color.Red != Color.Green)
        assert solver.check() == z3.sat

    def test_red_equals_green_is_unsat(self):
        """Z3 knows Red == Green is unsatisfiable."""
        reg = make_registry()
        ctx = reg._ctx
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        solver = z3.Solver(ctx=ctx)
        solver.add(Color.Red == Color.Green)
        assert solver.check() == z3.unsat

    def test_red_equals_red_is_tautology(self):
        """Red == Red is always true — its negation is unsat."""
        reg = make_registry()
        ctx = reg._ctx
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        solver = z3.Solver(ctx=ctx)
        solver.add(z3.Not(Color.Red == Color.Red))
        assert solver.check() == z3.unsat

    def test_all_pairs_distinct(self):
        """Every pair of distinct constructors is unequal."""
        reg = make_registry()
        ctx = reg._ctx
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        ctors = [Color.constructor(i)() for i in range(3)]
        for i in range(3):
            for j in range(3):
                solver = z3.Solver(ctx=ctx)
                if i != j:
                    solver.add(ctors[i] == ctors[j])
                    assert solver.check() == z3.unsat, f"Expected {i} != {j} to be unsat"
                else:
                    solver.add(z3.Not(ctors[i] == ctors[j]))
                    assert solver.check() == z3.unsat, f"Expected {i} == {j} tautology"


# ---------------------------------------------------------------------------
# get_constructor
# ---------------------------------------------------------------------------


class TestGetConstructor:
    def test_returns_func_decl_ref(self):
        reg = make_registry()
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        c = get_constructor(Color, "Red")
        assert isinstance(c, z3.FuncDeclRef)

    def test_constructor_name_matches(self):
        reg = make_registry()
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        c = get_constructor(Color, "Green")
        assert c.name() == "Green"

    def test_constructor_applied_gives_value(self):
        reg = make_registry()
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        red_ctor = get_constructor(Color, "Red")
        red_val = red_ctor()
        assert str(red_val) == "Red"

    def test_unknown_constructor_raises_key_error(self):
        reg = make_registry()
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        with pytest.raises(KeyError, match="Yellow"):
            get_constructor(Color, "Yellow")


# ---------------------------------------------------------------------------
# get_recognizer
# ---------------------------------------------------------------------------


class TestGetRecognizer:
    def test_returns_func_decl_ref(self):
        reg = make_registry()
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        r = get_recognizer(Color, "Red")
        assert isinstance(r, z3.FuncDeclRef)

    def test_is_red_of_red_is_true(self):
        """is_Red(Red()) is always true — negation is unsat."""
        reg = make_registry()
        ctx = reg._ctx
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        is_red = get_recognizer(Color, "Red")
        red_val = get_constructor(Color, "Red")()
        solver = z3.Solver(ctx=ctx)
        solver.add(z3.Not(is_red(red_val)))
        assert solver.check() == z3.unsat

    def test_is_red_of_green_is_false(self):
        """is_Red(Green()) is always false — directly is unsat."""
        reg = make_registry()
        ctx = reg._ctx
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        is_red = get_recognizer(Color, "Red")
        green_val = get_constructor(Color, "Green")()
        solver = z3.Solver(ctx=ctx)
        solver.add(is_red(green_val))
        assert solver.check() == z3.unsat

    def test_is_green_of_red_is_false(self):
        reg = make_registry()
        ctx = reg._ctx
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        is_green = get_recognizer(Color, "Green")
        red_val = get_constructor(Color, "Red")()
        solver = z3.Solver(ctx=ctx)
        solver.add(is_green(red_val))
        assert solver.check() == z3.unsat

    def test_unknown_constructor_raises_key_error(self):
        reg = make_registry()
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        with pytest.raises(KeyError, match="Yellow"):
            get_recognizer(Color, "Yellow")


# ---------------------------------------------------------------------------
# enum_values
# ---------------------------------------------------------------------------


class TestEnumValues:
    def test_returns_correct_number_of_values(self):
        reg = make_registry()
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        vals = enum_values(Color, ["Red", "Green", "Blue"])
        assert len(vals) == 3

    def test_values_are_z3_exprs(self):
        reg = make_registry()
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        vals = enum_values(Color, ["Red", "Green", "Blue"])
        for v in vals:
            assert isinstance(v, z3.ExprRef)

    def test_values_match_names(self):
        reg = make_registry()
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        vals = enum_values(Color, ["Red", "Green", "Blue"])
        assert str(vals[0]) == "Red"
        assert str(vals[1]) == "Green"
        assert str(vals[2]) == "Blue"

    def test_subset_of_constructors(self):
        reg = make_registry()
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        vals = enum_values(Color, ["Red", "Blue"])
        assert len(vals) == 2
        assert str(vals[0]) == "Red"
        assert str(vals[1]) == "Blue"

    def test_unknown_name_raises(self):
        reg = make_registry()
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        with pytest.raises(KeyError):
            enum_values(Color, ["Red", "Purple"])


# ---------------------------------------------------------------------------
# translate_pattern_match
# ---------------------------------------------------------------------------


class TestTranslatePatternMatch:
    def test_returns_if_structure(self):
        reg = make_registry()
        ctx = reg._ctx
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        x = z3.Const("x", Color)
        result = translate_pattern_match(
            x,
            Color,
            [("Red", z3.BoolVal(True, ctx=ctx)), ("Green", z3.BoolVal(False, ctx=ctx))],
            default=z3.BoolVal(False, ctx=ctx),
        )
        # The result should be an If-expression (ArithRef or BoolRef)
        assert result is not None
        assert "is" in str(result) or "If" in str(result)

    def test_red_case_evaluates_correctly(self):
        reg = make_registry()
        ctx = reg._ctx
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        x = z3.Const("x", Color)
        result = translate_pattern_match(
            x,
            Color,
            [("Red", z3.IntVal(10, ctx=ctx)), ("Green", z3.IntVal(20, ctx=ctx))],
            default=z3.IntVal(-1, ctx=ctx),
        )
        solver = z3.Solver(ctx=ctx)
        solver.add(x == Color.Red)
        assert solver.check() == z3.sat
        m = solver.model()
        assert m.eval(result).as_long() == 10

    def test_green_case_evaluates_correctly(self):
        reg = make_registry()
        ctx = reg._ctx
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        x = z3.Const("x", Color)
        result = translate_pattern_match(
            x,
            Color,
            [("Red", z3.IntVal(10, ctx=ctx)), ("Green", z3.IntVal(20, ctx=ctx))],
            default=z3.IntVal(-1, ctx=ctx),
        )
        solver = z3.Solver(ctx=ctx)
        solver.add(x == Color.Green)
        assert solver.check() == z3.sat
        m = solver.model()
        assert m.eval(result).as_long() == 20

    def test_default_used_for_unmatched_case(self):
        reg = make_registry()
        ctx = reg._ctx
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        x = z3.Const("x", Color)
        result = translate_pattern_match(
            x,
            Color,
            [("Red", z3.IntVal(10, ctx=ctx)), ("Green", z3.IntVal(20, ctx=ctx))],
            default=z3.IntVal(-1, ctx=ctx),
        )
        solver = z3.Solver(ctx=ctx)
        solver.add(x == Color.Blue)
        assert solver.check() == z3.sat
        m = solver.model()
        assert m.eval(result).as_long() == -1

    def test_bool_pattern_match(self):
        """Use bool results for a predicate-style match."""
        reg = make_registry()
        ctx = reg._ctx
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        x = z3.Const("x", Color)
        is_warm = translate_pattern_match(
            x,
            Color,
            [("Red", z3.BoolVal(True, ctx=ctx)), ("Green", z3.BoolVal(False, ctx=ctx))],
            default=z3.BoolVal(False, ctx=ctx),
        )
        solver = z3.Solver(ctx=ctx)
        solver.add(x == Color.Red)
        solver.add(z3.Not(is_warm))
        # If x is Red and is_warm is False, that's a contradiction
        assert solver.check() == z3.unsat

    def test_empty_cases_returns_default(self):
        reg = make_registry()
        ctx = reg._ctx
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        x = z3.Const("x", Color)
        result = translate_pattern_match(x, Color, [], default=z3.IntVal(42, ctx=ctx))
        # Should be the default value directly
        solver = z3.Solver(ctx=ctx)
        solver.add(result != 42)
        assert solver.check() == z3.unsat


# ---------------------------------------------------------------------------
# Variable enumeration: solver can find exactly 3 Color values
# ---------------------------------------------------------------------------


class TestEnumeration:
    def test_exactly_three_color_values(self):
        """
        Enumerate all values of a Color variable.

        We iteratively add blocking clauses until the solver is unsat.
        The number of models found should be exactly 3.
        """
        reg = make_registry()
        ctx = reg._ctx
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        x = z3.Const("x", Color)

        solver = z3.Solver(ctx=ctx)
        # Assert exhaustiveness so the solver tracks x and assigns it in models.
        solver.add(
            z3.Or(Color.is_Red(x), Color.is_Green(x), Color.is_Blue(x))
        )
        found = []
        while solver.check() == z3.sat:
            m = solver.model()
            val = m[x]
            found.append(str(val))
            # Block this value
            solver.add(x != val)

        assert len(found) == 3
        assert set(found) == {"Red", "Green", "Blue"}

    def test_no_value_outside_constructors(self):
        """A Color variable cannot be anything other than Red, Green, or Blue."""
        reg = make_registry()
        ctx = reg._ctx
        Color = declare_enum("Color", ["Red", "Green", "Blue"], reg)
        x = z3.Const("x", Color)
        solver = z3.Solver(ctx=ctx)
        solver.add(x != Color.Red)
        solver.add(x != Color.Green)
        solver.add(x != Color.Blue)
        assert solver.check() == z3.unsat


# ---------------------------------------------------------------------------
# List T recursive datatype
# ---------------------------------------------------------------------------


class TestListDatatype:
    def test_declare_list_nat(self):
        reg = make_registry()
        ctx = reg._ctx
        NatList = declare_list_datatype(z3.IntSort(ctx), reg)
        assert NatList is not None
        assert NatList.name() == "List_Int"

    def test_list_has_nil_and_cons(self):
        reg = make_registry()
        ctx = reg._ctx
        NatList = declare_list_datatype(z3.IntSort(ctx), reg)
        names = {NatList.constructor(i).name() for i in range(NatList.num_constructors())}
        assert names == {"Nil", "Cons"}

    def test_registered_in_registry(self):
        reg = make_registry()
        ctx = reg._ctx
        NatList = declare_list_datatype(z3.IntSort(ctx), reg)
        retrieved = reg.get("List_Int")
        assert NatList.eq(retrieved)

    def test_idempotent(self):
        reg = make_registry()
        ctx = reg._ctx
        L1 = declare_list_datatype(z3.IntSort(ctx), reg)
        L2 = declare_list_datatype(z3.IntSort(ctx), reg)
        assert L1.eq(L2)

    def test_nil_is_nil(self):
        """is_Nil(Nil()) is a tautology — its negation is unsat."""
        reg = make_registry()
        ctx = reg._ctx
        NatList = declare_list_datatype(z3.IntSort(ctx), reg)
        is_nil = get_recognizer(NatList, "Nil")
        nil_val = get_constructor(NatList, "Nil")()
        solver = z3.Solver(ctx=ctx)
        solver.add(z3.Not(is_nil(nil_val)))
        assert solver.check() == z3.unsat

    def test_cons_is_not_nil(self):
        """is_Nil(Cons(...)) is always false."""
        reg = make_registry()
        ctx = reg._ctx
        NatList = declare_list_datatype(z3.IntSort(ctx), reg)
        is_nil = get_recognizer(NatList, "Nil")
        nil_val = get_constructor(NatList, "Nil")()
        cons_val = NatList.Cons(z3.IntVal(1, ctx=ctx), nil_val)
        solver = z3.Solver(ctx=ctx)
        solver.add(is_nil(cons_val))
        assert solver.check() == z3.unsat

    def test_encode_list_1_2_3(self):
        """[1, 2, 3] encodes as Cons(1, Cons(2, Cons(3, Nil)))."""
        reg = make_registry()
        ctx = reg._ctx
        NatList = declare_list_datatype(z3.IntSort(ctx), reg)
        nil = NatList.Nil
        lst = NatList.Cons(
            z3.IntVal(1, ctx=ctx),
            NatList.Cons(
                z3.IntVal(2, ctx=ctx),
                NatList.Cons(z3.IntVal(3, ctx=ctx), nil),
            ),
        )
        # The list should be constructible and not nil
        is_nil = get_recognizer(NatList, "Nil")
        solver = z3.Solver(ctx=ctx)
        solver.add(is_nil(lst))
        assert solver.check() == z3.unsat

    def test_head_of_cons_5_nil_is_5(self):
        """The head accessor of Cons(5, Nil) returns 5."""
        reg = make_registry()
        ctx = reg._ctx
        NatList = declare_list_datatype(z3.IntSort(ctx), reg)
        nil = NatList.Nil
        cons5 = NatList.Cons(z3.IntVal(5, ctx=ctx), nil)
        head_expr = NatList.head(cons5)
        # head should equal 5
        solver = z3.Solver(ctx=ctx)
        solver.add(head_expr != z3.IntVal(5, ctx=ctx))
        assert solver.check() == z3.unsat

    def test_tail_of_cons_1_cons_2_nil(self):
        """The tail of Cons(1, Cons(2, Nil)) is Cons(2, Nil)."""
        reg = make_registry()
        ctx = reg._ctx
        NatList = declare_list_datatype(z3.IntSort(ctx), reg)
        nil = NatList.Nil
        inner = NatList.Cons(z3.IntVal(2, ctx=ctx), nil)
        outer = NatList.Cons(z3.IntVal(1, ctx=ctx), inner)
        tail_expr = NatList.tail(outer)
        # tail should have head == 2
        solver = z3.Solver(ctx=ctx)
        solver.add(NatList.head(tail_expr) != z3.IntVal(2, ctx=ctx))
        assert solver.check() == z3.unsat

    def test_is_nil_recognizer_on_nil(self):
        """Direct attribute access: NatList.is_Nil(NatList.Nil) is true."""
        reg = make_registry()
        ctx = reg._ctx
        NatList = declare_list_datatype(z3.IntSort(ctx), reg)
        solver = z3.Solver(ctx=ctx)
        solver.add(z3.Not(NatList.is_Nil(NatList.Nil)))
        assert solver.check() == z3.unsat

    def test_membership_check_head_equals(self):
        """
        Head membership: if head of list is 5, then 5 is in position 0.

        Given l = Cons(5, Nil) and is_Cons(l), the head(l) == 5.
        """
        reg = make_registry()
        ctx = reg._ctx
        NatList = declare_list_datatype(z3.IntSort(ctx), reg)
        l = z3.Const("l", NatList)
        solver = z3.Solver(ctx=ctx)
        solver.add(NatList.is_Cons(l))
        solver.add(NatList.head(l) == z3.IntVal(5, ctx=ctx))
        assert solver.check() == z3.sat
        m = solver.model()
        # The head should be 5 in the model
        assert m.eval(NatList.head(l)).as_long() == 5
