"""
Phase 2: Variable creation and schema instantiation.

Given a SchemaDecl AST node and an Environment of already-bound variables,
produces:
  - A new Environment with *all* schema parameters present (unbound params
    become fresh Z3 constants with the appropriate sort).
  - A list of Z3 BoolRef assertions representing type-level constraints
    (e.g.  x ∈ Nat  →  x >= 0).

When a variable is declared as `name ∈ SomeSchema` and SomeSchema is a
known schema (not a primitive type), its fields are instantiated recursively
with a `name.` prefix so that `name.field` is available in the environment
for constraint translation and model extraction.
"""
from __future__ import annotations
import z3

from .env import Environment
from .ast_types import SchemaDecl, Param, Identifier, MembershipConstraint, InlineEnumExpr, PassthroughItem, EvidentBlock

try:
    from .sorts import SortRegistry  # type: ignore[import]
except ImportError:  # pragma: no cover

    class SortRegistry:  # type: ignore[no-redef]
        def __init__(self):
            self._sorts: dict[str, z3.SortRef] = {}

        def get(self, type_name: str) -> z3.SortRef:
            _builtin: dict[str, z3.SortRef] = {
                "Nat": z3.IntSort(), "Int": z3.IntSort(),
                "Real": z3.RealSort(), "Bool": z3.BoolSort(),
                "String": z3.StringSort(),
            }
            if type_name in _builtin:
                return _builtin[type_name]
            raise KeyError(type_name)

        def declare_uninterpreted(self, type_name: str) -> z3.SortRef:
            if type_name not in self._sorts:
                self._sorts[type_name] = z3.DeclareSort(type_name)
            return self._sorts[type_name]


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

def make_const(name: str, sort: z3.SortRef, prefix: str = "") -> z3.ExprRef:
    """Create a fresh named Z3 constant with an optional name prefix."""
    full_name = f"{prefix}{name}" if prefix else name
    return z3.Const(full_name, sort)


def _resolve_type_name(param: Param) -> str:
    expr = param.set
    if isinstance(expr, Identifier):
        return expr.name
    return "unknown"


def type_constraint(var: z3.ExprRef, type_name: str) -> list[z3.BoolRef]:
    if type_name == "Nat":
        return [var >= 0]  # type: ignore[list-item]
    return []


def instantiate_schema(
    schema: SchemaDecl,
    given: Environment,
    registry: SortRegistry,
    prefix: str = "",
    schemas: dict | None = None,
) -> tuple[Environment, list[z3.BoolRef]]:
    """Instantiate all variables of *schema* against *given* bindings.

    When *schemas* is provided, variables declared as ``name ∈ SomeSchema``
    are expanded recursively: each field of SomeSchema is added to the
    environment as ``name.field``, and SomeSchema's body constraints are
    collected.  This makes sub-schema fields available both for constraint
    translation (``slot + task.duration ≤ budget``) and model extraction
    (bindings include ``task.duration``, ``task.deadline``, etc.).
    """
    env = Environment(bindings=dict(given.bindings), parent=given.parent)
    constraints: list[z3.BoolRef] = []

    for item in schema.body:
        # ── Passthrough: ..sub_schema ────────────────────────────────────
        # Flat-merges the sub-schema into the current scope.  Variables with
        # the same name are unified (relational join); new variables are added
        # directly (no dot prefix).  All sub-schema constraints are imported.
        if isinstance(item, PassthroughItem):
            if schemas and item.name in schemas:
                sub_schema = schemas[item.name]
                # Passing the current env as `given` means shared names reuse
                # existing Z3 variables automatically (names-match join).
                sub_env, sub_type_constraints = instantiate_schema(
                    sub_schema, env, registry, prefix=prefix, schemas=schemas
                )
                constraints.extend(sub_type_constraints)
                # Import sub-schema body constraints
                from .translate import translate_constraint
                for sub_item in sub_schema.body:
                    if isinstance(sub_item, (MembershipConstraint, EvidentBlock, PassthroughItem)):
                        continue
                    try:
                        constraints.append(translate_constraint(sub_item, sub_env, registry))
                    except (NotImplementedError, KeyError):
                        pass
                # Merge new variables (not already in parent env) into parent
                for sub_name, sub_var in sub_env.bindings.items():
                    if env.lookup(sub_name) is None:
                        env = env.bind(sub_name, sub_var)
            continue

        if not (
            isinstance(item, MembershipConstraint)
            and item.op == "∈"
            and isinstance(item.left, Identifier)
        ):
            continue

        name = item.left.name
        if env.lookup(name) is not None:
            continue  # already declared

        # ── Inline enum: x ∈ Red | Green | Blue ─────────────────────────
        if isinstance(item.right, InlineEnumExpr):
            variants = item.right.variants
            enum_name = "_Enum_" + "_".join(sorted(variants))
            sort = registry.declare_algebraic(enum_name, variants)
            type_name = enum_name
            existing = given.lookup(name)
            if existing is not None:
                env = env.bind(name, existing)
                var = existing
            else:
                var = make_const(name, sort, prefix=prefix)
                env = env.bind(name, var)
            constraints.extend(type_constraint(var, type_name))
            continue

        type_name = item.right.name if isinstance(item.right, Identifier) else "unknown"

        # ── Sub-schema expansion: name ∈ SomeSchema ──────────────────────
        if schemas and type_name in schemas:
            sub_schema = schemas[type_name]

            # Build a sub-given from any `name.field` values in given
            sub_given = Environment()
            field_prefix = f"{name}."
            for k, v in given.bindings.items():
                if k.startswith(field_prefix):
                    sub_given = sub_given.bind(k[len(field_prefix):], v)

            sub_env, sub_type_constraints = instantiate_schema(
                sub_schema, sub_given, registry,
                prefix=f"{prefix}{name}.",
                schemas=schemas,
            )
            constraints.extend(sub_type_constraints)

            # Translate sub-schema body constraints (e.g. duration < deadline)
            # using the sub_env where names are short (duration, deadline, …)
            from .translate import translate_constraint
            for sub_item in sub_schema.body:
                if isinstance(sub_item, (MembershipConstraint, EvidentBlock, PassthroughItem)):
                    continue
                try:
                    constraints.append(translate_constraint(sub_item, sub_env, registry))
                except (NotImplementedError, KeyError):
                    pass

            # Merge sub-fields into parent env as `name.field`
            for sub_name, sub_var in sub_env.bindings.items():
                full_name = f"{name}.{sub_name}"
                if env.lookup(full_name) is None:
                    env = env.bind(full_name, sub_var)
            continue

        # ── Primitive / uninterpreted sort ───────────────────────────────
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

    # Handle params (legacy path, usually empty for body-declared schemas)
    for param in schema.params:
        type_name = _resolve_type_name(param)
        try:
            sort = registry.get(type_name)
        except KeyError:
            sort = registry.declare_uninterpreted(type_name)

        for name in param.names:
            existing = given.lookup(name)
            if existing is not None:
                env = env.bind(name, existing)
                var = existing
            else:
                var = make_const(name, sort, prefix=prefix)
                env = env.bind(name, var)
            constraints.extend(type_constraint(var, type_name))

    return env, constraints
