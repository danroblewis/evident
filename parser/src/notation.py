"""
Notation expansion: AST-level substitution for user-defined syntactic rewrites.

A notation declares a term with positional holes:
    notation adjacent seq = {(seq[i], seq[i+1]) | i ∈ {0..#seq-2}}

When 'adjacent lines' appears in an expression, it expands by substituting
'lines' for every free occurrence of 'seq' in the body AST — before any
constraint translation or Z3 interaction.

This is AST substitution, not string replacement. Operator precedence and
structure are preserved. The body is parsed once at definition time; use
sites substitute their argument into the stored AST template.
"""

from __future__ import annotations
from .ast import (
    Expr, Identifier, FieldAccess, TupleIndex, FilterExpr,
    SetComprehension, ComprehensionGenerator, SetLiteral, EmptySet,
    RangeLiteral, TupleLiteral, BinaryExpr, UnaryExpr, CardinalityExpr,
    ChainExpr, SeqLiteral, SeqType, RegexLiteral,
    NatLiteral, IntLiteral, RealLiteral, StringLiteral, BoolLiteral,
    InlineEnumExpr, Binding,
    Constraint, MembershipConstraint, ArithmeticConstraint,
    LogicConstraint, UniversalConstraint, ExistentialConstraint,
    CardinalityConstraint, ApplicationConstraint, BindingConstraint,
    SetEqualityConstraint,
)


def substitute(expr: Expr, bindings: dict[str, Expr]) -> Expr:
    """Replace free occurrences of Identifier(name) in expr with bindings[name]."""
    if not bindings:
        return expr

    if isinstance(expr, Identifier):
        return bindings.get(expr.name, expr)

    if isinstance(expr, BinaryExpr):
        return BinaryExpr(
            op=expr.op,
            left=substitute(expr.left, bindings),
            right=substitute(expr.right, bindings),
        )

    if isinstance(expr, UnaryExpr):
        return UnaryExpr(op=expr.op, operand=substitute(expr.operand, bindings))

    if isinstance(expr, CardinalityExpr):
        return CardinalityExpr(set=substitute(expr.set, bindings))

    if isinstance(expr, FieldAccess):
        return FieldAccess(obj=substitute(expr.obj, bindings), field=expr.field)

    if isinstance(expr, TupleIndex):
        return TupleIndex(obj=substitute(expr.obj, bindings), index=expr.index)

    if isinstance(expr, FilterExpr):
        return FilterExpr(
            set=substitute(expr.set, bindings),
            condition=substitute_constraint(expr.condition, bindings),
        )

    if isinstance(expr, TupleLiteral):
        return TupleLiteral(elements=[substitute(e, bindings) for e in expr.elements])

    if isinstance(expr, SetLiteral):
        return SetLiteral(elements=[substitute(e, bindings) for e in expr.elements])

    if isinstance(expr, SeqLiteral):
        return SeqLiteral(elements=[substitute(e, bindings) for e in expr.elements])

    if isinstance(expr, RangeLiteral):
        return RangeLiteral(
            from_=substitute(expr.from_, bindings),
            to=substitute(expr.to, bindings),
        )

    if isinstance(expr, ChainExpr):
        return ChainExpr(
            op=expr.op,
            left=substitute(expr.left, bindings),
            right=substitute(expr.right, bindings),
        )

    if isinstance(expr, SetComprehension):
        # Generators introduce bound variables. Substitute in the set expression
        # of each generator (the domain), but NOT in the variable names.
        # For now substitute everywhere — safe as long as param names ≠ bound vars.
        new_generators = [_subst_generator(g, bindings) for g in expr.generators]
        return SetComprehension(
            output=substitute(expr.output, bindings),
            generators=new_generators,
        )

    # Leaf nodes (literals, EmptySet, SeqType, RegexLiteral, InlineEnumExpr):
    # no substitution needed.
    return expr


def _subst_generator(gen: ComprehensionGenerator,
                     bindings: dict[str, Expr]) -> ComprehensionGenerator:
    if gen.binding is not None:
        b = gen.binding
        new_set = substitute(b.set, bindings)
        new_binding = Binding(names=b.names, set=new_set,
                              guard=b.guard, distinct=b.distinct)
        return ComprehensionGenerator(binding=new_binding)
    if gen.constraint is not None:
        return ComprehensionGenerator(
            constraint=substitute_constraint(gen.constraint, bindings)
        )
    return gen


def substitute_constraint(c: Constraint, bindings: dict[str, Expr]) -> Constraint:
    """Substitute free expression variables inside a constraint."""
    if isinstance(c, MembershipConstraint):
        return MembershipConstraint(
            op=c.op,
            left=substitute(c.left, bindings),
            right=substitute(c.right, bindings),
        )

    if isinstance(c, ArithmeticConstraint):
        return ArithmeticConstraint(
            op=c.op,
            left=substitute(c.left, bindings),
            right=substitute(c.right, bindings),
        )

    if isinstance(c, LogicConstraint):
        return LogicConstraint(
            op=c.op,
            left=substitute_constraint(c.left, bindings) if c.left else None,
            right=substitute_constraint(c.right, bindings),
        )

    if isinstance(c, UniversalConstraint):
        new_bindings = [
            Binding(names=b.names,
                    set=substitute(b.set, bindings),
                    guard=b.guard, distinct=b.distinct)
            for b in c.bindings
        ]
        return UniversalConstraint(
            bindings=new_bindings,
            body=substitute_constraint(c.body, bindings),
        )

    if isinstance(c, ExistentialConstraint):
        new_bindings = [
            Binding(names=b.names,
                    set=substitute(b.set, bindings),
                    guard=b.guard, distinct=b.distinct)
            for b in c.bindings
        ]
        return ExistentialConstraint(
            quantifier=c.quantifier,
            bindings=new_bindings,
            body=substitute_constraint(c.body, bindings),
        )

    if isinstance(c, ApplicationConstraint):
        return ApplicationConstraint(
            name=c.name,
            args=[substitute(a, bindings) for a in c.args],
            mappings=c.mappings,
            block_mappings=c.block_mappings,
        )

    if isinstance(c, BindingConstraint):
        return BindingConstraint(
            name=c.name,
            value=substitute(c.value, bindings),
        )

    return c


def expand_notation(expr: Expr, notations: dict) -> Expr:
    """
    Recursively expand notation applications in an expression.

    A notation application looks like BinaryExpr(×, Identifier(name), arg)
    — the same juxtaposition form used for int_to_str.  Multi-argument
    notations are left-associative chains of the same form.
    """
    if isinstance(expr, BinaryExpr) and expr.op == '×':
        # Collect juxtaposition chain: f a b c → [f, a, b, c]
        name, args = _collect_juxt(expr)
        if name in notations:
            nd = notations[name]
            if len(args) == len(nd.params):
                # Substitute each argument for its parameter, expanding args first
                expanded_args = [expand_notation(a, notations) for a in args]
                bindings = dict(zip(nd.params, expanded_args))
                result = substitute(nd.body, bindings)
                # Expand again in case the body itself contains notations
                return expand_notation(result, notations)
        # Not a notation — still recurse into children
        return BinaryExpr(
            op=expr.op,
            left=expand_notation(expr.left, notations),
            right=expand_notation(expr.right, notations),
        )

    # Recurse into all expression types
    if isinstance(expr, UnaryExpr):
        return UnaryExpr(op=expr.op, operand=expand_notation(expr.operand, notations))
    if isinstance(expr, CardinalityExpr):
        return CardinalityExpr(set=expand_notation(expr.set, notations))
    if isinstance(expr, FieldAccess):
        return FieldAccess(obj=expand_notation(expr.obj, notations), field=expr.field)
    if isinstance(expr, TupleIndex):
        return TupleIndex(obj=expand_notation(expr.obj, notations), index=expr.index)
    if isinstance(expr, FilterExpr):
        return FilterExpr(
            set=expand_notation(expr.set, notations),
            condition=expand_notation_constraint(expr.condition, notations),
        )
    if isinstance(expr, TupleLiteral):
        return TupleLiteral([expand_notation(e, notations) for e in expr.elements])
    if isinstance(expr, SetLiteral):
        return SetLiteral([expand_notation(e, notations) for e in expr.elements])
    if isinstance(expr, SeqLiteral):
        return SeqLiteral([expand_notation(e, notations) for e in expr.elements])
    if isinstance(expr, RangeLiteral):
        return RangeLiteral(
            from_=expand_notation(expr.from_, notations),
            to=expand_notation(expr.to, notations),
        )
    if isinstance(expr, ChainExpr):
        return ChainExpr(
            op=expr.op,
            left=expand_notation(expr.left, notations),
            right=expand_notation(expr.right, notations),
        )
    if isinstance(expr, SetComprehension):
        new_gens = [_expand_generator(g, notations) for g in expr.generators]
        return SetComprehension(
            output=expand_notation(expr.output, notations),
            generators=new_gens,
        )
    return expr


def expand_notation_constraint(c: Constraint, notations: dict) -> Constraint:
    """Expand notation applications inside a constraint."""
    if isinstance(c, MembershipConstraint):
        return MembershipConstraint(
            op=c.op,
            left=expand_notation(c.left, notations),
            right=expand_notation(c.right, notations),
        )
    if isinstance(c, ArithmeticConstraint):
        return ArithmeticConstraint(
            op=c.op,
            left=expand_notation(c.left, notations),
            right=expand_notation(c.right, notations),
        )
    if isinstance(c, LogicConstraint):
        return LogicConstraint(
            op=c.op,
            left=expand_notation_constraint(c.left, notations) if c.left else None,
            right=expand_notation_constraint(c.right, notations),
        )
    if isinstance(c, UniversalConstraint):
        new_bindings = [
            Binding(names=b.names,
                    set=expand_notation(b.set, notations),
                    guard=b.guard, distinct=b.distinct)
            for b in c.bindings
        ]
        return UniversalConstraint(
            bindings=new_bindings,
            body=expand_notation_constraint(c.body, notations),
        )
    if isinstance(c, ExistentialConstraint):
        new_bindings = [
            Binding(names=b.names,
                    set=expand_notation(b.set, notations),
                    guard=b.guard, distinct=b.distinct)
            for b in c.bindings
        ]
        return ExistentialConstraint(
            quantifier=c.quantifier,
            bindings=new_bindings,
            body=expand_notation_constraint(c.body, notations),
        )
    if isinstance(c, ApplicationConstraint):
        return ApplicationConstraint(
            name=c.name,
            args=[expand_notation(a, notations) for a in c.args],
            mappings=c.mappings,
            block_mappings=c.block_mappings,
        )
    if isinstance(c, BindingConstraint):
        return BindingConstraint(
            name=c.name,
            value=expand_notation(c.value, notations),
        )
    return c


def _collect_juxt(expr: BinaryExpr) -> tuple[str, list[Expr]]:
    """
    Unpack a left-associative juxtaposition chain into (name, [arg1, arg2, ...]).
    'f a b' parses as BinaryExpr(×, BinaryExpr(×, f, a), b).
    Returns ('f', [a, b]).
    """
    args = []
    node = expr
    while isinstance(node, BinaryExpr) and node.op == '×':
        args.append(node.right)
        node = node.left
    if isinstance(node, Identifier):
        args.reverse()
        return node.name, args
    return '', []


def _expand_generator(gen: ComprehensionGenerator,
                      notations: dict) -> ComprehensionGenerator:
    if gen.binding is not None:
        b = gen.binding
        return ComprehensionGenerator(
            binding=Binding(names=b.names,
                            set=expand_notation(b.set, notations),
                            guard=b.guard, distinct=b.distinct)
        )
    if gen.constraint is not None:
        return ComprehensionGenerator(
            constraint=expand_notation_constraint(gen.constraint, notations)
        )
    return gen
