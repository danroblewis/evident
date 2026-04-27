"""
Phase 2: Variable creation and schema instantiation.

Given a SchemaDecl AST node and an Environment of already-bound variables,
produces:
  - A new Environment with *all* schema parameters present (unbound params
    become fresh Z3 constants with the appropriate sort).
  - A list of Z3 BoolRef assertions representing type-level constraints
    (e.g.  x ∈ Nat  →  x >= 0).
"""
from __future__ import annotations
import z3

from .env import Environment
from .ast_types import SchemaDecl, Param, Identifier, MembershipConstraint, InlineEnumExpr

# sorts.py is being written in parallel (Phase 1).  Import it if available;
# fall back to a minimal stub so this module remains usable and testable
# independently.
try:
    from .sorts import SortRegistry  # type: ignore[import]
except ImportError:  # pragma: no cover – only during parallel development

    class SortRegistry:  # type: ignore[no-redef]
        """Minimal stub used when sorts.py has not been written yet."""

        def __init__(self):
            self._sorts: dict[str, z3.SortRef] = {}

        def get(self, type_name: str) -> z3.SortRef:
            """Return a Z3 sort for a type name using simple built-in rules."""
            _builtin: dict[str, z3.SortRef] = {
                "Nat": z3.IntSort(),
                "Int": z3.IntSort(),
                "Real": z3.RealSort(),
                "Bool": z3.BoolSort(),
                "String": z3.StringSort(),
            }
            if type_name in _builtin:
                return _builtin[type_name]
            raise KeyError(type_name)

        def declare_uninterpreted(self, type_name: str) -> z3.SortRef:
            """Declare (or retrieve) an uninterpreted sort."""
            if type_name not in self._sorts:
                self._sorts[type_name] = z3.DeclareSort(type_name)
            return self._sorts[type_name]


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

def make_const(name: str, sort: z3.SortRef, prefix: str = "") -> z3.ExprRef:
    """Create a fresh named Z3 constant.

    The full Z3 name is ``prefix + name`` when *prefix* is non-empty, which
    allows multiple instantiations of the same schema to have distinct
    variables.
    """
    full_name = f"{prefix}{name}" if prefix else name
    return z3.Const(full_name, sort)


def _resolve_type_name(param: Param) -> str:
    """Extract the type name string from a Param's *set* expression.

    For the common case ``x ∈ Nat`` the set expression is an ``Identifier``
    whose ``name`` attribute is ``"Nat"``.  Other expression forms are not
    handled here (they require the full elaborator from later phases); we
    return a sentinel ``"unknown"`` so that callers can decide how to proceed.
    """
    expr = param.set
    if isinstance(expr, Identifier):
        return expr.name
    return "unknown"


def type_constraint(var: z3.ExprRef, type_name: str) -> list[z3.BoolRef]:
    """Return Z3 constraints that enforce the type semantics.

    Currently handled:
    - ``Nat``: ``var >= 0``  (Z3 IntSort does not restrict to non-negatives on
      its own)
    - All other types: no additional constraints — the sort already encodes the
      necessary structure.
    """
    if type_name == "Nat":
        return [var >= 0]  # type: ignore[list-item]
    return []


def instantiate_schema(
    schema: SchemaDecl,
    given: Environment,
    registry: SortRegistry,
    prefix: str = "",
) -> tuple[Environment, list[z3.BoolRef]]:
    """Instantiate all parameters of *schema* against *given* bindings.

    For each parameter declared in ``schema.params``:
    - If **all** of its names are already bound in *given*, use those bindings.
    - Otherwise, create a fresh Z3 constant for each unbound name.

    Returns ``(new_env, type_constraints)`` where:
    - ``new_env`` is a copy of *given* extended with all schema variables.
    - ``type_constraints`` is the list of Z3 assertions that enforce the
      declared types (e.g. ``x >= 0`` for ``x ∈ Nat``).
    """
    env = Environment(bindings=dict(given.bindings), parent=given.parent)
    constraints: list[z3.BoolRef] = []

    # The parser emits all variable declarations as MembershipConstraint nodes
    # in the body (e.g. `n ∈ Nat`, `c ∈ Red | Green | Blue`). Scan for those
    # first so body translation can look up variables by name.
    for item in schema.body:
        if (
            isinstance(item, MembershipConstraint)
            and item.op == "∈"
            and isinstance(item.left, Identifier)
        ):
            name = item.left.name
            if env.lookup(name) is not None:
                continue  # already declared (from params or a prior body scan)

            if isinstance(item.right, InlineEnumExpr):
                # x ∈ Red | Green | Blue — auto-declare an anonymous enum sort
                variants = item.right.variants
                enum_name = "_Enum_" + "_".join(sorted(variants))
                sort = registry.declare_algebraic(enum_name, variants)
                type_name = enum_name
            else:
                type_name = item.right.name if isinstance(item.right, Identifier) else "unknown"
                try:
                    sort = registry.get(type_name)
                except KeyError:
                    sort = registry.declare_uninterpreted(type_name)

            existing = given.lookup(name)
            if existing is not None:
                env = env.bind(name, existing)
                var = existing
            else:
                var = make_const(name, sort, prefix=prefix)
                env = env.bind(name, var)
            constraints.extend(type_constraint(var, type_name))

    for param in schema.params:
        type_name = _resolve_type_name(param)
        try:
            sort = registry.get(type_name)
        except KeyError:
            # Unknown / custom type — auto-register as uninterpreted sort
            sort = registry.declare_uninterpreted(type_name)

        for name in param.names:
            existing = given.lookup(name)
            if existing is not None:
                # Variable is already bound — record it in the new env
                # (it may only be in a parent; ensure it's in the flat layer)
                env = env.bind(name, existing)
                var = existing
            else:
                # Create a fresh Z3 constant for this unbound variable
                var = make_const(name, sort, prefix=prefix)
                env = env.bind(name, var)

            # Collect type-level constraints regardless of whether the variable
            # was pre-bound — a pre-bound variable still has to satisfy Nat ≥ 0
            constraints.extend(type_constraint(var, type_name))

    return env, constraints
