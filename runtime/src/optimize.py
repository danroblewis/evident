"""
Phase 13: Performance optimizations for the Evident runtime.

Provides domain inference, quantifier-unrolling heuristics, and solver
timeout helpers so that the main evaluation loop can make smarter
decisions about how to encode constraints.
"""

from __future__ import annotations

import z3

from .ast_types import (
    SchemaDecl,
    ArithmeticConstraint,
    MembershipConstraint,
    Identifier,
    NatLiteral,
    SetLiteral,
    RangeLiteral,
)


# ---------------------------------------------------------------------------
# Domain inference
# ---------------------------------------------------------------------------


def infer_domain(schema: SchemaDecl) -> dict[str, tuple[int, int] | None]:
    """
    Scan a schema body for explicit bounds on Nat/Int variables.

    Returns {var_name: (lower, upper) | None} — None means unbounded.

    Example:
        'n > 5, n < 100' -> {'n': (6, 99)}
        'n in Nat' alone  -> {'n': (0, None)}

    Used to decide when to unroll quantifiers vs use ForAll.

    The returned tuple is always (lower, upper) where either component may
    be None when that bound has not been declared.  If both are present the
    range is inclusive: lower <= n <= upper.
    """
    bounds: dict[str, tuple[int | None, int | None]] = {}
    for item in schema.body:
        _scan_item_for_bounds(item, bounds)
    return bounds  # type: ignore[return-value]


def _scan_item_for_bounds(item, bounds: dict) -> None:
    """Recursively scan a single body item for numeric bounds."""
    if isinstance(item, ArithmeticConstraint):
        _update_bounds(item, bounds)
    elif isinstance(item, MembershipConstraint) and item.op == "∈":
        # 'x ∈ Nat' establishes a lower bound of 0 (no upper bound yet).
        if isinstance(item.left, Identifier) and isinstance(item.right, Identifier):
            if item.right.name == "Nat" and item.left.name not in bounds:
                bounds[item.left.name] = (0, None)


def _update_bounds(constraint: ArithmeticConstraint, bounds: dict) -> None:
    """
    Extract a numeric bound from an arithmetic comparison constraint.

    Only handles the simple form  <identifier> <op> <nat-literal>.
    """
    if not isinstance(constraint.left, Identifier):
        return
    if not isinstance(constraint.right, NatLiteral):
        return

    name = constraint.left.name
    val = constraint.right.value
    lo, hi = bounds.get(name, (None, None))

    if constraint.op in (">", "≥"):
        new_lo = (val + 1) if constraint.op == ">" else val
        # Take the tightest (largest) lower bound seen so far.
        lo = new_lo if lo is None else max(lo, new_lo)
    elif constraint.op in ("<", "≤"):
        new_hi = (val - 1) if constraint.op == "<" else val
        # Take the tightest (smallest) upper bound seen so far.
        hi = new_hi if hi is None else min(hi, new_hi)
    else:
        # '=' and '≠' — not a simple bound; skip.
        return

    bounds[name] = (lo, hi)


# ---------------------------------------------------------------------------
# Quantifier-unrolling heuristic
# ---------------------------------------------------------------------------


def should_unroll_quantifier(set_expr, domain_bound: int = 1000) -> bool:
    """
    Decide whether to unroll a quantifier or use Z3 ForAll/Exists.

    Unrolling is preferred for concrete, finite, small sets because Z3
    quantifiers are expensive and can produce unknowns.  For symbolic sets
    or large ranges, ForAll/Exists is safer.

    Parameters
    ----------
    set_expr:
        The AST expression that represents the quantifier's domain.
    domain_bound:
        Maximum set size (inclusive) for which unrolling is attempted.
        Default 1000.

    Returns
    -------
    True  — unroll (the set is small and concrete)
    False — use ForAll/Exists (symbolic or too large)
    """
    if isinstance(set_expr, SetLiteral):
        return len(set_expr.elements) <= domain_bound

    if isinstance(set_expr, RangeLiteral):
        if isinstance(set_expr.from_, NatLiteral) and isinstance(set_expr.to, NatLiteral):
            size = set_expr.to.value - set_expr.from_.value + 1
            return size <= domain_bound

    # Symbolic expression — use ForAll.
    return False


# ---------------------------------------------------------------------------
# Solver timeout helpers
# ---------------------------------------------------------------------------


def add_timeout(solver: z3.Solver, ms: int = 5000) -> None:
    """
    Configure a solver timeout in milliseconds.

    After this call solver.check() will return z3.unknown instead of
    blocking indefinitely if the problem is too hard.

    Parameters
    ----------
    solver:
        A z3.Solver instance.
    ms:
        Timeout in milliseconds.  Default 5000 (5 seconds).
    """
    solver.set("timeout", ms)


def check_with_timeout(
    solver: z3.Solver, timeout_ms: int = 5000
) -> z3.CheckSatResult:
    """
    Run solver.check() with a configurable timeout.

    Sets the timeout, then calls check().  If Z3 cannot determine
    satisfiability within *timeout_ms* milliseconds it returns z3.unknown.

    Parameters
    ----------
    solver:
        A z3.Solver instance (will have its timeout mutated as a side-effect).
    timeout_ms:
        Timeout in milliseconds.  Default 5000 (5 seconds).

    Returns
    -------
    z3.sat, z3.unsat, or z3.unknown.
    """
    add_timeout(solver, timeout_ms)
    return solver.check()
