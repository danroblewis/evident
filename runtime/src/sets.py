"""
Phase 4: Set operations encoded as Z3 Array theory.

In Z3, ``Set T`` is encoded as ``Array(T_sort, BoolSort())``.
Membership ``x ∈ S`` is ``Select(S, x)`` — array select returns True if x is in the set.
This makes all set operations expressible as array operations.
"""

from z3 import (
    ArrayRef,
    BoolRef,
    BoolVal,
    ExprRef,
    Exists,
    FreshConst,
    ForAll,
    Implies,
    K,
    Lambda,
    Not,
    Or,
    And,
    Select,
    SortRef,
    Store,
    TupleSort,
)


def empty_set(element_sort: SortRef) -> ArrayRef:
    """The empty set: K(sort, False) — constant false array."""
    return K(element_sort, BoolVal(False))


def singleton(x: ExprRef, element_sort: SortRef) -> ArrayRef:
    """Set containing only x."""
    return Store(empty_set(element_sort), x, BoolVal(True))


def set_union(s: ArrayRef, t: ArrayRef, element_sort: SortRef) -> ArrayRef:
    """S ∪ T: λx. S[x] ∨ T[x]"""
    x = FreshConst(element_sort, "x")
    return Lambda([x], Or(Select(s, x), Select(t, x)))


def set_intersection(s: ArrayRef, t: ArrayRef, element_sort: SortRef) -> ArrayRef:
    """S ∩ T: λx. S[x] ∧ T[x]"""
    x = FreshConst(element_sort, "x")
    return Lambda([x], And(Select(s, x), Select(t, x)))


def set_difference(s: ArrayRef, t: ArrayRef, element_sort: SortRef) -> ArrayRef:
    """S \\ T: λx. S[x] ∧ ¬T[x]"""
    x = FreshConst(element_sort, "x")
    return Lambda([x], And(Select(s, x), Not(Select(t, x))))


def set_member(x: ExprRef, s: ArrayRef) -> BoolRef:
    """x ∈ S: Select(S, x)"""
    return Select(s, x)


def set_not_member(x: ExprRef, s: ArrayRef) -> BoolRef:
    """x ∉ S: ¬Select(S, x)"""
    return Not(Select(s, x))


def set_subset(s: ArrayRef, t: ArrayRef, element_sort: SortRef) -> BoolRef:
    """S ⊆ T: ∀x. S[x] → T[x]"""
    x = FreshConst(element_sort, "x")
    return ForAll([x], Implies(Select(s, x), Select(t, x)))


def set_from_list(elements: list, element_sort: SortRef) -> ArrayRef:
    """Build a set from a list of concrete Z3 values."""
    s = empty_set(element_sort)
    for e in elements:
        s = Store(s, e, BoolVal(True))
    return s


def set_complement(s: ArrayRef, element_sort: SortRef) -> ArrayRef:
    """Sᶜ: λx. ¬S[x]"""
    x = FreshConst(element_sort, "x")
    return Lambda([x], Not(Select(s, x)))


def set_cartesian_product(
    s: ArrayRef,
    t: ArrayRef,
    s_sort: SortRef,
    t_sort: SortRef,
) -> ArrayRef:
    """S × T: set of all pairs (a, b) where a ∈ S and b ∈ T.

    Returns ``Array(TupleSort([s_sort, t_sort]), Bool)``.
    The TupleSort name is derived from the component sort names so that repeated
    calls with the same sorts use a consistent (and idempotent) Z3 datatype.
    """
    tuple_name = f"Tuple_{s_sort.name()}_{t_sort.name()}"
    ts, _mk, accs = TupleSort(tuple_name, [s_sort, t_sort])
    p = FreshConst(ts, "p")
    return Lambda([p], And(Select(s, accs[0](p)), Select(t, accs[1](p))))


def set_image(
    f_array: ArrayRef, s: ArrayRef, domain_sort: SortRef, range_sort: SortRef
) -> ArrayRef:
    """Image of f over S: { f(x) | x ∈ S }.

    Encoded as: λy. ∃x. S[x] ∧ f[x] = y

    Parameters
    ----------
    f_array:     Array(domain_sort, range_sort) — the function
    s:           Array(domain_sort, Bool) — the source set
    domain_sort: Z3 sort of domain elements
    range_sort:  Z3 sort of range elements (determines element sort of result set)
    """
    x = FreshConst(domain_sort, "x")
    y = FreshConst(range_sort, "y")
    return Lambda([y], Exists([x], And(Select(s, x), Select(f_array, x) == y)))
