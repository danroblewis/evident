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
from .ast_types import (SchemaDecl, Param, Identifier, MembershipConstraint,
                        InlineEnumExpr, PassthroughItem, EvidentBlock, TupleLiteral,
                        MultiMembershipDecl, SeqType, RegexLiteral,
                        ArithmeticConstraint, SetLiteral, EmptySet,
                        NatLiteral, IntLiteral, RealLiteral, StringLiteral)


def _is_type_decl(item) -> bool:
    """True for  x ∈ TypeName  declarations already handled by instantiate_schema.
    The right-hand side must be a bare Identifier (e.g. Nat, Real, Color),
    NOT a set literal ({30, 45, 60}) or range ({1..10}), which are constraints."""
    return (
        isinstance(item, MembershipConstraint)
        and item.op == "∈"
        and isinstance(item.left, Identifier)   # plain variable, not a tuple
        and isinstance(item.right, Identifier)  # bare type name, not {…} or range
    )

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


def _infer_set_element_sort(set_expr, registry: "SortRegistry") -> z3.SortRef:
    """Infer the element sort of a set literal from its contents."""
    if isinstance(set_expr, SetLiteral) and set_expr.elements:
        first = set_expr.elements[0]
        if isinstance(first, (NatLiteral, IntLiteral)):
            return registry._int_sort()
        if isinstance(first, RealLiteral):
            return registry._real_sort()
        if isinstance(first, StringLiteral):
            return registry._string_sort()
    return registry._int_sort()  # default


def _create_implicit_set_vars(
    schema: SchemaDecl,
    env: "Environment",
    registry: "SortRegistry",
    prefix: str,
) -> "Environment":
    """
    Pre-pass: create Array(T, Bool) variables for names that appear in
    set-context constraints (name = SetLiteral, or name ⊆/⊇ name) but
    are not yet declared in env.  This lets schemas like

        schema S
            A ⊆ B
            A = {1, 2, 3}
            B = {1, 2}

    work without explicit 'A ∈ SetType' declarations.
    """
    for item in schema.body:
        # name = SetLiteral  or  name = EmptySet
        if (isinstance(item, ArithmeticConstraint) and item.op == '='
                and isinstance(item.left, Identifier)
                and isinstance(item.right, (SetLiteral, EmptySet))):
            name = item.left.name
            if env.lookup(name) is None:
                elem_sort = _infer_set_element_sort(item.right, registry)
                set_sort = registry.set_sort(elem_sort)
                var = make_const(name, set_sort, prefix=prefix)
                env = env.bind(name, var)

        # A ⊆ B  or  A ⊇ B  — both sides may be undeclared
        if (isinstance(item, MembershipConstraint) and item.op in ('⊆', '⊇')):
            for side in [item.left, item.right]:
                if isinstance(side, Identifier) and env.lookup(side.name) is None:
                    # Default element sort: try to infer later; use Int for now
                    set_sort = registry.set_sort(registry._int_sort())
                    var = make_const(side.name, set_sort, prefix=prefix)
                    env = env.bind(side.name, var)
    return env


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

    # Create implicit set variables for names appearing in set-context
    # constraints (A = {…}, A ⊆ B) before the main instantiation loop.
    env = _create_implicit_set_vars(schema, env, registry, prefix)

    for item in schema.body:
        # ── Multi-name: x, y, z ∈ Type ──────────────────────────────────
        if isinstance(item, MultiMembershipDecl):
            type_name = item.set.name if isinstance(item.set, Identifier) else "unknown"
            if isinstance(item.set, InlineEnumExpr):
                variants  = item.set.variants
                type_name = "_Enum_" + "_".join(sorted(variants))
                sort      = registry.declare_algebraic(type_name, variants)
            else:
                try:
                    sort = registry.get(type_name)
                except KeyError:
                    sort = registry.declare_uninterpreted(type_name)
            for name in item.names:
                if env.lookup(name) is not None:
                    continue
                existing = given.lookup(name)
                if existing is not None:
                    env = env.bind(name, existing)
                    var = existing
                else:
                    var = make_const(name, sort, prefix=prefix)
                    env = env.bind(name, var)
                constraints.extend(type_constraint(var, type_name))
            continue

        # ── Passthrough: ..sub_schema ────────────────────────────────────
        # Flat-merges the sub-schema into the current scope.  Variables with
        # the same name are unified (relational join); new variables are added
        # directly (no dot prefix).  All sub-schema constraints are imported.
        if isinstance(item, PassthroughItem):
            if schemas and item.name in schemas:
                sub_schema = schemas[item.name]
                # Apply explicit slot renames before names-match.
                # ..claim (bronth ↦ month) pre-binds sub-schema's `bronth`
                # to the parent's `month` Z3 variable so they share identity.
                given_for_sub = env
                for mapping in item.mappings:
                    parent_name = (
                        mapping.value.name
                        if isinstance(mapping.value, Identifier) else None
                    )
                    if not parent_name:
                        continue
                    # Leaf variable: bind directly
                    parent_var = env.lookup(parent_name)
                    if parent_var is not None:
                        given_for_sub = given_for_sub.bind(mapping.slot, parent_var)
                    else:
                        # Sub-schema: map all parent_name.* entries to slot.*
                        # e.g. next mapsto state_next binds next.location → state_next.location's Z3 var
                        prefix_str = parent_name + '.'
                        for env_name, env_var in env.bindings.items():
                            if env_name.startswith(prefix_str):
                                field = env_name[len(prefix_str):]
                                given_for_sub = given_for_sub.bind(
                                    mapping.slot + '.' + field, env_var
                                )
                # Passing the (possibly augmented) env as `given` means shared
                # names unify automatically (names-match relational join).
                sub_env, sub_type_constraints = instantiate_schema(
                    sub_schema, given_for_sub, registry, prefix=prefix, schemas=schemas
                )
                constraints.extend(sub_type_constraints)
                # Import sub-schema body constraints
                from .translate import translate_constraint
                for sub_item in sub_schema.body:
                    if isinstance(sub_item, (EvidentBlock, PassthroughItem)) or _is_type_decl(sub_item):
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

        # ── Regex literal: name ∈ /pattern/ → String ───────────────────
        if isinstance(item.right, RegexLiteral):
            sort = registry.get('String')
            existing = given.lookup(name)
            if existing is not None:
                env = env.bind(name, existing)
            else:
                var = make_const(name, sort, prefix=prefix)
                env = env.bind(name, var)
            continue

        # ── Seq type: name ∈ Seq(T) ─────────────────────────────────────
        if isinstance(item.right, SeqType):
            try:
                elem_sort = registry.get(item.right.element_name)
            except KeyError:
                elem_sort = registry.declare_uninterpreted(item.right.element_name)
            seq_sort = z3.SeqSort(elem_sort)
            existing = given.lookup(name)
            if existing is not None:
                env = env.bind(name, existing)
            else:
                var = make_const(name, seq_sort, prefix=prefix)
                env = env.bind(name, var)
            continue

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

        # If the right-hand side is an env variable (not a type name), this is a
        # constraint like `5 ∈ s` or `Hearts ∈ hand` — skip instantiation.
        if env.lookup(type_name) is not None:
            continue
        # Also skip if left side is a known enum constructor (Hearts, Red, etc.)
        if registry.get_constructor(name) is not None:
            continue

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
                if isinstance(sub_item, (EvidentBlock, PassthroughItem)) or _is_type_decl(sub_item):
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
