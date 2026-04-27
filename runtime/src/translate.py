"""
Phase 3: Basic constraint translation.

Translates Evident AST constraint and expression nodes to Z3 expressions
and boolean assertions.
"""

from __future__ import annotations

import z3

from .env import Environment
from .sorts import SortRegistry
from .ast_types import (
    # Constraints
    ArithmeticConstraint,
    MembershipConstraint,
    LogicConstraint,
    BindingConstraint,
    # Expressions
    Identifier,
    FieldAccess,
    TupleLiteral,
    BinaryExpr,
    UnaryExpr,
    NatLiteral,
    IntLiteral,
    RealLiteral,
    StringLiteral,
    BoolLiteral,
)

# Counter for generating fresh variable names in subset constraints.
_fresh_counter = 0


def _fresh_var(sort: z3.SortRef) -> z3.ExprRef:
    """Create a fresh Z3 constant of the given sort."""
    global _fresh_counter
    _fresh_counter += 1
    name = f"__z_fresh_{_fresh_counter}__"
    return z3.Const(name, sort)


# ---------------------------------------------------------------------------
# Expression translation
# ---------------------------------------------------------------------------


def translate_expr(expr, env: Environment, registry: SortRegistry) -> z3.ExprRef:
    """Translate an Evident expression to a Z3 expression.

    Parameters
    ----------
    expr:
        An AST expression node (Identifier, NatLiteral, BinaryExpr, …).
    env:
        The current variable environment mapping names to Z3 expressions.
    registry:
        The sort registry used for type look-ups (needed for tuple sorts).

    Returns
    -------
    z3.ExprRef
        The corresponding Z3 expression.

    Raises
    ------
    KeyError
        If an Identifier is not found in the environment.
    NotImplementedError
        For expression forms not yet handled.
    """

    # ── Identifier ────────────────────────────────────────────────────────────
    if isinstance(expr, Identifier):
        value = env.lookup(expr.name)
        if value is not None:
            return value
        # Fall back to enum constructor lookup (e.g. Red, Green, Blue)
        ctor = registry.get_constructor(expr.name)
        if ctor is not None:
            return ctor
        raise KeyError(
            f"Unbound variable {expr.name!r} in environment. "
            f"Bound names: {list(env.bindings.keys())}"
        )

    # ── Numeric literals ──────────────────────────────────────────────────────
    if isinstance(expr, (NatLiteral, IntLiteral)):
        return z3.IntVal(expr.value)

    if isinstance(expr, RealLiteral):
        return z3.RealVal(expr.value)

    # ── String literal ────────────────────────────────────────────────────────
    if isinstance(expr, StringLiteral):
        return z3.StringVal(expr.value)

    # ── Bool literal ──────────────────────────────────────────────────────────
    if isinstance(expr, BoolLiteral):
        return z3.BoolVal(expr.value)

    # ── Binary arithmetic / set ops ───────────────────────────────────────────
    if isinstance(expr, BinaryExpr):
        left = translate_expr(expr.left, env, registry)
        right = translate_expr(expr.right, env, registry)
        op = expr.op
        if op == "+":
            return left + right
        if op == "-":
            return left - right
        if op == "*":
            return left * right
        if op == "/":
            return left / right
        raise NotImplementedError(
            f"BinaryExpr op {op!r} not supported in translate_expr. "
            "Set operations (∪, ∩, \\, ×) are handled in the sets module."
        )

    # ── Unary negation ────────────────────────────────────────────────────────
    if isinstance(expr, UnaryExpr):
        if expr.op == "¬":
            return z3.Not(translate_expr(expr.operand, env, registry))
        raise NotImplementedError(f"UnaryExpr op {expr.op!r} not supported.")

    # ── Tuple literal ─────────────────────────────────────────────────────────
    if isinstance(expr, TupleLiteral):
        elements = [translate_expr(e, env, registry) for e in expr.elements]
        sorts = [e.sort() for e in elements]
        sort_name = "Tuple_" + "_".join(s.name() for s in sorts)
        # Retrieve or create the tuple sort so we have access to the constructor.
        if sort_name not in registry._registry:
            registry.tuple_sort(sorts)
        _ts, mk_tuple, _accs = z3.TupleSort(sort_name, sorts)
        return mk_tuple(*elements)

    # ── Field access ──────────────────────────────────────────────────────────
    if isinstance(expr, FieldAccess):
        # Simple case: the object is an Identifier — look up "obj.field".
        if isinstance(expr.obj, Identifier):
            key = f"{expr.obj.name}.{expr.field}"
            value = env.lookup(key)
            if value is not None:
                return value
            # Fall back to looking up the object and using a dotted-name convention.
            raise KeyError(
                f"Field access {key!r} not found in environment. "
                f"Bound names: {list(env.bindings.keys())}"
            )
        raise NotImplementedError(
            "FieldAccess on non-Identifier objects is not yet supported."
        )

    raise NotImplementedError(
        f"translate_expr: unsupported expression type {type(expr).__name__!r}. "
        f"Value: {expr!r}"
    )


# ---------------------------------------------------------------------------
# Constraint translation
# ---------------------------------------------------------------------------


def translate_constraint(
    constraint, env: Environment, registry: SortRegistry
) -> z3.BoolRef:
    """Translate an Evident constraint to a Z3 boolean expression.

    Parameters
    ----------
    constraint:
        An AST constraint node (ArithmeticConstraint, MembershipConstraint, …).
    env:
        The current variable environment.
    registry:
        The sort registry.

    Returns
    -------
    z3.BoolRef
        A Z3 boolean expression that encodes the constraint.

    Raises
    ------
    NotImplementedError
        For constraint or operator forms not yet handled.
    """

    # ── ArithmeticConstraint ──────────────────────────────────────────────────
    if isinstance(constraint, ArithmeticConstraint):
        left = translate_expr(constraint.left, env, registry)
        right = translate_expr(constraint.right, env, registry)
        op = constraint.op
        if op == "=":
            return left == right
        if op == "≠":
            return left != right
        if op == "<":
            return left < right
        if op == ">":
            return left > right
        if op == "≤":
            return left <= right
        if op == "≥":
            return left >= right
        raise NotImplementedError(f"ArithmeticConstraint op {op!r} not supported.")

    # ── MembershipConstraint ──────────────────────────────────────────────────
    if isinstance(constraint, MembershipConstraint):
        op = constraint.op
        left = constraint.left
        right = constraint.right

        # Determine if the right-hand side is a named primitive type.
        rhs_name = right.name if isinstance(right, Identifier) else None

        if op == "∈":
            if rhs_name == "Nat":
                # x ∈ Nat ≡ x ≥ 0  (Int with non-negativity constraint)
                x = translate_expr(left, env, registry)
                return x >= z3.IntVal(0)
            if rhs_name in ("Int", "Bool"):
                # No additional constraint beyond the variable's sort.
                return z3.BoolVal(True)
            # General case: right is a Set (Array sort) — use array select.
            x = translate_expr(left, env, registry)
            s = translate_expr(right, env, registry)
            return z3.Select(s, x)

        if op == "∉":
            x = translate_expr(left, env, registry)
            s = translate_expr(right, env, registry)
            return z3.Not(z3.Select(s, x))

        if op == "⊆":
            # S ⊆ T  ≡  ∀z. S[z] ⇒ T[z]
            s = translate_expr(left, env, registry)
            t = translate_expr(right, env, registry)
            # Infer element sort from the array domain.
            elem_sort = z3.ArraySort(s.sort().domain(), s.sort().range()).domain()
            z_var = _fresh_var(s.sort().domain())
            return z3.ForAll(
                [z_var],
                z3.Implies(z3.Select(s, z_var), z3.Select(t, z_var)),
            )

        if op == "⊇":
            # S ⊇ T  ≡  T ⊆ S
            s = translate_expr(left, env, registry)
            t = translate_expr(right, env, registry)
            z_var = _fresh_var(s.sort().domain())
            return z3.ForAll(
                [z_var],
                z3.Implies(z3.Select(t, z_var), z3.Select(s, z_var)),
            )

        raise NotImplementedError(
            f"MembershipConstraint op {op!r} not supported."
        )

    # ── LogicConstraint ───────────────────────────────────────────────────────
    if isinstance(constraint, LogicConstraint):
        op = constraint.op
        if op == "¬":
            return z3.Not(translate_constraint(constraint.right, env, registry))
        if op == "∧":
            return z3.And(
                translate_constraint(constraint.left, env, registry),
                translate_constraint(constraint.right, env, registry),
            )
        if op == "∨":
            return z3.Or(
                translate_constraint(constraint.left, env, registry),
                translate_constraint(constraint.right, env, registry),
            )
        if op == "⇒":
            return z3.Implies(
                translate_constraint(constraint.left, env, registry),
                translate_constraint(constraint.right, env, registry),
            )
        raise NotImplementedError(f"LogicConstraint op {op!r} not supported.")

    # ── BindingConstraint ─────────────────────────────────────────────────────
    if isinstance(constraint, BindingConstraint):
        # x = expr  →  translate_expr(x) == translate_expr(expr)
        lhs = env.lookup(constraint.name)
        if lhs is None:
            raise KeyError(
                f"BindingConstraint: variable {constraint.name!r} not found "
                f"in environment. Bound names: {list(env.bindings.keys())}"
            )
        rhs = translate_expr(constraint.value, env, registry)
        return lhs == rhs

    raise NotImplementedError(
        f"translate_constraint: unsupported constraint type "
        f"{type(constraint).__name__!r}. Value: {constraint!r}"
    )
