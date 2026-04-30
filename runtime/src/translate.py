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
    InlineEnumExpr,
    SetLiteral,
    RangeLiteral,
    # Expressions
    Identifier,
    FieldAccess,
    FilterExpr,
    TupleLiteral,
    BinaryExpr,
    UnaryExpr,
    CardinalityExpr,
    NatLiteral,
    IntLiteral,
    RealLiteral,
    StringLiteral,
    BoolLiteral,
    RegexLiteral,
    SeqLiteral,
)


def _build_z3_regex(pattern: str):
    """Translate a regex pattern string into a Z3 regex expression.

    Supports: literals, . * + ? | [] [^] \\d \\w \\s \\D \\W \\S
    This is a simplified subset — not full PCRE.
    """
    import re as _re

    pos = 0

    def peek():
        return pattern[pos] if pos < len(pattern) else None

    def consume():
        nonlocal pos
        ch = pattern[pos]; pos += 1; return ch

    def parse_char_class():
        """Parse [...] or [^...] into a Z3 union of ranges/chars."""
        nonlocal pos
        negate = False
        if peek() == '^':
            negate = True; consume()
        parts = []
        while peek() and peek() != ']':
            ch = consume()
            if ch == '\\':
                esc = consume()
                parts.append(_esc_to_z3(esc))
            elif peek() == '-' and pos + 1 < len(pattern) and pattern[pos+1] != ']':
                consume()  # consume '-'
                end = consume()
                parts.append(z3.Range(ch, end))
            else:
                parts.append(z3.Re(z3.StringVal(ch)))
        if peek() == ']':
            consume()
        if not parts:
            return z3.Re(z3.StringVal(''))
        result = parts[0] if len(parts) == 1 else z3.Union(*parts)
        return z3.Complement(result) if negate else result

    def _esc_to_z3(ch):
        if ch == 'd': return z3.Range('0', '9')
        if ch == 'D': return z3.Complement(z3.Range('0', '9'))
        if ch == 'w': return z3.Union(z3.Range('a','z'), z3.Range('A','Z'), z3.Range('0','9'), z3.Re(z3.StringVal('_')))
        if ch == 'W': return z3.Complement(z3.Union(z3.Range('a','z'), z3.Range('A','Z'), z3.Range('0','9'), z3.Re(z3.StringVal('_'))))
        if ch == 's': return z3.Union(z3.Re(z3.StringVal(' ')), z3.Re(z3.StringVal('\t')), z3.Re(z3.StringVal('\n')))
        if ch == 'S': return z3.Complement(z3.Union(z3.Re(z3.StringVal(' ')), z3.Re(z3.StringVal('\t')), z3.Re(z3.StringVal('\n'))))
        return z3.Re(z3.StringVal(ch))  # escaped literal

    def parse_atom():
        ch = peek()
        if ch is None:
            return z3.Re(z3.StringVal(''))
        if ch == '(':
            consume()
            inner = parse_alternation()
            if peek() == ')': consume()
            return inner
        if ch == '[':
            consume()
            return parse_char_class()
        if ch == '.':
            consume()
            return z3.AllChar()
        if ch == '\\':
            consume()
            return _esc_to_z3(consume())
        if ch in ('|', ')', '*', '+', '?'):
            return z3.Re(z3.StringVal(''))
        consume()
        return z3.Re(z3.StringVal(ch))

    def parse_quantified():
        atom = parse_atom()
        q = peek()
        if q == '*':  consume(); return z3.Star(atom)
        if q == '+':  consume(); return z3.Plus(atom)
        if q == '?':  consume(); return z3.Option(atom)
        return atom

    def parse_concat():
        parts = []
        while peek() and peek() not in ('|', ')'):
            parts.append(parse_quantified())
        if not parts:
            return z3.Re(z3.StringVal(''))
        result = parts[0]
        for p in parts[1:]:
            result = z3.Concat(result, p)
        return result

    def parse_alternation():
        left = parse_concat()
        if peek() == '|':
            consume()
            right = parse_alternation()
            return z3.Union(left, right)
        return left

    return parse_alternation()

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
        # The parser produces `obj × .field` (juxt_dot_app) for `obj.field` syntax.
        # Intercept this before evaluating operands, since `obj` may not exist as
        # a standalone variable — only `obj.field` does.
        if (expr.op == '×'
                and isinstance(expr.right, FieldAccess)
                and isinstance(expr.right.obj, Identifier)
                and expr.right.obj.name == '.'):
            # Reinterpret as a dotted field lookup: obj.field
            if isinstance(expr.left, Identifier):
                key = f"{expr.left.name}.{expr.right.field}"
                value = env.lookup(key)
                if value is not None:
                    return value
            # Could be a nested access like (a.b).c — recurse on left first
            obj_val = translate_expr(expr.left, env, registry)
            raise NotImplementedError(
                f"Nested field access {expr!r} is not yet supported."
            )

        # Built-in function applications via juxtaposition: int_to_str n
        if expr.op == '×' and isinstance(expr.left, Identifier):
            fn = expr.left.name
            if fn == 'int_to_str':
                return z3.IntToStr(translate_expr(expr.right, env, registry))
            if fn == 'str_to_int':
                return z3.StrToInt(translate_expr(expr.right, env, registry))

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
        if op == "++":
            return z3.Concat(left, right)
        raise NotImplementedError(
            f"BinaryExpr op {op!r} not supported in translate_expr. "
            "Set operations (∪, ∩, \\, ×) are handled in the sets module."
        )

    # ── Unary negation ────────────────────────────────────────────────────────
    # ── Sequence literal: ⟨a, b, c⟩ ─────────────────────────────────────────
    if isinstance(expr, SeqLiteral):
        if not expr.elements:
            raise NotImplementedError(
                "Empty sequence literal ⟨⟩ requires a known element type. "
                "Declare the variable with a type first: s ∈ Seq(Nat)."
            )
        elements = [translate_expr(e, env, registry) for e in expr.elements]
        result = z3.Unit(elements[0])
        for e in elements[1:]:
            result = z3.Concat(result, z3.Unit(e))
        return result

    # ── Sequence / string indexing: s[i] ─────────────────────────────────────
    if isinstance(expr, FilterExpr):
        collection = translate_expr(expr.set, env, registry)
        if z3.is_seq(collection):
            idx = translate_expr(expr.condition, env, registry)
            return collection[idx]
        raise NotImplementedError(
            f"FilterExpr indexing is only supported on sequences, not {collection.sort()}."
        )

    # ── Sequence / string length: #s ─────────────────────────────────────────
    if isinstance(expr, CardinalityExpr):
        inner = translate_expr(expr.set, env, registry)
        if z3.is_seq(inner):
            return z3.Length(inner)
        raise NotImplementedError("Set cardinality |S| must appear as a constraint, not an expression.")

    if isinstance(expr, UnaryExpr):
        if expr.op == "¬":
            return z3.Not(translate_expr(expr.operand, env, registry))
        if expr.op == '-':
            return -translate_expr(expr.operand, env, registry)
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
        # ── String predicates ──────────────────────────────────────────
        if op == "starts_with":
            lhs = translate_expr(constraint.left,  env, registry)
            rhs = translate_expr(constraint.right, env, registry)
            return z3.PrefixOf(rhs, lhs)   # PrefixOf(prefix, full_string)
        if op == "ends_with":
            lhs = translate_expr(constraint.left,  env, registry)
            rhs = translate_expr(constraint.right, env, registry)
            return z3.SuffixOf(rhs, lhs)   # SuffixOf(suffix, full_string)
        if op == "contains":
            lhs = translate_expr(constraint.left,  env, registry)
            rhs = translate_expr(constraint.right, env, registry)
            return z3.Contains(lhs, rhs)
        if op == "matches":
            lhs     = translate_expr(constraint.left,  env, registry)
            pattern = translate_expr(constraint.right, env, registry)
            return z3.InRe(lhs, z3.Re(pattern))
        raise NotImplementedError(f"ArithmeticConstraint op {op!r} not supported.")

    # ── MembershipConstraint ──────────────────────────────────────────────────
    if isinstance(constraint, MembershipConstraint):
        op = constraint.op
        left = constraint.left
        right = constraint.right

        # Determine if the right-hand side is a named primitive type.
        rhs_name = right.name if isinstance(right, Identifier) else None

        # ── ∋ / ∌ : haystack ∋ needle (string/sequence containment) ─────────────
        if op in ("∋", "∌"):
            haystack = translate_expr(left,  env, registry)
            needle   = translate_expr(right, env, registry)
            result   = z3.Contains(haystack, needle)
            return z3.Not(result) if op == "∌" else result

        if op in ("∈", "∉"):
            # Regex literal: s ∈ /pattern/
            if isinstance(right, RegexLiteral):
                lhs = translate_expr(left, env, registry)
                re  = _build_z3_regex(right.pattern)
                result = z3.InRe(lhs, re)
                return z3.Not(result) if op == "∉" else result

            # String/sequence containment: needle ∈ haystack
            # Only fires when right is a variable (not a type name or set literal)
            if isinstance(right, Identifier) and right.name not in (
                'Nat','Int','Real','Bool','String',
            ):
                named = registry.get_named_set(right.name)
                rhs_expr = None
                if named is None:
                    try:
                        rhs_expr = translate_expr(right, env, registry)
                    except (KeyError, NotImplementedError):
                        rhs_expr = None
                if rhs_expr is not None and z3.is_string(rhs_expr):
                    # String: needle ∈ string_var → substring check
                    lhs = translate_expr(left, env, registry)
                    result = z3.Contains(rhs_expr, lhs)
                    return z3.Not(result) if op == "∉" else result
                if rhs_expr is not None and z3.is_seq(rhs_expr):
                    # Seq(T): element ∈ seq_var → element containment
                    lhs = translate_expr(left, env, registry)
                    result = z3.Contains(rhs_expr, z3.Unit(lhs))
                    return z3.Not(result) if op == "∉" else result

            # Resolve named set reference: x ∈ months_map
            if isinstance(right, Identifier):
                named = registry.get_named_set(right.name)
                if named is not None:
                    right = named
                    constraint = MembershipConstraint(op=op, left=left, right=right)

            # Union/difference: x ∈ A ∪ B  ≡  x ∈ A ∨ x ∈ B
            #                   x ∉ A ∪ B  ≡  x ∉ A ∧ x ∉ B
            # Handles any nesting depth via recursion.
            if isinstance(right, BinaryExpr) and right.op == '∪':
                left_c  = translate_constraint(MembershipConstraint(op=op, left=left, right=right.left),  env, registry)
                right_c = translate_constraint(MembershipConstraint(op=op, left=left, right=right.right), env, registry)
                return z3.Or(left_c, right_c) if op == '∈' else z3.And(left_c, right_c)

        if op == "∈":
            # Inline enum: x ∈ Red | Green | Blue
            if isinstance(right, InlineEnumExpr):
                x = translate_expr(left, env, registry)
                ctors = [registry.get_constructor(v) for v in right.variants]
                ctors = [c for c in ctors if c is not None]
                if ctors:
                    return z3.Or(*[x == c for c in ctors])
                return z3.BoolVal(True)
            # Tuple membership: (x, y) ∈ {(a, b), (c, d), ...}
            # Each tuple in the set produces a conjunction; the whole thing is a disjunction.
            # This is the direct set-theoretic definition of a finite relation.
            if isinstance(left, TupleLiteral) and isinstance(right, SetLiteral):
                lhs = [translate_expr(e, env, registry) for e in left.elements]
                clauses = []
                for elem in right.elements:
                    if isinstance(elem, TupleLiteral) and len(elem.elements) == len(lhs):
                        rhs = [translate_expr(e, env, registry) for e in elem.elements]
                        clauses.append(z3.And(*[l == r for l, r in zip(lhs, rhs)]))
                return z3.Or(*clauses) if clauses else z3.BoolVal(False)

            # Set literal: x ∈ {1, 2, 3}  →  x=1 ∨ x=2 ∨ x=3
            if isinstance(right, SetLiteral):
                x = translate_expr(left, env, registry)
                if not right.elements:
                    return z3.BoolVal(False)
                return z3.Or(*[x == translate_expr(e, env, registry)
                               for e in right.elements])
            # Range literal: x ∈ {lo..hi}  →  lo ≤ x ≤ hi
            if isinstance(right, RangeLiteral):
                x   = translate_expr(left, env, registry)
                lo  = translate_expr(right.from_, env, registry)
                hi  = translate_expr(right.to, env, registry)
                return z3.And(lo <= x, x <= hi)
            if rhs_name == "Nat":
                x = translate_expr(left, env, registry)
                return x >= z3.IntVal(0)
            if rhs_name in ("Int", "Bool"):
                return z3.BoolVal(True)
            # General case: right is a Set (Array sort) — use array select.
            x = translate_expr(left, env, registry)
            s = translate_expr(right, env, registry)
            return z3.Select(s, x)

        if op == "∉":
            # Set literal: x ∉ {1, 2, 3}  →  x≠1 ∧ x≠2 ∧ x≠3
            if isinstance(right, SetLiteral):
                x = translate_expr(left, env, registry)
                if not right.elements:
                    return z3.BoolVal(True)
                return z3.And(*[x != translate_expr(e, env, registry)
                                for e in right.elements])
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
