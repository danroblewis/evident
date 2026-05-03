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
    EnumDecl,
    ImportStmt,
    AssertStmt,
    ForwardRule,
    QueryStmt,
    ConstraintStmt,
    NotationDecl,
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
        self.pending_queries: list = []       # ? statements collected during load
        self._loaded_files: set = set()       # resolved paths already loaded

    # ------------------------------------------------------------------
    # Schema registration
    # ------------------------------------------------------------------

    def load_schema(self, schema: SchemaDecl) -> None:
        """Register a schema definition."""
        from .ast_types import MembershipConstraint, InlineEnumExpr
        # Pre-register inline enum types so conflicts are caught at load time.
        for item in schema.body:
            if (isinstance(item, MembershipConstraint) and item.op == "∈"
                    and isinstance(item.right, InlineEnumExpr)):
                variants = item.right.variants
                enum_name = "_Enum_" + "_".join(sorted(variants))
                self.solver.registry.declare_algebraic(enum_name, variants)
        self.schemas[schema.name] = schema
        self.solver.register_schema(schema)

    # ------------------------------------------------------------------
    # Program loading
    # ------------------------------------------------------------------

    def load_source(self, source: str, base_dir=None) -> None:
        """Parse Evident source text and load it into the runtime."""
        import sys
        from pathlib import Path
        sys.path.insert(0, str(Path(__file__).parent.parent.parent))
        from parser.src.parser import parse
        self.load_program(parse(source), base_dir=base_dir)

    def load_file(self, path) -> None:
        """Load an Evident source file, resolving imports relative to its directory."""
        from pathlib import Path
        p = Path(path).resolve()
        if p in self._loaded_files:
            return   # already loaded — skip (cycle / duplicate guard)
        self._loaded_files.add(p)
        self.load_source(p.read_text(), base_dir=p.parent)

    def load_program(self, program: Program, base_dir=None) -> None:
        """Load all statements from a parsed Program AST."""
        from pathlib import Path
        for stmt in program.statements:
            if isinstance(stmt, ImportStmt):
                if base_dir is None:
                    base_dir = Path.cwd()
                candidate = Path(base_dir) / stmt.path
                if not candidate.exists():
                    # Fall back to cwd-relative (lets you write import "ide/examples/beavers.ev")
                    candidate = Path.cwd() / stmt.path
                self.load_file(candidate)
            elif isinstance(stmt, EnumDecl):
                self.solver.registry.declare_algebraic(stmt.name, stmt.variants)
            elif isinstance(stmt, NotationDecl):
                self.solver.registry.register_notation(stmt)
            elif isinstance(stmt, SchemaDecl):
                self.load_schema(_expand_schema_notations(stmt, self.solver.registry.get_notations()))
            elif isinstance(stmt, AssertStmt):
                self._handle_assert(stmt)
            elif isinstance(stmt, QueryStmt):
                self.pending_queries.append(stmt)
            elif isinstance(stmt, ForwardRule):
                self.forward_rules.append(stmt)
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

        result, evidence = evaluate_with_evidence(
            schema, given, self.schemas, registry=self.solver.registry
        )

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
    # Automaton execution
    # ------------------------------------------------------------------

    def execute(self, schema_name: str = 'main',
                input_stream=None, output_stream=None) -> None:
        """
        Run schema_name as a constraint automaton.
        Reads from input_stream (default: sys.stdin),
        writes to output_stream (default: sys.stdout).

        Requires stdlib/io.ev to already be loaded (or the relevant
        Stdin/Stdout schemas to be defined in the program).
        """
        from .executor import EvidentExecutor
        executor = EvidentExecutor.__new__(EvidentExecutor)
        executor.rt = self
        executor.run(input_stream=input_stream, output_stream=output_stream)

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def _handle_assert(self, stmt: AssertStmt) -> None:
        """Handle an AssertStmt from a loaded program."""
        if stmt.value is not None:
            from .ast_types import SetLiteral, RangeLiteral, BinaryExpr
            # assert months = { ... }  — named set, stored for ∈ resolution
            if isinstance(stmt.value, (SetLiteral, RangeLiteral, BinaryExpr)):
                self.solver.registry.register_named_set(stmt.name, stmt.value)
                return
            # assert x = <literal>
            val = _extract_literal(stmt.value)
            if val is not None:
                self.assert_ground(stmt.name, val)


def _expand_schema_notations(schema: SchemaDecl, notations: dict) -> SchemaDecl:
    """Expand notation applications in every body constraint of a schema."""
    if not notations:
        return schema
    from parser.src.notation import expand_notation_constraint
    new_body = []
    for item in schema.body:
        try:
            new_body.append(expand_notation_constraint(item, notations))
        except Exception:
            new_body.append(item)
    return SchemaDecl(
        keyword=schema.keyword,
        name=schema.name,
        params=schema.params,
        body=new_body,
    )


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
