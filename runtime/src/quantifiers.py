"""
Phase 6: Quantifier translation and cardinality constraints.

Translates UniversalConstraint, ExistentialConstraint, and CardinalityConstraint
AST nodes to Z3 boolean expressions.

Strategy
--------
- For finite/enumerated domains (SetLiteral, EmptySet, RangeLiteral), unroll
  quantifiers into conjunctions or disjunctions — no Z3 quantifiers needed.
- For symbolic set expressions, use Z3 ForAll / Exists with array select.
- Cardinality uses Z3 pseudo-boolean: PbLe, PbGe, PbEq.
"""

from __future__ import annotations

import z3


class _SymbolicRange(Exception):
    """Raised when a RangeLiteral has symbolic bounds that can't be unrolled."""
    def __init__(self, lo, hi):
        self.lo = lo
        self.hi = hi

from .env import Environment
from .sorts import SortRegistry
from .ast_types import (
    UniversalConstraint,
    ExistentialConstraint,
    CardinalityConstraint,
    Binding,
    SetLiteral,
    EmptySet as EmptySetNode,
    RangeLiteral,
    NatLiteral,
    IntLiteral,
    Identifier,
)


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------

def _is_concrete_set(set_expr) -> bool:
    """Return True when the set expression enumerates concrete elements."""
    return isinstance(set_expr, (SetLiteral, EmptySetNode, RangeLiteral))


def _enumerate_elements(set_expr, env: Environment, registry: SortRegistry) -> list[z3.ExprRef]:
    """
    Return the concrete Z3 element list for a finite set expression.

    Supports SetLiteral, EmptySet, and RangeLiteral (integer range).
    """
    from .translate import translate_expr  # local import to avoid circularity

    if isinstance(set_expr, EmptySetNode):
        return []

    if isinstance(set_expr, SetLiteral):
        return [translate_expr(e, env, registry) for e in set_expr.elements]

    if isinstance(set_expr, RangeLiteral):
        from_val = translate_expr(set_expr.from_, env, registry)
        to_val   = translate_expr(set_expr.to,   env, registry)

        # Try to simplify — if the given data makes the bounds concrete
        # (e.g. {0..#contents-1} when contents is a given sequence),
        # unroll into individual values instead of using a Z3 ForAll.
        from_s = z3.simplify(from_val)
        to_s   = z3.simplify(to_val)

        if z3.is_int_value(from_s) and z3.is_int_value(to_s):
            lo = from_s.as_long()
            hi = to_s.as_long()
            return [z3.IntVal(i) for i in range(lo, hi + 1)]

        # Truly symbolic bounds: fall back to ForAll with arithmetic guard.
        raise _SymbolicRange(from_val, to_val)

    raise NotImplementedError(
        f"Cannot enumerate elements of set expression {type(set_expr).__name__!r}."
    )


def _translate_body_for_element(
    elem: z3.ExprRef,
    name: str,
    body,
    env: Environment,
    registry: SortRegistry,
) -> z3.BoolRef:
    """Translate the quantifier body with `name` bound to `elem`."""
    from .translate import translate_constraint  # local import to avoid circularity
    inner_env = env.bind(name, elem)
    return translate_constraint(body, inner_env, registry)


# ---------------------------------------------------------------------------
# Public API: quantifier translation
# ---------------------------------------------------------------------------

def translate_universal(
    node: UniversalConstraint,
    env: Environment,
    registry: SortRegistry,
) -> z3.BoolRef:
    """
    ∀ x ∈ S : P(x)

    For finite/enumerated domains: unroll to a conjunction.
    For symbolic set domains: use Z3 ForAll with array select.

    Multiple bindings (∀ x, y ∈ S) are handled as nested universals —
    the body is evaluated for every (element, name) pair independently,
    which matches the Evident semantics of binding each name to the same
    set and requiring the body to hold for all combinations.
    """
    # Process bindings one at a time; for multiple names in one binding
    # we treat each name independently over the same set.
    assertions: list[z3.BoolRef] = []

    def _universal_for_binding(binding: Binding, cur_env: Environment) -> list[z3.BoolRef]:
        """Return Z3 assertions for a single binding clause."""
        set_expr = binding.set
        parts: list[z3.BoolRef] = []

        # SetComprehension: ∀ (a, b) ∈ {output(i) | i ∈ range}: body
        # Unroll by enumerating the generator's domain and substituting.
        from .ast_types import SetComprehension, TupleLiteral as TL
        if isinstance(set_expr, SetComprehension):
            gens = set_expr.generators
            if (len(gens) >= 1 and gens[0].binding is not None
                    and len(gens[0].binding.names) == 1):
                gen_name = gens[0].binding.names[0]
                gen_range = gens[0].binding.set
                if _is_concrete_set(gen_range):
                    try:
                        gen_elements = _enumerate_elements(gen_range, cur_env, registry)
                    except (_SymbolicRange, NotImplementedError, KeyError):
                        gen_elements = None
                    if gen_elements is not None:
                        output = set_expr.output
                        from .translate import translate_expr, translate_constraint
                        for gen_val in gen_elements:
                            gen_env = cur_env.bind(gen_name, gen_val)
                            if (isinstance(output, TL)
                                    and len(output.elements) == len(binding.names)):
                                # Tuple output: bind each name to its element
                                bound_env = gen_env
                                for bname, elem_expr in zip(binding.names, output.elements):
                                    try:
                                        elem_val = translate_expr(elem_expr, gen_env, registry)
                                        bound_env = bound_env.bind(bname, elem_val)
                                    except (NotImplementedError, KeyError):
                                        bound_env = None
                                        break
                                if bound_env is not None:
                                    try:
                                        parts.append(
                                            translate_constraint(node.body, bound_env, registry)
                                        )
                                    except (NotImplementedError, KeyError):
                                        pass
                            elif len(binding.names) == 1:
                                try:
                                    val = translate_expr(output, gen_env, registry)
                                    bound_env = gen_env.bind(binding.names[0], val)
                                    parts.append(
                                        translate_constraint(node.body, bound_env, registry)
                                    )
                                except (NotImplementedError, KeyError):
                                    pass
                        if parts:
                            return parts

        if _is_concrete_set(set_expr):
            try:
                elements = _enumerate_elements(set_expr, cur_env, registry)
            except _SymbolicRange as sr:
                # Symbolic integer range — use ForAll with arithmetic guard
                from .translate import translate_constraint
                for name in binding.names:
                    i_var = z3.FreshInt(name)
                    body_env = cur_env.bind(name, i_var)
                    body_z3 = translate_constraint(node.body, body_env, registry)
                    parts.append(
                        z3.ForAll(
                            [i_var],
                            z3.Implies(
                                z3.And(sr.lo <= i_var, i_var <= sr.hi),
                                body_z3,
                            ),
                        )
                    )
                return parts

            if not elements:
                # Vacuously true — return True
                return [z3.BoolVal(True)]

            # Each name in this binding gets the same element list.
            # For a single binding with multiple names, unroll all combinations.
            from itertools import product as iproduct
            name_element_combos = iproduct(*[elements for _ in binding.names])
            for combo in name_element_combos:
                bound_env = cur_env
                for name, elem in zip(binding.names, combo):
                    bound_env = bound_env.bind(name, elem)
                from .translate import translate_constraint
                parts.append(translate_constraint(node.body, bound_env, registry))
        else:
            # Symbolic set — use Z3 ForAll with Implies(Select(S, x), P(x)).
            from .translate import translate_expr, translate_constraint
            s = translate_expr(set_expr, cur_env, registry)
            # Infer element sort from the array domain.
            elem_sort = s.sort().domain()
            for name in binding.names:
                x_var = z3.FreshConst(elem_sort, name)
                body_env = cur_env.bind(name, x_var)
                body_z3 = translate_constraint(node.body, body_env, registry)
                parts.append(
                    z3.ForAll(
                        [x_var],
                        z3.Implies(z3.Select(s, x_var), body_z3),
                    )
                )
        return parts

    for binding in node.bindings:
        assertions.extend(_universal_for_binding(binding, env))

    if not assertions:
        return z3.BoolVal(True)
    return z3.And(*assertions) if len(assertions) > 1 else assertions[0]


def translate_existential(
    node: ExistentialConstraint,
    env: Environment,
    registry: SortRegistry,
) -> z3.BoolRef:
    """
    ∃  x ∈ S : P(x)  → Exists([x], And(Select(S, x), P(x)))
    ∃! x ∈ S : P(x)  → exactly one element of S satisfies P
    ¬∃ x ∈ S : P(x)  → Not(Exists([x], And(Select(S, x), P(x))))

    For finite/enumerated domains: unroll to disjunctions / cardinality.
    For symbolic domains: use Z3 Exists.
    """
    quantifier = node.quantifier  # "∃", "∃!", or "¬∃"

    # Collect all (name, set_expr) pairs from bindings.
    # For simplicity we require a single binding clause here.
    # (Multiple clauses would be handled as nested existentials.)
    if len(node.bindings) != 1:
        raise NotImplementedError(
            "translate_existential currently supports exactly one binding clause. "
            f"Got {len(node.bindings)} bindings."
        )
    binding = node.bindings[0]
    set_expr = binding.set
    names = binding.names

    from .translate import translate_expr, translate_constraint

    if _is_concrete_set(set_expr):
        elements = _enumerate_elements(set_expr, env, registry)

        if quantifier == "∃":
            if not elements:
                return z3.BoolVal(False)
            from itertools import product as iproduct
            name_element_combos = list(iproduct(*[elements for _ in names]))
            disjuncts: list[z3.BoolRef] = []
            for combo in name_element_combos:
                bound_env = env
                for name, elem in zip(names, combo):
                    bound_env = bound_env.bind(name, elem)
                disjuncts.append(translate_constraint(node.body, bound_env, registry))
            return z3.Or(*disjuncts) if len(disjuncts) > 1 else disjuncts[0]

        if quantifier == "¬∃":
            if not elements:
                return z3.BoolVal(True)
            from itertools import product as iproduct
            name_element_combos = list(iproduct(*[elements for _ in names]))
            disjuncts = []
            for combo in name_element_combos:
                bound_env = env
                for name, elem in zip(names, combo):
                    bound_env = bound_env.bind(name, elem)
                disjuncts.append(translate_constraint(node.body, bound_env, registry))
            exists_z3 = z3.Or(*disjuncts) if len(disjuncts) > 1 else disjuncts[0]
            return z3.Not(exists_z3)

        if quantifier == "∃!":
            # Exactly one element satisfies P — use PbEq over satisfying indicator bools.
            if not elements:
                return z3.BoolVal(False)
            # We need a single name for ∃! semantics; multi-name is unusual.
            if len(names) != 1:
                raise NotImplementedError("∃! with multiple names in one binding is not supported.")
            name = names[0]
            indicators: list[z3.BoolRef] = []
            for elem in elements:
                bound_env = env.bind(name, elem)
                indicators.append(translate_constraint(node.body, bound_env, registry))
            return exactly_n(1, indicators)

        raise NotImplementedError(f"Unknown existential quantifier: {quantifier!r}")

    else:
        # Symbolic set path — use Z3 Exists.
        s = translate_expr(set_expr, env, registry)
        elem_sort = s.sort().domain()

        if len(names) != 1:
            raise NotImplementedError(
                "Symbolic-set existential with multiple names is not yet supported."
            )
        name = names[0]
        x_var = z3.FreshConst(elem_sort, name)
        body_env = env.bind(name, x_var)
        body_z3 = translate_constraint(node.body, body_env, registry)
        membership = z3.Select(s, x_var)
        core = z3.Exists([x_var], z3.And(membership, body_z3))

        if quantifier == "∃":
            return core
        if quantifier == "¬∃":
            return z3.Not(core)
        if quantifier == "∃!":
            # unique: ∃x. S[x] ∧ P(x) ∧ ∀y. (S[y] ∧ P(y)) ⇒ y = x
            y_var = z3.FreshConst(elem_sort, name + "_uniq")
            body_y_env = env.bind(name, y_var)
            body_y_z3 = translate_constraint(node.body, body_y_env, registry)
            uniqueness = z3.ForAll(
                [y_var],
                z3.Implies(
                    z3.And(z3.Select(s, y_var), body_y_z3),
                    y_var == x_var,
                ),
            )
            return z3.Exists([x_var], z3.And(membership, body_z3, uniqueness))

        raise NotImplementedError(f"Unknown existential quantifier: {quantifier!r}")


# ---------------------------------------------------------------------------
# Cardinality helpers
# ---------------------------------------------------------------------------

def _make_terms(
    elements: list[z3.ExprRef],
    predicate: z3.BoolRef | None,
) -> list[tuple[z3.BoolRef, int]]:
    """
    Build the (indicator, weight) list for PbLe/PbGe/PbEq.

    If predicate is None every element counts as 1.
    If predicate is a single BoolRef it is used for every element
    (the simplified form from the stub).  In practice callers should
    pass a list of per-element booleans via the helper functions below.
    """
    if predicate is None:
        return [(z3.BoolVal(True), 1) for _ in elements]
    # predicate is a single formula — repeat for each element.
    return [(predicate, 1) for _ in elements]


def at_most_n(
    n: int,
    elements: list[z3.ExprRef],
    predicate: z3.BoolRef | None = None,
) -> z3.BoolRef:
    """
    At most n of *elements* satisfy *predicate*.

    When predicate is None every element is counted.
    *elements* may be a list of boolean indicator expressions; in that
    case pass predicate=None and let each item serve as its own indicator.
    """
    if not elements:
        # at_most n of [] — trivially true (0 ≤ n for n ≥ 0).
        return z3.BoolVal(True)

    # If the elements are boolean sorts, treat each as its own indicator.
    if elements and elements[0].sort() == z3.BoolSort():
        terms = [(e, 1) for e in elements]
    else:
        terms = _make_terms(elements, predicate)

    return z3.PbLe(terms, n)


def at_least_n(
    n: int,
    elements: list[z3.ExprRef],
    predicate: z3.BoolRef | None = None,
) -> z3.BoolRef:
    """At least n of *elements* satisfy *predicate* (uses PbGe)."""
    if not elements:
        # at_least n of [] — only true when n == 0.
        return z3.BoolVal(n == 0)

    if elements and elements[0].sort() == z3.BoolSort():
        terms = [(e, 1) for e in elements]
    else:
        terms = _make_terms(elements, predicate)

    return z3.PbGe(terms, n)


def exactly_n(
    n: int,
    elements: list[z3.ExprRef],
    predicate: z3.BoolRef | None = None,
) -> z3.BoolRef:
    """Exactly n of *elements* satisfy *predicate* (uses PbEq)."""
    if not elements:
        return z3.BoolVal(n == 0)

    if elements and elements[0].sort() == z3.BoolSort():
        terms = [(e, 1) for e in elements]
    else:
        terms = _make_terms(elements, predicate)

    return z3.PbEq(terms, n)


def all_different(elements: list[z3.ExprRef]) -> z3.BoolRef:
    """All elements are mutually distinct: z3.Distinct(...)."""
    if len(elements) <= 1:
        return z3.BoolVal(True)
    return z3.Distinct(*elements)


# ---------------------------------------------------------------------------
# CardinalityConstraint translation
# ---------------------------------------------------------------------------

def translate_cardinality_constraint(
    node: CardinalityConstraint,
    env: Environment,
    registry: SortRegistry,
) -> z3.BoolRef:
    """
    Translate at_most/at_least/exactly constraints.

    node.set must be a concrete finite set expression (SetLiteral, EmptySet,
    RangeLiteral).  node.count must be a concrete integer literal.
    """
    from .translate import translate_expr

    # Resolve the count — must be a concrete integer.
    count_z3 = translate_expr(node.count, env, registry)
    if not z3.is_int_value(count_z3):
        raise ValueError(
            f"CardinalityConstraint count must be a concrete integer literal. "
            f"Got: {count_z3!r}"
        )
    n = count_z3.as_long()

    # Enumerate the concrete elements.
    if not _is_concrete_set(node.set):
        raise NotImplementedError(
            "translate_cardinality_constraint requires a concrete finite set "
            f"(SetLiteral / EmptySet / RangeLiteral). Got: {type(node.set).__name__!r}"
        )
    elements = _enumerate_elements(node.set, env, registry)

    op = node.op
    if op == "at_most":
        return at_most_n(n, elements)
    if op == "at_least":
        return at_least_n(n, elements)
    if op == "exactly":
        return exactly_n(n, elements)

    raise NotImplementedError(f"Unknown CardinalityConstraint op: {op!r}")
