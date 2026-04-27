"""
Phase 1: Z3 sort registry.

Maps Evident type names/expressions to Z3 sorts. This is the foundation
for all subsequent phases of the runtime.
"""

from __future__ import annotations

import z3


# Built-in type names that map to primitive Z3 sorts.
_BUILTIN_SORTS = {
    "Nat":    "Int",   # non-negativity enforced as a separate constraint
    "Int":    "Int",
    "Real":   "Real",
    "Bool":   "Bool",
    "String": "String",
}


class SortRegistry:
    """
    Central registry that maps Evident type names to Z3 sorts.

    All methods accept an optional `ctx` at construction time and use it
    consistently so that tests can run in isolated Z3 contexts.
    """

    def __init__(self, ctx: z3.Context | None = None):
        self._ctx = ctx
        # name → z3.SortRef cache (covers both uninterpreted and algebraic sorts)
        self._registry: dict[str, z3.SortRef] = {}
        # Cache for tuple sorts keyed by a tuple of sort ids so we can look
        # them up without re-creating them.
        self._tuple_cache: dict[tuple[int, ...], z3.SortRef] = {}
        # Enum variant name → Z3 constructor value (e.g. "Red" → Color.Red)
        self._constructors: dict[str, z3.ExprRef] = {}

    # ------------------------------------------------------------------
    # Primitive sort helpers
    # ------------------------------------------------------------------

    def _int_sort(self) -> z3.ArithSortRef:
        return z3.IntSort(self._ctx) if self._ctx else z3.IntSort()

    def _real_sort(self) -> z3.ArithSortRef:
        return z3.RealSort(self._ctx) if self._ctx else z3.RealSort()

    def _bool_sort(self) -> z3.BoolSortRef:
        return z3.BoolSort(self._ctx) if self._ctx else z3.BoolSort()

    def _string_sort(self) -> z3.SeqSortRef:
        return z3.StringSort(self._ctx) if self._ctx else z3.StringSort()

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def get(self, type_name: str) -> z3.SortRef:
        """
        Return the Z3 sort for a built-in or previously registered type.

        Raises KeyError for unknown names.
        """
        if type_name in ("Nat", "Int"):
            return self._int_sort()
        if type_name == "Real":
            return self._real_sort()
        if type_name == "Bool":
            return self._bool_sort()
        if type_name == "String":
            return self._string_sort()

        # Check the custom registry (uninterpreted + algebraic sorts).
        if type_name in self._registry:
            return self._registry[type_name]

        raise KeyError(
            f"Unknown Evident type: {type_name!r}. "
            "Register it with register(), declare_uninterpreted(), or declare_algebraic()."
        )

    def register(self, name: str, sort: z3.SortRef) -> None:
        """
        Register an arbitrary Z3 sort under a given name.

        If a sort is already registered under that name, this is a no-op
        (idempotent as long as the same sort is used).
        """
        if name in self._registry:
            existing = self._registry[name]
            if not existing.eq(sort):
                raise ValueError(
                    f"Conflicting sort registration for {name!r}: "
                    f"already registered as {existing}, tried to register {sort}."
                )
            return
        self._registry[name] = sort

    def declare_uninterpreted(self, name: str) -> z3.SortRef:
        """
        Declare (or retrieve) an uninterpreted Z3 sort for a custom schema type.

        Calling this multiple times with the same name returns the same sort
        (idempotent).
        """
        if name in self._registry:
            return self._registry[name]

        sort = (
            z3.DeclareSort(name, self._ctx)
            if self._ctx
            else z3.DeclareSort(name)
        )
        self._registry[name] = sort
        return sort

    def declare_algebraic(
        self, name: str, constructors: list[str]
    ) -> z3.DatatypeSortRef:
        """
        Declare (or retrieve) a Z3 Datatype for an algebraic type.

        Example:
            Color = declare_algebraic("Color", ["Red", "Green", "Blue"])

        Each constructor is a no-argument constructor (an enumeration variant).
        Calling this multiple times with the same name and constructors returns
        the same sort (idempotent).
        """
        if name in self._registry:
            existing = self._registry[name]
            return existing  # type: ignore[return-value]

        dt = z3.Datatype(name, self._ctx) if self._ctx else z3.Datatype(name)
        for ctor in constructors:
            dt.declare(ctor)
        sort = dt.create()
        self._registry[name] = sort
        # Register each variant so translate_expr can resolve it by name
        for ctor in constructors:
            self._constructors[ctor] = getattr(sort, ctor)
        return sort

    def get_constructor(self, name: str):
        """Return the Z3 constructor value for an enum variant, or None."""
        return self._constructors.get(name)

    def set_sort(self, element_sort: z3.SortRef) -> z3.ArraySortRef:
        """
        Return the Z3 sort for ``Set T``.

        Encoded as ``ArraySort(T, Bool)`` — membership is a boolean-valued
        function on the domain sort.
        """
        bool_sort = self._bool_sort()
        return z3.ArraySort(element_sort, bool_sort)

    def tuple_sort(self, sorts: list[z3.SortRef]) -> z3.SortRef:
        """
        Return a Z3 sort for a fixed-arity tuple/product type.

        Uses a deterministic name derived from the component sort names so
        that repeated calls with the same sorts return the same Z3 sort
        (idempotent).

        Returns the sort reference; the mk_tuple constructor and field
        accessors can be retrieved from the sort object if needed later.
        """
        if not sorts:
            raise ValueError("tuple_sort requires at least one sort.")

        # Build a canonical name, e.g. "Tuple_Int_Bool"
        sort_name = "Tuple_" + "_".join(s.name() for s in sorts)

        if sort_name in self._registry:
            return self._registry[sort_name]

        kwargs = {"ctx": self._ctx} if self._ctx else {}
        ts, _mk, _accs = z3.TupleSort(sort_name, sorts, **kwargs)
        self._registry[sort_name] = ts
        return ts
