"""
Phase 9: Forward implications using Z3's Fixedpoint (Datalog/Spacer) engine.

Z3's Fixedpoint engine supports two backends:
- "datalog": efficient bottom-up evaluation; requires finite/bitvector sorts.
- "spacer": PDR-based; supports infinite sorts (Int, Real, String).

This module uses "spacer" so that Evident's natural Int/String sorts work
without conversion.  For purely finite enumerated domains the caller can
switch to "datalog" via the engine parameter.
"""

from __future__ import annotations

import z3

from .sorts import SortRegistry


def _is_rule_variable(expr: z3.ExprRef) -> bool:
    """
    Return True if *expr* is an uninterpreted constant (a 'logic variable'
    in Datalog terminology) rather than a ground term.

    Z3 represents both Int('x') and IntVal(5) as constants, but only the
    former has kind Z3_OP_UNINTERPRETED.
    """
    return (
        z3.is_const(expr)
        and expr.num_args() == 0
        and expr.decl().kind() == z3.Z3_OP_UNINTERPRETED
    )


class FixedpointSolver:
    """
    Thin wrapper around z3.Fixedpoint that provides a Datalog-friendly API
    for Evident's forward-implication rules.

    Usage pattern::

        fs = FixedpointSolver(registry)
        Int = IntSort()
        fs.declare_relation("node",     [Int])
        fs.declare_relation("adjacent", [Int, Int])
        fs.declare_relation("reachable",[Int, Int])

        fs.add_fact("node",     [IntVal(1)])
        fs.add_fact("adjacent", [IntVal(1), IntVal(2)])

        n = Int("n"); a = Int("a"); b = Int("b"); c = Int("c")
        fs.add_rule("reachable", [n, n], [("node", [n])])
        fs.add_rule("reachable", [a, c], [("reachable", [a, b]),
                                          ("adjacent", [b, c])])

        assert fs.query("reachable", [IntVal(1), IntVal(2)])
    """

    def __init__(
        self,
        registry: SortRegistry,
        engine: str = "spacer",
    ):
        self.fp = z3.Fixedpoint()
        self.registry = registry
        self.relations: dict[str, z3.FuncDeclRef] = {}
        # Track all rule variables seen so we only declare_var once each.
        self._declared_vars: set[int] = set()  # keyed by ast id
        self.fp.set("engine", engine)

    # ------------------------------------------------------------------
    # Relation management
    # ------------------------------------------------------------------

    def declare_relation(
        self,
        name: str,
        arg_sorts: list[z3.SortRef],
    ) -> z3.FuncDeclRef:
        """Declare a Datalog relation (predicate) and register it."""
        rel = z3.Function(name, *arg_sorts, z3.BoolSort())
        self.fp.register_relation(rel)
        self.relations[name] = rel
        return rel

    # ------------------------------------------------------------------
    # Facts and rules
    # ------------------------------------------------------------------

    def _declare_rule_vars(self, *arg_lists: list[z3.ExprRef]) -> None:
        """
        Call fp.declare_var() on every uninterpreted constant in the given
        argument lists.  Idempotent — each variable is declared at most once.
        """
        for args in arg_lists:
            for expr in args:
                if _is_rule_variable(expr):
                    expr_id = expr.get_id()
                    if expr_id not in self._declared_vars:
                        self.fp.declare_var(expr)
                        self._declared_vars.add(expr_id)

    def add_fact(self, relation_name: str, args: list[z3.ExprRef]) -> None:
        """Add a ground fact: relation(arg1, arg2, ...)"""
        rel = self.relations[relation_name]
        self.fp.add_rule(rel(*args))

    def add_rule(
        self,
        head_name: str,
        head_args: list[z3.ExprRef],
        body: list[tuple[str, list[z3.ExprRef]]],
    ) -> None:
        """
        Add a Datalog rule::

            head_name(head_args) :- body[0][0](body[0][1]), ...

        *body* is a list of ``(relation_name, args)`` pairs.
        Logic-variable args should be Z3 constants created with e.g.
        ``Int("x")``; ground args should be concrete values like
        ``IntVal(3)``.

        The method automatically calls ``fp.declare_var`` for every
        uninterpreted constant it encounters.
        """
        # Collect all arg lists so we can declare variables in one pass.
        all_arg_lists = [head_args] + [args for _, args in body]
        self._declare_rule_vars(*all_arg_lists)

        head_rel = self.relations[head_name]
        head_atom = head_rel(*head_args)

        if body:
            body_atoms = [self.relations[name](*args) for name, args in body]
            premise = z3.And(*body_atoms) if len(body_atoms) > 1 else body_atoms[0]
            self.fp.rule(head_atom, premise)
        else:
            self.fp.rule(head_atom)

    # ------------------------------------------------------------------
    # Queries
    # ------------------------------------------------------------------

    def query(self, relation_name: str, args: list[z3.ExprRef]) -> bool:
        """
        Query whether *relation_name*(*args*) is derivable.

        All *args* must be ground terms (no logic variables).  Returns
        ``True`` iff the atom is derivable from the current rules and facts.
        """
        rel = self.relations[relation_name]
        result = self.fp.query(rel(*args))
        return result == z3.sat

    # ------------------------------------------------------------------
    # AST integration
    # ------------------------------------------------------------------

    def translate_forward_rule(
        self,
        rule: "ForwardRule",  # noqa: F821  (imported lazily to avoid circular)
        var_sorts: dict[str, z3.SortRef],
    ) -> None:
        """
        Translate an Evident ``ForwardRule`` AST node into Datalog rules.

        ``ForwardRule`` has:
        - ``premises``: ``list[ApplicationConstraint]``
        - ``conclusion``: ``ApplicationConstraint``

        Each ``ApplicationConstraint`` has a ``name`` (relation name) and
        ``args`` (list of ``Identifier`` or literal AST nodes).

        ``var_sorts`` maps variable name → Z3 sort for every logic variable
        that appears in the rule.  Variables not in ``var_sorts`` are treated
        as ground constants (not currently supported — callers should supply
        sorts for all variables).
        """
        from .ast_types import Identifier  # local import to avoid cycles

        # Build a Z3 constant for each named variable.
        z3_vars: dict[str, z3.ExprRef] = {}
        for vname, vsort in var_sorts.items():
            z3_vars[vname] = z3.Const(vname, vsort)

        def _translate_args(
            constraint_args: list,
        ) -> list[z3.ExprRef]:
            result = []
            for arg in constraint_args:
                if isinstance(arg, Identifier):
                    if arg.name in z3_vars:
                        result.append(z3_vars[arg.name])
                    else:
                        raise ValueError(
                            f"Variable {arg.name!r} not in var_sorts."
                        )
                else:
                    raise NotImplementedError(
                        f"translate_forward_rule: unsupported arg type {type(arg)}"
                    )
            return result

        conclusion = rule.conclusion
        head_args = _translate_args(conclusion.args)

        body = [
            (premise.name, _translate_args(premise.args))
            for premise in rule.premises
        ]

        self.add_rule(conclusion.name, head_args, body)
