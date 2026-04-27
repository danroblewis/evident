"""
Phase 12: EvidentRuntime — the top-level API.

Manages a session: loaded schema definitions, asserted ground facts,
and the evidence base (grows monotonically).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from .evaluate import EvidentSolver, EvaluationResult
from .evidence import Evidence, evaluate_with_evidence
from .compose import names_match_compose, chain_compose, partial_application
from .fixedpoint import FixedpointSolver
from .sorts import SortRegistry
from .ast_types import (
    Program,
    SchemaDecl,
    AssertStmt,
    ForwardRule,
    ConstraintStmt,
    NatLiteral,
    IntLiteral,
    StringLiteral,
    BoolLiteral,
    RealLiteral,
)


# ---------------------------------------------------------------------------
# QueryResult
# ---------------------------------------------------------------------------


@dataclass
class QueryResult:
    """Outcome of a top-level query."""

    satisfied: bool
    bindings: dict[str, Any]
    evidence: Evidence | None


# ---------------------------------------------------------------------------
# EvidentRuntime
# ---------------------------------------------------------------------------


class EvidentRuntime:
    """
    The top-level Evident runtime. Manages a session:
    - Loaded schema definitions
    - Asserted ground facts
    - The evidence base (grows monotonically)
    """

    def __init__(self):
        self.solver = EvidentSolver()
        self.fixedpoint = FixedpointSolver(self.solver.registry)
        self.schemas: dict[str, SchemaDecl] = {}
        self.forward_rules: list[ForwardRule] = []
        self.evidence_base: list[Evidence] = []

    # ------------------------------------------------------------------
    # Schema registration
    # ------------------------------------------------------------------

    def load_schema(self, schema: SchemaDecl) -> None:
        """Register a schema definition."""
        self.schemas[schema.name] = schema
        self.solver.register_schema(schema)

    # ------------------------------------------------------------------
    # Program loading
    # ------------------------------------------------------------------

    def load_program(self, program: Program) -> None:
        """Load all statements from a parsed Program AST."""
        for stmt in program.statements:
            if isinstance(stmt, SchemaDecl):
                self.load_schema(stmt)
            elif isinstance(stmt, AssertStmt):
                self._handle_assert(stmt)
            elif isinstance(stmt, ForwardRule):
                self.forward_rules.append(stmt)
                # TODO: translate and register with fixedpoint engine
            elif isinstance(stmt, ConstraintStmt):
                # Top-level constraints — not yet handled
                pass

    # ------------------------------------------------------------------
    # Ground fact assertion
    # ------------------------------------------------------------------

    def assert_ground(self, name: str, value: Any) -> None:
        """Assert a concrete ground fact: name = value."""
        self.solver.assert_fact(name, value)

    # ------------------------------------------------------------------
    # Querying
    # ------------------------------------------------------------------

    def query(
        self,
        schema_name: str,
        given: dict[str, Any] | None = None,
    ) -> QueryResult:
        """
        Query whether the named schema can be satisfied.

        Parameters
        ----------
        schema_name:
            Name of a previously loaded SchemaDecl.
        given:
            Optional dict of pre-bound variable assignments.

        Returns
        -------
        QueryResult with satisfied, bindings, and evidence fields.

        Raises
        ------
        KeyError
            If no schema with that name has been loaded.
        """
        schema = self.schemas.get(schema_name)
        if schema is None:
            raise KeyError(f"Unknown schema: {schema_name!r}")

        result, evidence = evaluate_with_evidence(schema, given, self.schemas)

        qr = QueryResult(
            satisfied=result.satisfied,
            bindings=result.bindings,
            evidence=evidence,
        )

        if evidence is not None:
            self.evidence_base.append(evidence)

        return qr

    def query_schema(
        self,
        schema: SchemaDecl,
        given: dict[str, Any] | None = None,
    ) -> QueryResult:
        """
        Query an inline schema (not necessarily pre-registered).

        The schema is registered as a side-effect so that subsequent
        queries by name work correctly.
        """
        self.load_schema(schema)
        return self.query(schema.name, given)

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def _handle_assert(self, stmt: AssertStmt) -> None:
        """Handle an AssertStmt from a loaded program."""
        if stmt.value is not None:
            # assert x = <literal>
            val = _extract_literal(stmt.value)
            if val is not None:
                self.assert_ground(stmt.name, val)
            # Non-literal expressions (e.g. set literals) are skipped for now —
            # they require the full expression evaluator.
        # assert x (unbound) and assert x ∈ T are not ground facts;
        # they constrain future queries but we don't have a global solver context.


def _extract_literal(expr) -> Any | None:
    """
    Extract a Python value from a simple AST literal node.

    Returns None for non-literal expressions.
    """
    if isinstance(expr, (NatLiteral, IntLiteral)):
        return expr.value
    if isinstance(expr, RealLiteral):
        return expr.value
    if isinstance(expr, StringLiteral):
        return expr.value
    if isinstance(expr, BoolLiteral):
        return expr.value
    return None
