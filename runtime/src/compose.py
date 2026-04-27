"""
Phase 8: Schema composition mechanisms.

Implements names-match (natural join), passthrough (..schema), partial
application, and chain composition of Evident schemas.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Union

import z3

from .env import Environment
from .sorts import SortRegistry
from .instantiate import instantiate_schema, make_const, type_constraint, _resolve_type_name
from .ast_types import SchemaDecl, Param, Identifier


# ---------------------------------------------------------------------------
# Names-match composition
# ---------------------------------------------------------------------------


def names_match_compose(
    parent_env: Environment,
    sub_schema: SchemaDecl,
    registry: SortRegistry,
    explicit_mappings: dict[str, str] | None = None,
) -> tuple[Environment, list[z3.BoolRef]]:
    """
    Compose a sub-schema into a parent environment using names-match.

    For each variable in sub_schema.params:
      - If explicit_mappings provides a mapping slot→name, use parent_env[name]
        (i.e. explicit_mappings maps sub_schema_var_name → parent_env_var_name)
      - If the name exists in parent_env, use that binding (names-match / natural join)
      - Otherwise, create a fresh Z3 constant and add it to the merged env

    Returns (merged_env, type_constraints).
    explicit_mappings: {sub_schema_var_name: parent_env_var_name}
    """
    if explicit_mappings is None:
        explicit_mappings = {}

    env = Environment(bindings=dict(parent_env.bindings), parent=parent_env.parent)
    constraints: list[z3.BoolRef] = []

    for param in sub_schema.params:
        type_name = _resolve_type_name(param)
        try:
            sort = registry.get(type_name)
        except KeyError:
            sort = registry.declare_uninterpreted(type_name)

        for name in param.names:
            # Check if this sub-schema var is explicitly mapped to a parent var
            if name in explicit_mappings:
                parent_name = explicit_mappings[name]
                existing = parent_env.lookup(parent_name)
                if existing is None:
                    raise KeyError(
                        f"Explicit mapping: parent env has no variable {parent_name!r} "
                        f"(mapped from sub-schema var {name!r})"
                    )
                # The sub-schema's 'name' is identified with parent's 'parent_name'
                env = env.bind(name, existing)
                var = existing
            elif parent_env.is_bound(name):
                # Names-match: variable exists in parent — share it
                existing = parent_env.lookup(name)
                env = env.bind(name, existing)
                var = existing
            else:
                # Fresh variable — add to merged env
                var = make_const(name, sort)
                env = env.bind(name, var)

            constraints.extend(type_constraint(var, type_name))

    return env, constraints


# ---------------------------------------------------------------------------
# Passthrough composition  (..schema)
# ---------------------------------------------------------------------------


def passthrough_compose(
    parent_env: Environment,
    sub_schema: SchemaDecl,
    registry: SortRegistry,
    mappings: dict[str, str] | None = None,  # sub_var → parent_var renames
) -> tuple[Environment, list[z3.BoolRef]]:
    """
    ..schema — lift all sub-schema variables into parent scope.

    Same as names_match_compose: variables already in parent are shared;
    new ones are added as fresh constants.  The distinction from
    names_match_compose is semantic (the caller signals intent via ..),
    but the mechanism is identical — all variables end up in the returned env.

    mappings: optional rename table {sub_var_name: parent_var_name}.
    When provided, sub_var is treated as if it were named parent_var for
    the purposes of lookup/binding.
    """
    if mappings is None:
        mappings = {}

    env = Environment(bindings=dict(parent_env.bindings), parent=parent_env.parent)
    constraints: list[z3.BoolRef] = []

    for param in sub_schema.params:
        type_name = _resolve_type_name(param)
        try:
            sort = registry.get(type_name)
        except KeyError:
            sort = registry.declare_uninterpreted(type_name)

        for name in param.names:
            # Apply rename if provided
            effective_name = mappings.get(name, name)

            if parent_env.is_bound(effective_name):
                existing = parent_env.lookup(effective_name)
                # Bind under both the effective name and the original sub-schema name
                env = env.bind(effective_name, existing)
                if name != effective_name:
                    env = env.bind(name, existing)
                var = existing
            else:
                var = make_const(effective_name, sort)
                env = env.bind(effective_name, var)
                if name != effective_name:
                    env = env.bind(name, var)

            constraints.extend(type_constraint(var, type_name))

    return env, constraints


# ---------------------------------------------------------------------------
# Partial application
# ---------------------------------------------------------------------------


def partial_application(
    schema: SchemaDecl,
    fixed: dict[str, z3.ExprRef],
    registry: SortRegistry,
) -> "PartialSchema":
    """
    Fix some variables, leave others free.
    Returns a PartialSchema that remembers which vars are fixed.
    """
    return PartialSchema(base=schema, fixed=dict(fixed), registry=registry)


@dataclass
class PartialSchema:
    """A schema with some variables pre-bound to concrete Z3 values."""

    base: SchemaDecl
    fixed: dict[str, z3.ExprRef]      # var_name → fixed Z3 value
    registry: SortRegistry

    def instantiate(self, env: Environment) -> tuple[Environment, list[z3.BoolRef]]:
        """
        Apply to an environment, using fixed values for fixed vars.

        Fixed variables override both the environment and fresh-var creation.
        Non-fixed variables follow names-match: shared with parent if name
        exists, otherwise created fresh.
        """
        # Build a temporary env that includes the fixed bindings so that
        # names_match_compose will pick them up via the normal lookup path.
        fixed_env = Environment(bindings=dict(env.bindings), parent=env.parent)
        for var_name, z3_val in self.fixed.items():
            fixed_env = fixed_env.bind(var_name, z3_val)

        return names_match_compose(fixed_env, self.base, self.registry)


# ---------------------------------------------------------------------------
# Chain composition  (A · B · C)
# ---------------------------------------------------------------------------


def chain_compose(
    schemas: list[Union[SchemaDecl, PartialSchema]],
    registry: SortRegistry,
) -> tuple[Environment, list[z3.BoolRef]]:
    """
    A · B · C — compose a chain of schemas.

    Shared variable names are identified (natural join / names-match).
    Returns the merged environment and all type constraints collected from
    every schema in the chain.
    """
    env = Environment()
    all_constraints: list[z3.BoolRef] = []

    for schema in schemas:
        if isinstance(schema, PartialSchema):
            new_env, constraints = schema.instantiate(env)
        else:
            new_env, constraints = names_match_compose(env, schema, registry)
        env = new_env
        all_constraints.extend(constraints)

    return env, all_constraints
