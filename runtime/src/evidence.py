"""
Phase 10: Evidence terms — structured derivation trees.

When the solver finds a satisfying assignment, Evident produces an "evidence term":
a structured proof that shows HOW each claim was established, not just THAT it was.
This is first-class data.
"""

from __future__ import annotations
from dataclasses import dataclass, field
from typing import Any
import json


# ---------------------------------------------------------------------------
# Evidence node
# ---------------------------------------------------------------------------


@dataclass
class Evidence:
    """
    A derivation tree node. Represents that a claim was established,
    how (which sub-claims were used), and with what variable values.
    """
    claim: str                          # name of the established claim/schema
    bindings: dict[str, Any]           # variable assignments (name → Python value)
    sub_evidence: list[Evidence] = field(default_factory=list)
    rule_used: str | None = None       # which evident block / rule fired

    def to_dict(self) -> dict:
        """Serialize to a JSON-compatible dict."""
        return {
            "claim": self.claim,
            "bindings": self.bindings,
            "rule_used": self.rule_used,
            "sub_evidence": [e.to_dict() for e in self.sub_evidence],
        }

    def to_json(self, indent=2) -> str:
        """Serialize to a JSON string."""
        return json.dumps(self.to_dict(), indent=indent, default=str)

    def get(self, var_name: str) -> Any:
        """Get the value of a variable from this evidence node."""
        return self.bindings.get(var_name)

    def find_sub(self, claim_name: str) -> "Evidence | None":
        """Find a sub-evidence node by claim name (first match)."""
        for sub in self.sub_evidence:
            if sub.claim == claim_name:
                return sub
        return None

    def __repr__(self):
        bindings_str = ", ".join(f"{k}={v!r}" for k, v in self.bindings.items())
        return f"Evidence({self.claim!r}, {{{bindings_str}}})"


# ---------------------------------------------------------------------------
# Z3 → Python conversion (self-contained, independent of evaluate.py)
# ---------------------------------------------------------------------------


def _z3_to_python(expr) -> Any:
    """Convert a Z3 expr to a Python value."""
    from z3 import is_int_value, is_rational_value, is_true, is_false, SeqRef
    if expr is None:
        return None
    if is_int_value(expr):
        return expr.as_long()
    if is_rational_value(expr):
        return float(expr.as_decimal(10))
    if is_true(expr):
        return True
    if is_false(expr):
        return False
    if isinstance(expr, SeqRef):
        try:
            return expr.as_string()
        except Exception:
            pass
    return str(expr)


# ---------------------------------------------------------------------------
# Evidence builder
# ---------------------------------------------------------------------------


def build_evidence(
    claim_name: str,
    bindings: dict[str, Any],
    body_items: list,
    env: "Environment",
    model: "z3.ModelRef",
    sub_schemas: dict[str, "SchemaDecl"] | None = None,
) -> Evidence:
    """
    Build an Evidence tree for a satisfied claim.

    Walk the body items:
    - For ApplicationConstraint sub-claim references: recursively build
      sub-evidence if we have the schema registered.
    - Other constraints contribute to bindings but don't produce sub-nodes.

    Returns the root Evidence node.
    """
    from .ast_types import ApplicationConstraint

    sub_evidence: list[Evidence] = []

    if sub_schemas:
        for item in body_items:
            if (
                isinstance(item, ApplicationConstraint)
                and item.name in sub_schemas
            ):
                sub_schema = sub_schemas[item.name]
                # Extract bindings for the sub-schema's variables from the model
                sub_bindings: dict[str, Any] = {}
                for param in sub_schema.params:
                    for vname in param.names:
                        z3_expr = env.lookup(vname)
                        if z3_expr is not None:
                            val = model.eval(z3_expr, model_completion=True)
                            sub_bindings[vname] = _z3_to_python(val)
                sub_evidence.append(
                    Evidence(
                        claim=item.name,
                        bindings=sub_bindings,
                    )
                )

    return Evidence(
        claim=claim_name,
        bindings=bindings,
        sub_evidence=sub_evidence,
    )


# ---------------------------------------------------------------------------
# Convenience wrapper: evaluate + build evidence in one call
# ---------------------------------------------------------------------------


def evaluate_with_evidence(
    schema: "SchemaDecl",
    given: dict[str, Any] | None = None,
    sub_schemas: dict[str, "SchemaDecl"] | None = None,
    registry=None,
) -> tuple["EvaluationResult", Evidence | None]:
    """
    Evaluate a schema and build an evidence tree if satisfied.

    Parameters
    ----------
    schema:
        The SchemaDecl to evaluate.
    given:
        Optional pre-bound variable assignments (name → Python value).
    sub_schemas:
        Optional dict of named SchemaDecl objects that may be referenced
        as sub-claims inside *schema*'s body.  Evidence nodes will be
        created for each ApplicationConstraint whose name appears here.

    Returns
    -------
    (result, evidence)
        *result* is the EvaluationResult from the solver.
        *evidence* is an Evidence tree when sat, None when unsat.
    """
    from .evaluate import EvidentSolver, EvaluationResult

    solver = EvidentSolver()
    if registry is not None:
        solver.registry = registry
    if sub_schemas:
        for name, s in sub_schemas.items():
            solver.register_schema(s)

    result = solver.evaluate(schema, given or {})

    if not result.satisfied:
        return result, None

    # Re-run instantiate_schema to recover the final environment so we can
    # look up variable → Z3 expr mappings for sub-schema bindings.
    from .instantiate import instantiate_schema

    init_env = solver.env
    for name, py_val in (given or {}).items():
        z3_val = solver._python_to_z3_untyped(py_val)
        init_env = init_env.bind(name, z3_val)

    env, _ = instantiate_schema(schema, init_env, solver.registry)

    evidence = build_evidence(
        schema.name,
        result.bindings,
        schema.body,
        env,
        result.model,
        sub_schemas or {},
    )
    return result, evidence
