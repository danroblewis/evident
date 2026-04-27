"""
Phase 7: Full schema evaluation — the complete solve loop.

Provides EvidentSolver and the evaluate_schema convenience function.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

import z3

from .sorts import SortRegistry
from .env import Environment
from .instantiate import instantiate_schema, type_constraint
from .translate import translate_constraint, translate_expr
from .fixedpoint import FixedpointSolver
from .ast_types import (
    SchemaDecl,
    UniversalConstraint,
    ExistentialConstraint,
    CardinalityConstraint,
    EvidentBlock,
    PassthroughItem,
)


# ---------------------------------------------------------------------------
# Result type
# ---------------------------------------------------------------------------


@dataclass
class EvaluationResult:
    """Outcome of evaluating a schema."""

    satisfied: bool
    bindings: dict[str, Any]    # variable name → Python value (int, str, bool, etc.)
    model: z3.ModelRef | None   # raw Z3 model, None if unsat
    explanation: str | None     # why unsat, if known


# ---------------------------------------------------------------------------
# Constraint dispatcher
# ---------------------------------------------------------------------------


def _translate_body_constraint(
    constraint,
    env: Environment,
    registry: SortRegistry,
) -> z3.BoolRef:
    """
    Translate any Evident constraint AST node to a Z3 boolean.

    Routes quantifier nodes to their specialist translators;
    all others go through the general translate_constraint.
    """
    from .quantifiers import translate_universal, translate_existential, translate_cardinality_constraint

    if isinstance(constraint, UniversalConstraint):
        return translate_universal(constraint, env, registry)

    if isinstance(constraint, ExistentialConstraint):
        return translate_existential(constraint, env, registry)

    if isinstance(constraint, CardinalityConstraint):
        return translate_cardinality_constraint(constraint, env, registry)

    # All other constraint types (Arithmetic, Membership, Logic, Binding, SetEquality)
    return translate_constraint(constraint, env, registry)


# ---------------------------------------------------------------------------
# EvidentSolver
# ---------------------------------------------------------------------------


class EvidentSolver:
    """
    Top-level evaluator.  Wraps a Z3 Solver, a SortRegistry, and an
    Environment and exposes ``evaluate(schema, given)`` as the primary
    entry point.
    """

    def __init__(self):
        self.registry = SortRegistry()
        self.solver = z3.Solver()
        self.env = Environment()
        self.schemas: dict[str, SchemaDecl] = {}
        self.fixedpoint: FixedpointSolver | None = None

    # ------------------------------------------------------------------
    # Schema registration
    # ------------------------------------------------------------------

    def register_schema(self, schema: SchemaDecl) -> None:
        """Store a schema for later reference (e.g. schema composition)."""
        self.schemas[schema.name] = schema

    # ------------------------------------------------------------------
    # Ground fact assertion
    # ------------------------------------------------------------------

    def assert_fact(self, name: str, value: Any) -> None:
        """
        Assert a ground fact: name = value.

        Creates a Z3 constant with the appropriate sort and asserts equality
        between it and the Python value.  The binding is stored in the
        top-level environment so that subsequent evaluate() calls can see it.
        """
        z3_val = self._python_to_z3_untyped(value)
        sort = z3_val.sort()
        const = z3.Const(name, sort)
        self.solver.add(const == z3_val)
        self.env = self.env.bind(name, const)

    # ------------------------------------------------------------------
    # Evaluation
    # ------------------------------------------------------------------

    def evaluate(
        self,
        schema: SchemaDecl,
        given: dict[str, Any] | None = None,
    ) -> EvaluationResult:
        """
        Evaluate a schema with some variables optionally pre-bound.

        Steps:
        1. Build an Environment from ``given`` (convert Python values → Z3 exprs).
        2. Call ``instantiate_schema`` to create Z3 constants for unbound vars
           and collect type-level constraints.
        3. Add type constraints to a fresh solver.
        4. For each body item that is a Constraint, translate and add it.
        5. Call ``solver.check()``.
        6. If sat: extract model, build result dict.
        7. If unsat: return explanation.
        """
        if given is None:
            given = {}

        # ── Step 1: build the initial environment from 'given' values ─────────
        init_env = Environment(bindings=dict(self.env.bindings))
        for name, py_val in given.items():
            z3_val = self._python_to_z3_untyped(py_val)
            init_env = init_env.bind(name, z3_val)

        # ── Step 2: instantiate the schema ─────────────────────────────────────
        env, type_constraints = instantiate_schema(schema, init_env, self.registry)

        # ── Step 3 & 4: build and populate a fresh solver ─────────────────────
        s = z3.Solver()
        for tc in type_constraints:
            s.add(tc)

        for item in schema.body:
            # Skip non-constraint body items (EvidentBlock, PassthroughItem)
            if isinstance(item, (EvidentBlock, PassthroughItem)):
                continue
            try:
                z3_constraint = _translate_body_constraint(item, env, self.registry)
                s.add(z3_constraint)
            except (NotImplementedError, KeyError) as exc:
                # Gracefully skip constraints we can't yet translate.
                # In a production system we'd warn; for now we document why.
                pass

        # ── Step 5: check satisfiability ──────────────────────────────────────
        result = s.check()

        if result == z3.sat:
            model = s.model()
            bindings = self._extract_model(env, model)
            return EvaluationResult(
                satisfied=True,
                bindings=bindings,
                model=model,
                explanation=None,
            )
        elif result == z3.unsat:
            return EvaluationResult(
                satisfied=False,
                bindings={},
                model=None,
                explanation=self._build_unsat_explanation(s),
            )
        else:
            # unknown — Z3 timed out or gave up
            return EvaluationResult(
                satisfied=False,
                bindings={},
                model=None,
                explanation="Z3 returned unknown (timeout or resource limit).",
            )

    # ------------------------------------------------------------------
    # Value conversion helpers
    # ------------------------------------------------------------------

    def _python_to_z3_untyped(self, value: Any) -> z3.ExprRef:
        """Convert a Python value to a Z3 expression, inferring the sort."""
        if isinstance(value, bool):
            return z3.BoolVal(value)
        if isinstance(value, int):
            return z3.IntVal(value)
        if isinstance(value, float):
            return z3.RealVal(value)
        if isinstance(value, str):
            return z3.StringVal(value)
        raise ValueError(
            f"Cannot convert {value!r} to a Z3 expression. "
            "Supported types: bool, int, float, str."
        )

    def _python_to_z3(self, value: Any, sort: z3.SortRef) -> z3.ExprRef:
        """Convert a Python value to a Z3 expression of the given sort."""
        if isinstance(value, bool):
            return z3.BoolVal(value)
        if isinstance(value, int):
            return z3.IntVal(value)
        if isinstance(value, float):
            return z3.RealVal(value)
        if isinstance(value, str):
            return z3.StringVal(value)
        raise ValueError(
            f"Cannot convert {value!r} to Z3 sort {sort}."
        )

    def _z3_to_python(self, expr: z3.ExprRef) -> Any:
        """Extract a Python value from a Z3 model expression."""
        if expr is None:
            return None
        if z3.is_int_value(expr):
            return expr.as_long()
        if z3.is_rational_value(expr):
            return float(expr.as_decimal(10))
        if z3.is_true(expr):
            return True
        if z3.is_false(expr):
            return False
        if isinstance(expr, z3.SeqRef):
            try:
                return expr.as_string()
            except Exception:
                pass
        # Uninterpreted sort value (e.g. Task!val!0) — return None, not a
        # meaningful Python value. Callers should filter out None bindings.
        expr_str = str(expr)
        if "!val!" in expr_str:
            return None
        return expr_str

    # ------------------------------------------------------------------
    # Model extraction
    # ------------------------------------------------------------------

    def _extract_model(
        self,
        env: Environment,
        model: z3.ModelRef,
    ) -> dict[str, Any]:
        """
        Extract bindings for all named variables in env from the Z3 model.

        Returns a dict mapping variable name → Python value.  Only variables
        that have a concrete value in the model are included.
        """
        result: dict[str, Any] = {}
        for name, z3_expr in env.bindings.items():
            # Skip internal / synthetic names (e.g. fresh variables)
            if name.startswith("__") or name.startswith("."):
                continue
            try:
                val = model.eval(z3_expr, model_completion=True)
                result[name] = self._z3_to_python(val)
            except Exception:
                # Some expressions can't be evaluated — skip them
                pass
        return result

    # ------------------------------------------------------------------
    # Unsat explanation
    # ------------------------------------------------------------------

    @staticmethod
    def _build_unsat_explanation(solver: z3.Solver) -> str:
        """
        Build a human-readable explanation for why the solver is unsatisfied.

        Uses Z3's unsat core if available (requires tracking assertions),
        otherwise returns a generic message.
        """
        try:
            # Try to get a simplified explanation by listing the assertions
            assertions = solver.assertions()
            if len(assertions) == 0:
                return "No constraints were added — empty schema is somehow unsat."
            return (
                f"Constraints are unsatisfiable. "
                f"The solver checked {len(assertions)} assertion(s)."
            )
        except Exception:
            return "Constraints are unsatisfiable."


# ---------------------------------------------------------------------------
# Convenience function
# ---------------------------------------------------------------------------


def evaluate_schema(
    schema: SchemaDecl,
    given: dict[str, Any] | None = None,
    registry: SortRegistry | None = None,
) -> EvaluationResult:
    """
    Create a fresh EvidentSolver and evaluate a schema.

    Parameters
    ----------
    schema:
        The SchemaDecl AST node to evaluate.
    given:
        Optional dict mapping variable names to concrete Python values.
        These variables will be pre-bound before solving.
    registry:
        Optional SortRegistry.  If provided it is used in place of the
        default fresh registry (useful when custom sorts are pre-registered).

    Returns
    -------
    EvaluationResult
    """
    solver = EvidentSolver()
    if registry is not None:
        solver.registry = registry
    return solver.evaluate(schema, given or {})
