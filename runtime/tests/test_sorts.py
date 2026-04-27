"""
Tests for runtime/src/sorts.py — Phase 1: Z3 sort registry.
"""

import pytest
import z3

from runtime.src.sorts import SortRegistry


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def make_registry() -> SortRegistry:
    """Create a registry with an isolated Z3 context so tests don't interfere."""
    ctx = z3.Context()
    return SortRegistry(ctx=ctx)


# ---------------------------------------------------------------------------
# Built-in type mappings
# ---------------------------------------------------------------------------


class TestBuiltinSorts:
    def test_nat_maps_to_int_sort(self):
        reg = make_registry()
        s = reg.get("Nat")
        assert s.kind() == z3.Z3_INT_SORT, f"Expected IntSort, got {s}"

    def test_int_maps_to_int_sort(self):
        reg = make_registry()
        s = reg.get("Int")
        assert s.kind() == z3.Z3_INT_SORT, f"Expected IntSort, got {s}"

    def test_real_maps_to_real_sort(self):
        reg = make_registry()
        s = reg.get("Real")
        assert s.kind() == z3.Z3_REAL_SORT, f"Expected RealSort, got {s}"

    def test_bool_maps_to_bool_sort(self):
        reg = make_registry()
        s = reg.get("Bool")
        assert s.kind() == z3.Z3_BOOL_SORT, f"Expected BoolSort, got {s}"

    def test_string_maps_to_string_sort(self):
        reg = make_registry()
        s = reg.get("String")
        assert s == z3.StringSort(reg._ctx), f"Expected StringSort, got {s}"

    def test_nat_and_int_produce_same_sort(self):
        reg = make_registry()
        nat_sort = reg.get("Nat")
        int_sort = reg.get("Int")
        assert nat_sort.eq(int_sort)

    def test_unknown_type_raises_key_error(self):
        reg = make_registry()
        with pytest.raises(KeyError, match="Unknown Evident type"):
            reg.get("UnknownType")


# ---------------------------------------------------------------------------
# set_sort
# ---------------------------------------------------------------------------


class TestSetSort:
    def test_set_int_is_array_int_bool(self):
        reg = make_registry()
        ctx = reg._ctx
        s = reg.set_sort(z3.IntSort(ctx))
        expected = z3.ArraySort(z3.IntSort(ctx), z3.BoolSort(ctx))
        assert s.eq(expected), f"Expected {expected}, got {s}"

    def test_set_real(self):
        reg = make_registry()
        ctx = reg._ctx
        s = reg.set_sort(z3.RealSort(ctx))
        expected = z3.ArraySort(z3.RealSort(ctx), z3.BoolSort(ctx))
        assert s.eq(expected)

    def test_set_bool(self):
        reg = make_registry()
        ctx = reg._ctx
        s = reg.set_sort(z3.BoolSort(ctx))
        expected = z3.ArraySort(z3.BoolSort(ctx), z3.BoolSort(ctx))
        assert s.eq(expected)

    def test_set_via_get_nat(self):
        """set_sort(reg.get('Nat')) should work end-to-end."""
        reg = make_registry()
        ctx = reg._ctx
        nat_sort = reg.get("Nat")
        s = reg.set_sort(nat_sort)
        expected = z3.ArraySort(z3.IntSort(ctx), z3.BoolSort(ctx))
        assert s.eq(expected)

    def test_set_sort_is_array_sort(self):
        reg = make_registry()
        ctx = reg._ctx
        s = reg.set_sort(z3.IntSort(ctx))
        assert s.kind() == z3.Z3_ARRAY_SORT

    def test_set_sort_domain_and_range(self):
        reg = make_registry()
        ctx = reg._ctx
        elem = z3.IntSort(ctx)
        s = reg.set_sort(elem)
        assert s.domain().eq(elem)
        assert s.range().eq(z3.BoolSort(ctx))


# ---------------------------------------------------------------------------
# declare_uninterpreted
# ---------------------------------------------------------------------------


class TestDeclareUninterpreted:
    def test_returns_sort_named_task(self):
        reg = make_registry()
        s = reg.declare_uninterpreted("Task")
        assert s.name() == "Task"

    def test_idempotent_same_call(self):
        reg = make_registry()
        s1 = reg.declare_uninterpreted("Task")
        s2 = reg.declare_uninterpreted("Task")
        assert s1.eq(s2)

    def test_different_names_give_different_sorts(self):
        reg = make_registry()
        s1 = reg.declare_uninterpreted("Task")
        s2 = reg.declare_uninterpreted("User")
        assert not s1.eq(s2)

    def test_registered_sort_retrievable_via_get(self):
        reg = make_registry()
        s1 = reg.declare_uninterpreted("Task")
        s2 = reg.get("Task")
        assert s1.eq(s2)

    def test_uninterpreted_sort_is_not_int(self):
        reg = make_registry()
        s = reg.declare_uninterpreted("Task")
        assert s.kind() != z3.Z3_INT_SORT


# ---------------------------------------------------------------------------
# declare_algebraic
# ---------------------------------------------------------------------------


class TestDeclareAlgebraic:
    def test_creates_datatype_with_correct_constructors(self):
        reg = make_registry()
        Color = reg.declare_algebraic("Color", ["Red", "Green", "Blue"])
        assert Color.num_constructors() == 3
        ctor_names = {
            Color.constructor(i).name()
            for i in range(Color.num_constructors())
        }
        assert ctor_names == {"Red", "Green", "Blue"}

    def test_datatype_name(self):
        reg = make_registry()
        Shape = reg.declare_algebraic("Shape", ["Circle", "Square"])
        assert Shape.name() == "Shape"

    def test_idempotent_same_call(self):
        reg = make_registry()
        c1 = reg.declare_algebraic("Direction", ["North", "South", "East", "West"])
        c2 = reg.declare_algebraic("Direction", ["North", "South", "East", "West"])
        assert c1.eq(c2)

    def test_registered_algebraic_retrievable_via_get(self):
        reg = make_registry()
        c1 = reg.declare_algebraic("Color", ["Red", "Green", "Blue"])
        c2 = reg.get("Color")
        assert c1.eq(c2)

    def test_constructor_values_are_distinct(self):
        """Z3 should know Red != Green."""
        reg = make_registry()
        ctx = reg._ctx
        Color = reg.declare_algebraic("Color", ["Red", "Green", "Blue"])
        solver = z3.Solver(ctx=ctx)
        solver.add(Color.Red == Color.Green)
        assert solver.check() == z3.unsat

    def test_single_constructor(self):
        reg = make_registry()
        Unit = reg.declare_algebraic("Unit", ["unit"])
        assert Unit.num_constructors() == 1
        assert Unit.constructor(0).name() == "unit"


# ---------------------------------------------------------------------------
# register
# ---------------------------------------------------------------------------


class TestRegister:
    def test_register_custom_sort(self):
        reg = make_registry()
        ctx = reg._ctx
        custom = z3.DeclareSort("MySort", ctx)
        reg.register("MySort", custom)
        assert reg.get("MySort").eq(custom)

    def test_register_idempotent_same_sort(self):
        reg = make_registry()
        ctx = reg._ctx
        custom = z3.DeclareSort("MySort", ctx)
        reg.register("MySort", custom)
        reg.register("MySort", custom)  # should not raise
        assert reg.get("MySort").eq(custom)

    def test_register_conflicting_sort_raises(self):
        reg = make_registry()
        ctx = reg._ctx
        s1 = z3.DeclareSort("A", ctx)
        s2 = z3.DeclareSort("B", ctx)
        reg.register("MySort", s1)
        with pytest.raises(ValueError, match="Conflicting sort registration"):
            reg.register("MySort", s2)


# ---------------------------------------------------------------------------
# tuple_sort
# ---------------------------------------------------------------------------


class TestTupleSort:
    def test_basic_int_bool_tuple(self):
        reg = make_registry()
        ctx = reg._ctx
        ts = reg.tuple_sort([z3.IntSort(ctx), z3.BoolSort(ctx)])
        assert ts is not None
        assert ts.name() == "Tuple_Int_Bool"

    def test_triple_sort(self):
        reg = make_registry()
        ctx = reg._ctx
        ts = reg.tuple_sort([z3.IntSort(ctx), z3.RealSort(ctx), z3.BoolSort(ctx)])
        assert ts.name() == "Tuple_Int_Real_Bool"

    def test_idempotent_same_sorts(self):
        reg = make_registry()
        ctx = reg._ctx
        ts1 = reg.tuple_sort([z3.IntSort(ctx), z3.BoolSort(ctx)])
        ts2 = reg.tuple_sort([z3.IntSort(ctx), z3.BoolSort(ctx)])
        assert ts1.eq(ts2)

    def test_empty_sorts_raises(self):
        reg = make_registry()
        with pytest.raises(ValueError, match="at least one sort"):
            reg.tuple_sort([])

    def test_single_element_tuple(self):
        reg = make_registry()
        ctx = reg._ctx
        ts = reg.tuple_sort([z3.IntSort(ctx)])
        assert ts.name() == "Tuple_Int"

    def test_tuple_sort_retrievable_via_get(self):
        reg = make_registry()
        ctx = reg._ctx
        ts1 = reg.tuple_sort([z3.IntSort(ctx), z3.BoolSort(ctx)])
        ts2 = reg.get("Tuple_Int_Bool")
        assert ts1.eq(ts2)

    def test_tuple_of_uninterpreted_sorts(self):
        reg = make_registry()
        ctx = reg._ctx
        task_sort = reg.declare_uninterpreted("Task")
        ts = reg.tuple_sort([task_sort, z3.IntSort(ctx)])
        assert ts.name() == "Tuple_Task_Int"


# ---------------------------------------------------------------------------
# Consistent context
# ---------------------------------------------------------------------------


class TestContextConsistency:
    def test_all_builtin_sorts_use_same_context(self):
        """Sorts produced by the registry must all share the same context."""
        ctx = z3.Context()
        reg = SortRegistry(ctx=ctx)
        sorts = [
            reg.get("Nat"),
            reg.get("Int"),
            reg.get("Real"),
            reg.get("Bool"),
            reg.get("String"),
        ]
        for s in sorts:
            assert s.ctx == ctx, f"Sort {s} has wrong context"

    def test_uninterpreted_sort_uses_same_context(self):
        ctx = z3.Context()
        reg = SortRegistry(ctx=ctx)
        s = reg.declare_uninterpreted("Task")
        assert s.ctx == ctx

    def test_algebraic_sort_uses_same_context(self):
        ctx = z3.Context()
        reg = SortRegistry(ctx=ctx)
        Color = reg.declare_algebraic("Color", ["Red", "Green", "Blue"])
        assert Color.ctx == ctx

    def test_set_sort_result_uses_same_context(self):
        ctx = z3.Context()
        reg = SortRegistry(ctx=ctx)
        s = reg.set_sort(z3.IntSort(ctx))
        assert s.ctx == ctx

    def test_tuple_sort_uses_same_context(self):
        ctx = z3.Context()
        reg = SortRegistry(ctx=ctx)
        ts = reg.tuple_sort([z3.IntSort(ctx), z3.BoolSort(ctx)])
        assert ts.ctx == ctx

    def test_default_context_registry(self):
        """SortRegistry() without a ctx still works using the global context."""
        reg = SortRegistry()
        s = reg.get("Nat")
        assert s.kind() == z3.Z3_INT_SORT
        s2 = reg.declare_uninterpreted("Widget")
        assert s2.name() == "Widget"
