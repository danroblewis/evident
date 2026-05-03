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
    MultiMembershipDecl,
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

        # ── Step 0: pre-register inline enum types so given values resolve ───────
        # Inline enums (x ∈ Red | Green | Blue) are normally registered during
        # instantiate_schema, but given values are converted before that. Scan
        # once so constructors are available when converting given strings.
        from .ast_types import MembershipConstraint, Identifier, InlineEnumExpr
        for item in schema.body:
            if (isinstance(item, MembershipConstraint) and item.op == "∈"
                    and isinstance(item.right, InlineEnumExpr)):
                variants = item.right.variants
                enum_name = "_Enum_" + "_".join(sorted(variants))
                self.registry.declare_algebraic(enum_name, variants)

        # ── Step 1: build the initial environment from 'given' values ─────────
        init_env = Environment(bindings=dict(self.env.bindings))
        for name, py_val in given.items():
            z3_val = self._python_to_z3_untyped(py_val)
            init_env = init_env.bind(name, z3_val)

        # ── Step 2: instantiate the schema ─────────────────────────────────────
        env, type_constraints = instantiate_schema(schema, init_env, self.registry, schemas=self.schemas)

        # ── Step 3: propagate concrete sequence lengths to bound variables ────────
        #
        # When a schema uses ∀ i ∈ {0..n-1}: body, the quantifier bound n-1
        # is symbolic at translation time. Z3's ForAll with IntToStr + Seq
        # indexing in the body exceeds its decidable fragment (returns unknown).
        #
        # Shim: scan the schema body for constraints of the form  n = #seq
        # where seq is already bound to a concrete Z3 sequence in the env.
        # For those, compute Length(seq) statically and bind n to the concrete
        # integer — without calling the solver. The quantifier unroller then
        # sees a concrete bound and unrolls rather than using ForAll.
        from .ast_types import ArithmeticConstraint, CardinalityExpr

        # Pass 1: propagate x = y where one side is a concrete Seq — makes
        # sub-schema fields like nd.contents concrete when linked to a given Seq.
        _orig_bindings = dict(env.bindings)
        changed = True
        while changed:
            changed = False
            for item in schema.body:
                if (isinstance(item, ArithmeticConstraint) and item.op == '='):
                    try:
                        from .translate import translate_expr as _te
                        lhs_z3 = _te(item.left, env, self.registry)
                        rhs_z3 = _te(item.right, env, self.registry)
                        for sym, conc in [(lhs_z3, rhs_z3), (rhs_z3, lhs_z3)]:
                            if (z3.is_seq(sym) and not z3.is_string(sym)
                                    and z3.is_int_value(z3.simplify(z3.Length(conc)))):
                                for ename, evar in list(env.bindings.items()):
                                    try:
                                        if z3.eq(evar, sym) and not z3.eq(evar, conc):
                                            env = env.bind(ename, conc)
                                            changed = True
                                    except Exception:
                                        pass
                    except Exception:
                        pass

        # Pass 2: for each type_constraint, substitute the updated env bindings
        # and simplify. If this yields  sym_int == concrete_int, bind it.
        # No solver call needed — pure symbolic evaluation.
        _subst_pairs = [
            (orig_var, env.lookup(name))
            for name, orig_var in _orig_bindings.items()
            if env.lookup(name) is not None
            and not z3.eq(orig_var, env.lookup(name))
        ]
        if _subst_pairs:
            for tc in type_constraints:
                try:
                    simplified = z3.simplify(z3.substitute(tc, _subst_pairs))
                    if z3.is_eq(simplified) and z3.is_int(simplified.arg(0)):
                        for (sym_side, val_side) in [
                            (simplified.arg(0), simplified.arg(1)),
                            (simplified.arg(1), simplified.arg(0)),
                        ]:
                            if not z3.is_int_value(sym_side) and z3.is_int_value(val_side):
                                for name, orig_var in _orig_bindings.items():
                                    try:
                                        if z3.eq(orig_var, sym_side):
                                            env = env.bind(name, val_side)
                                    except Exception:
                                        pass
                except Exception:
                    pass

        # Pass 3: propagate n = #seq directly (for cases covered by the shim).
        for item in schema.body:
            if (isinstance(item, ArithmeticConstraint) and item.op == '='
                    and isinstance(item.left, Identifier)
                    and isinstance(item.right, CardinalityExpr)
                    and isinstance(item.right.set, Identifier)):
                var_name = item.left.name
                seq_name = item.right.set.name
                var_z3 = env.lookup(var_name)
                seq_z3 = env.lookup(seq_name)
                if (var_z3 is not None and seq_z3 is not None
                        and z3.is_seq(seq_z3) and not z3.is_int_value(var_z3)):
                    length = z3.simplify(z3.Length(seq_z3))
                    if z3.is_int_value(length):
                        env = env.bind(var_name, length)

        # ── Step 4: build and populate the full solver ────────────────────────
        s = z3.Solver()
        for tc in type_constraints:
            s.add(tc)

        for item in schema.body:
            # Skip non-constraint body items
            if isinstance(item, (EvidentBlock, PassthroughItem, MultiMembershipDecl)):
                continue
            try:
                z3_constraint = _translate_body_constraint(item, env, self.registry)
                s.add(z3_constraint)
            except (NotImplementedError, KeyError) as exc:
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
            # unknown — try decomposing free Seq(String) variables into
            # individual String variables and re-solving. Z3 can synthesize
            # individual String vars with IntToStr constraints but not Seq
            # elements. If n is concrete (resolved by the length shim above),
            # we can replace Seq variables with n individual String vars.
            fallback = self._try_seq_decomposition(
                schema, env, type_constraints, given or {}, _orig_bindings
            )
            if fallback is not None:
                return fallback
            return EvaluationResult(
                satisfied=False,
                bindings={},
                model=None,
                explanation="Z3 returned unknown (timeout or resource limit).",
            )

    # ------------------------------------------------------------------
    # Seq(String) decomposition fallback
    # ------------------------------------------------------------------

    def _try_seq_decomposition(self, schema, env, type_constraints, given,
                              orig_bindings=None):
        """
        Fallback for when Z3 returns unknown on a query involving Seq(String)
        synthesis with IntToStr constraints.

        Finds free Seq(String) variables whose length is concrete (resolved by
        the length shim), replaces each with n individual String variables, and
        re-solves. Z3 can synthesize individual String variables with IntToStr
        constraints even though it cannot synthesize Seq elements.

        Returns an EvaluationResult if the decomposed solve succeeds, else None.
        """
        # Find free Seq(String) variables with known concrete length.
        # Two sources of concrete length:
        # (a) z3.simplify(Length(seq)) is already concrete (seq given as list)
        # (b) n = #seq where n is concrete in env (resolved by the length shim)
        seq_vars = {}   # name → (z3_seq_var, concrete_length)

        for name, z3_var in env.bindings.items():
            if z3.is_seq(z3_var) and not z3.is_string(z3_var) and name not in given:
                length_expr = z3.simplify(z3.Length(z3_var))
                if z3.is_int_value(length_expr):
                    seq_vars[name] = (z3_var, length_expr.as_long())

        # Also check n = #seq constraints where n is concrete
        from .ast_types import ArithmeticConstraint, CardinalityExpr, Identifier
        for item in schema.body:
            if (isinstance(item, ArithmeticConstraint) and item.op == '='
                    and isinstance(item.left, Identifier)
                    and isinstance(item.right, CardinalityExpr)
                    and isinstance(item.right.set, Identifier)):
                n_name   = item.left.name
                seq_name = item.right.set.name
                n_val    = env.lookup(n_name)
                seq_z3   = env.lookup(seq_name)
                if (n_val is not None and z3.is_int_value(n_val)
                        and seq_z3 is not None
                        and z3.is_seq(seq_z3) and not z3.is_string(seq_z3)
                        and seq_name not in given
                        and seq_name not in seq_vars):
                    seq_vars[seq_name] = (seq_z3, n_val.as_long())

        # Also find free Seqs whose lengths are determined by type constraints.
        # After the shim, some Int vars are concrete (e.g. nd.n = 3). Substitute
        # those into type_constraints and look for Length(free_seq) == concrete_int.
        if orig_bindings:
            _tc_subst = [
                (orig_bindings[name], env.lookup(name))
                for name in orig_bindings
                if env.lookup(name) is not None
                and not z3.eq(orig_bindings[name], env.lookup(name))
            ]
            if _tc_subst:
                for tc in type_constraints:
                    if z3.is_quantifier(tc):
                        continue
                    try:
                        simplified = z3.simplify(z3.substitute(tc, _tc_subst))
                        if z3.is_eq(simplified) and z3.is_int(simplified.arg(0)):
                            for (int_side, len_side) in [
                                (simplified.arg(0), simplified.arg(1)),
                                (simplified.arg(1), simplified.arg(0)),
                            ]:
                                if (z3.is_int_value(int_side)
                                        and z3.is_app(len_side)
                                        and len_side.num_args() == 1):
                                    seq_arg = len_side.arg(0)
                                    for ename, evar in env.bindings.items():
                                        if (ename not in given
                                                and ename not in seq_vars
                                                and z3.is_seq(evar)
                                                and not z3.is_string(evar)):
                                            try:
                                                if z3.eq(evar, seq_arg):
                                                    seq_vars[ename] = (
                                                        evar, int_side.as_long()
                                                    )
                                            except Exception:
                                                pass
                    except Exception:
                        pass

        if not seq_vars:
            return None

        # Build a new env with the Seq vars replaced by individual String vars
        new_env = env
        elem_vars = {}  # name → list of individual String z3 vars

        for seq_name, (seq_z3, n) in seq_vars.items():
            elem_list = [z3.String(f'{seq_name}_{i}') for i in range(n)]
            elem_vars[seq_name] = elem_list
            # Replace the Seq var in env with a concrete sequence of the elem vars
            seq_val = z3.Unit(elem_list[0])
            for e in elem_list[1:]:
                seq_val = z3.Concat(seq_val, z3.Unit(e))
            new_env = new_env.bind(seq_name, seq_val)

        # Re-solve with the decomposed env.
        # Build a substitution that includes BOTH the shim's Int bindings AND
        # the decomposed Seq replacements, then apply to type_constraints so
        # free Seq vars like nd.lines_orig become Concat(Unit(lines_0),...).
        # Build substitution: orig symbolic vars → updated concrete values
        # (from shim) AND orig Seq vars → decomposed Concat sequences.
        _decomp_subst = []
        if orig_bindings:
            for name, orig_var in orig_bindings.items():
                new_val = new_env.lookup(name)
                if new_val is not None and not z3.eq(orig_var, new_val):
                    _decomp_subst.append((orig_var, new_val))
        # Also substitute original Seq vars → their decomposed forms
        for seq_name, (seq_z3, _) in seq_vars.items():
            decomposed = new_env.lookup(seq_name)
            if decomposed is not None and not z3.eq(seq_z3, decomposed):
                _decomp_subst.append((seq_z3, decomposed))

        s2 = z3.Solver()
        s2.set('timeout', 10000)
        for tc in type_constraints:
            if not z3.is_quantifier(tc):
                subst_tc = z3.substitute(tc, _decomp_subst) if _decomp_subst else tc
                s2.add(z3.simplify(subst_tc))

        # Re-translate sub-schema body constraints with prefix-stripped sub-envs
        # so quantifiers unroll (n is concrete in new_env after shim).
        # Schema main has 'nd.lines', 'nd.n' etc.; NumberedDocument body uses
        # 'lines', 'n' — strip the 'nd.' prefix to build the sub-env.
        from .ast_types import MembershipConstraint as _MC
        for item in schema.body:
            if (isinstance(item, _MC) and item.op == '∈'
                    and isinstance(item.left, Identifier)
                    and isinstance(item.right, Identifier)
                    and item.right.name in self.schemas):
                var_name  = item.left.name      # e.g. 'nd'
                sub_name  = item.right.name     # e.g. 'NumberedDocument'
                prefix    = f'{var_name}.'
                sub_env   = Environment()
                for ename, evar in new_env.bindings.items():
                    if ename.startswith(prefix):
                        sub_env = sub_env.bind(ename[len(prefix):], evar)
                for sub_item in self.schemas[sub_name].body:
                    if isinstance(sub_item, (EvidentBlock, PassthroughItem,
                                             MultiMembershipDecl)):
                        continue
                    try:
                        s2.add(_translate_body_constraint(sub_item, sub_env,
                                                          self.registry))
                    except (NotImplementedError, KeyError):
                        pass
        for item in schema.body:
            if isinstance(item, (EvidentBlock, PassthroughItem, MultiMembershipDecl)):
                continue
            try:
                s2.add(_translate_body_constraint(item, new_env, self.registry))
            except (NotImplementedError, KeyError):
                pass

        if s2.check() != z3.sat:
            return None

        model = s2.model()

        # Reconstruct bindings: include individual element bindings
        bindings = self._extract_model(new_env, model)

        # Also expose the Seq vars as formatted strings and indexed elements
        for seq_name, elem_list in elem_vars.items():
            elements = []
            for i, e in enumerate(elem_list):
                val = model.eval(e, model_completion=True)
                py_val = self._z3_to_python(val)
                bindings[f'{seq_name}.{i}'] = py_val
                elements.append(py_val if py_val is not None else '?')
            bindings[seq_name] = '⟨' + ', '.join(repr(e) for e in elements) + '⟩'

        return EvaluationResult(
            satisfied=True,
            bindings=bindings,
            model=model,
            explanation=None,
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
            # Check enum constructors before falling back to string literal
            ctor = self.registry.get_constructor(value)
            if ctor is not None:
                return ctor
            return z3.StringVal(value)
        if isinstance(value, (list, tuple)):
            # Convert to a Z3 sequence
            if not value:
                return z3.Empty(z3.StringSort() if True else z3.IntSort())
            elements = [self._python_to_z3_untyped(v) for v in value]
            result = z3.Unit(elements[0])
            for e in elements[1:]:
                result = z3.Concat(result, z3.Unit(e))
            return result
        raise ValueError(
            f"Cannot convert {value!r} to a Z3 expression. "
            "Supported types: bool, int, float, str, list."
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
            ctor = self.registry.get_constructor(value)
            if ctor is not None:
                return ctor
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
            # as_decimal() appends '?' when truncated — use exact fraction instead
            try:
                return expr.numerator_as_long() / expr.denominator_as_long()
            except Exception:
                return float(expr.as_decimal(15).rstrip('?'))
        if z3.is_true(expr):
            return True
        if z3.is_false(expr):
            return False
        if isinstance(expr, z3.SeqRef):
            try:
                return expr.as_string()
            except Exception:
                pass
        # Algebraic datatype value (enum variant) — return the constructor name
        if z3.is_app(expr) and expr.num_args() == 0:
            sort = expr.sort()
            if z3.is_sort(sort) and sort.kind() == z3.Z3_DATATYPE_SORT:
                return expr.decl().name()
        # Uninterpreted sort value (e.g. Task!val!0) — not meaningful
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

        For sequence variables (Seq(T)), also emits name.0, name.1, … entries
        so individual elements are available for plotting.  The top-level name
        is set to a formatted ⟨a, b, c⟩ string for display.

        For string variables, also emits name.length as a numeric binding.
        """
        result: dict[str, Any] = {}
        for name, z3_expr in env.bindings.items():
            if name.startswith("__") or name.startswith("."):
                continue
            try:
                val = model.eval(z3_expr, model_completion=True)

                # ── Non-string sequences: expand into indexed sub-bindings ──
                if z3.is_seq(val) and not z3.is_string(val):
                    length_val = model.eval(z3.Length(z3_expr), model_completion=True)
                    if z3.is_int_value(length_val):
                        n = length_val.as_long()
                        elements = []
                        for i in range(min(n, 50)):
                            elem = model.eval(z3_expr[i], model_completion=True)
                            py_elem = self._z3_to_python(elem)
                            elements.append(py_elem)
                            result[f"{name}.{i}"] = py_elem
                        result[name] = "⟨" + ", ".join(
                            str(e) if e is not None else "?" for e in elements
                        ) + "⟩"
                    else:
                        result[name] = self._z3_to_python(val)
                    continue

                py_val = self._z3_to_python(val)
                result[name] = py_val

                # ── Strings: add a length sub-binding for plotting ──────────
                if isinstance(py_val, str) and z3.is_string_value(val):
                    result[f"{name}.length"] = len(py_val)

            except Exception:
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
