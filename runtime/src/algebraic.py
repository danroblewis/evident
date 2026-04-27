"""
Phase 11: Algebraic/sum type encoding using Z3 Datatypes.

Provides utilities for declaring enum types and recursive datatypes, looking
up their constructors and recognizers, and translating pattern matching into
nested Z3 If-expressions.
"""

from __future__ import annotations

import z3

from .sorts import SortRegistry


# ---------------------------------------------------------------------------
# Enum declaration
# ---------------------------------------------------------------------------


def declare_enum(
    name: str,
    constructors: list[str],
    registry: SortRegistry,
) -> z3.DatatypeSortRef:
    """
    Declare a simple enum type: type Color = Red | Green | Blue.

    Each constructor has no arguments.  Returns the created (or previously
    cached) Z3 Datatype sort and registers it under *name* in *registry* so
    that ``registry.get(name)`` works afterwards.

    Calling this multiple times with the same *name* is idempotent — the
    existing sort is returned without re-creating it.
    """
    return registry.declare_algebraic(name, constructors)


# ---------------------------------------------------------------------------
# Constructor / recognizer lookup
# ---------------------------------------------------------------------------


def get_constructor(
    datatype: z3.DatatypeSortRef,
    name: str,
) -> z3.FuncDeclRef:
    """
    Return the constructor ``FuncDeclRef`` for the variant named *name*.

    For a zero-argument constructor ``Red`` of a ``Color`` datatype this
    returns the callable that, when applied with no arguments, produces the
    ``Red`` term — i.e. ``datatype.constructor(i)`` for the matching ``i``.

    Raises ``KeyError`` if no constructor with *name* exists.
    """
    for i in range(datatype.num_constructors()):
        c = datatype.constructor(i)
        if c.name() == name:
            return c
    raise KeyError(
        f"No constructor {name!r} in datatype {datatype.name()!r}. "
        f"Available: {[datatype.constructor(i).name() for i in range(datatype.num_constructors())]}"
    )


def get_recognizer(
    datatype: z3.DatatypeSortRef,
    name: str,
) -> z3.FuncDeclRef:
    """
    Return the ``is_<name>`` recognizer ``FuncDeclRef`` for the variant *name*.

    For example, for ``Red`` in ``Color`` this returns the Z3 ``is(Red)``
    function — i.e. ``datatype.recognizer(i)`` for the matching ``i``.

    Raises ``KeyError`` if no constructor with *name* exists.
    """
    for i in range(datatype.num_constructors()):
        c = datatype.constructor(i)
        if c.name() == name:
            return datatype.recognizer(i)
    raise KeyError(
        f"No constructor {name!r} in datatype {datatype.name()!r}. "
        f"Available: {[datatype.constructor(i).name() for i in range(datatype.num_constructors())]}"
    )


# ---------------------------------------------------------------------------
# Enum value helpers
# ---------------------------------------------------------------------------


def enum_values(
    datatype: z3.DatatypeSortRef,
    constructors: list[str],
) -> list[z3.ExprRef]:
    """
    Return Z3 expressions for each listed constructor.

    For each name in *constructors* the zero-argument constructor is applied
    to produce the corresponding ground term (e.g. ``Red``, ``Green``, ``Blue``).
    The order of the returned list matches the order of *constructors*.

    Raises ``KeyError`` if a name is not found.
    """
    return [get_constructor(datatype, name)() for name in constructors]


# ---------------------------------------------------------------------------
# Pattern matching translation
# ---------------------------------------------------------------------------


def translate_pattern_match(
    subject: z3.ExprRef,
    datatype: z3.DatatypeSortRef,
    cases: list[tuple[str, z3.BoolRef]],
    default: z3.BoolRef = z3.BoolVal(False),
) -> z3.BoolRef:
    """
    Translate pattern matching on an algebraic type into nested If-expressions.

    *cases* is a list of ``(constructor_name, constraint_when_matched)`` pairs
    evaluated in order.  The result has the shape::

        If(is_Red(subject),   case_Red,
        If(is_Green(subject), case_Green,
           default))

    The outermost case corresponds to the *first* entry in *cases*; later
    entries are nested deeper.  *default* is used when no case matches.

    Returns a ``z3.BoolRef`` (or whatever type the case constraints have —
    Z3 infers the common type).
    """
    result = default
    # Build from the inside out so the first case ends up outermost.
    for ctor_name, constraint in reversed(cases):
        recognizer = get_recognizer(datatype, ctor_name)
        result = z3.If(recognizer(subject), constraint, result)
    return result


# ---------------------------------------------------------------------------
# Recursive datatype helper
# ---------------------------------------------------------------------------


def declare_list_datatype(
    element_sort: z3.SortRef,
    registry: SortRegistry,
) -> z3.DatatypeSortRef:
    """
    Declare (or retrieve) a recursive ``List`` datatype over *element_sort*.

    The datatype has the shape::

        List T = Nil | Cons(head: T, tail: List T)

    The sort is registered in *registry* under the name
    ``"List_<element_sort_name>"`` so it can be looked up with
    ``registry.get("List_Nat")``, for example.

    Calling this multiple times with the same element sort is idempotent.
    """
    list_name = f"List_{element_sort.name()}"

    # Return cached version if already registered.
    try:
        return registry.get(list_name)  # type: ignore[return-value]
    except KeyError:
        pass

    ctx = registry._ctx

    if ctx:
        lst = z3.Datatype(list_name, ctx)
    else:
        lst = z3.Datatype(list_name)

    lst.declare("Nil")
    lst.declare("Cons", ("head", element_sort), ("tail", lst))
    sort = lst.create()
    registry.register(list_name, sort)
    return sort
