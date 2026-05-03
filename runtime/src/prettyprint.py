"""
Render Evident AST nodes back to source-like text.
Used for unsat-core diagnostics in the test runner.
"""

from __future__ import annotations


def pretty_expr(node) -> str:
    from .ast_types import (
        Identifier, FieldAccess, TupleIndex, TupleLiteral, SeqLiteral,
        SetLiteral, RangeLiteral, StringLiteral, NatLiteral, IntLiteral,
        RealLiteral, BoolLiteral, BinaryExpr, UnaryExpr, CardinalityExpr,
        InlineEnumExpr, SeqType, EmptySet, RegexLiteral,
    )
    if isinstance(node, Identifier):
        return node.name
    if isinstance(node, FieldAccess):
        return f'{pretty_expr(node.obj)}.{node.field}'
    if isinstance(node, TupleIndex):
        return f'{pretty_expr(node.obj)}.{node.index}'
    if isinstance(node, TupleLiteral):
        return '(' + ', '.join(pretty_expr(e) for e in node.elements) + ')'
    if isinstance(node, SeqLiteral):
        return '⟨' + ', '.join(pretty_expr(e) for e in node.elements) + '⟩'
    if isinstance(node, SetLiteral):
        return '{' + ', '.join(pretty_expr(e) for e in node.elements) + '}'
    if isinstance(node, EmptySet):
        return '{}'
    if isinstance(node, RangeLiteral):
        return f'{{{pretty_expr(node.from_)}..{pretty_expr(node.to)}}}'
    if isinstance(node, StringLiteral):
        return f'"{node.value}"'
    if isinstance(node, (NatLiteral, IntLiteral)):
        return str(node.value)
    if isinstance(node, RealLiteral):
        return str(node.value)
    if isinstance(node, BoolLiteral):
        return 'true' if node.value else 'false'
    if isinstance(node, BinaryExpr):
        l, r = pretty_expr(node.left), pretty_expr(node.right)
        return f'({l} {node.op} {r})'
    if isinstance(node, UnaryExpr):
        return f'{node.op}{pretty_expr(node.operand)}'
    if isinstance(node, CardinalityExpr):
        return f'#{pretty_expr(node.set)}'
    if isinstance(node, InlineEnumExpr):
        return ' | '.join(node.variants)
    if isinstance(node, SeqType):
        return f'Seq({node.element_name})'
    if isinstance(node, RegexLiteral):
        return f'/{node.pattern}/'
    return repr(node)


def vars_in_expr(node) -> set[str]:
    """Collect all variable names referenced in an expression."""
    from .ast_types import (
        Identifier, FieldAccess, TupleLiteral, SeqLiteral, SetLiteral,
        BinaryExpr, UnaryExpr, CardinalityExpr, RangeLiteral,
    )
    if isinstance(node, Identifier):
        return {node.name}
    if isinstance(node, FieldAccess):
        inner = vars_in_expr(node.obj)
        if isinstance(node.obj, Identifier):
            inner.add(f'{node.obj.name}.{node.field}')
        return inner
    if isinstance(node, (TupleLiteral, SeqLiteral, SetLiteral)):
        result: set[str] = set()
        for e in node.elements:
            result |= vars_in_expr(e)
        return result
    if isinstance(node, BinaryExpr):
        return vars_in_expr(node.left) | vars_in_expr(node.right)
    if isinstance(node, UnaryExpr):
        return vars_in_expr(node.operand)
    if isinstance(node, CardinalityExpr):
        return vars_in_expr(node.set)
    if isinstance(node, RangeLiteral):
        return vars_in_expr(node.from_) | vars_in_expr(node.to)
    return set()


def vars_in_constraint(node) -> set[str]:
    """Collect all variable names referenced in a constraint."""
    from .ast_types import (
        MembershipConstraint, ArithmeticConstraint, LogicConstraint,
        BindingConstraint, SetEqualityConstraint, ExistentialConstraint,
        UniversalConstraint,
    )
    if isinstance(node, (MembershipConstraint, ArithmeticConstraint)):
        return vars_in_expr(node.left) | vars_in_expr(node.right)
    if isinstance(node, BindingConstraint):
        return {node.name} | vars_in_expr(node.value)
    if isinstance(node, SetEqualityConstraint):
        return vars_in_expr(node.set) | vars_in_expr(node.value)
    if isinstance(node, LogicConstraint):
        result: set[str] = set()
        if node.left:
            result |= vars_in_constraint(node.left)
        result |= vars_in_constraint(node.right)
        return result
    if isinstance(node, (ExistentialConstraint, UniversalConstraint)):
        result = set()
        for b in node.bindings:
            result |= vars_in_expr(b.set)
        result |= vars_in_constraint(node.body)
        return result
    return set()


def pretty_binding(b) -> str:
    names = ', '.join(b.names)
    s = f'{names} ∈ {pretty_expr(b.set)}'
    if b.guard is not None:
        s += f' where {pretty_expr(b.guard)}'
    return s


def pretty_constraint(node) -> str:
    from .ast_types import (
        MembershipConstraint, ArithmeticConstraint, LogicConstraint,
        ExistentialConstraint, UniversalConstraint, BindingConstraint,
        SetEqualityConstraint, ApplicationConstraint, CardinalityConstraint,
    )
    if isinstance(node, MembershipConstraint):
        return f'{pretty_expr(node.left)} {node.op} {pretty_expr(node.right)}'
    if isinstance(node, ArithmeticConstraint):
        return f'{pretty_expr(node.left)} {node.op} {pretty_expr(node.right)}'
    if isinstance(node, BindingConstraint):
        return f'{node.name} = {pretty_expr(node.value)}'
    if isinstance(node, SetEqualityConstraint):
        return f'{pretty_expr(node.set)} = {pretty_expr(node.value)}'
    if isinstance(node, LogicConstraint):
        if node.op == '¬':
            return f'¬({pretty_constraint(node.right)})'
        if node.left is None:
            return f'{node.op} {pretty_constraint(node.right)}'
        return f'({pretty_constraint(node.left)} {node.op} {pretty_constraint(node.right)})'
    if isinstance(node, ExistentialConstraint):
        binds = ', '.join(pretty_binding(b) for b in node.bindings)
        return f'{node.quantifier} {binds}: {pretty_constraint(node.body)}'
    if isinstance(node, UniversalConstraint):
        binds = ', '.join(pretty_binding(b) for b in node.bindings)
        return f'∀ {binds}: {pretty_constraint(node.body)}'
    if isinstance(node, ApplicationConstraint):
        parts = [node.name] + [pretty_expr(a) for a in node.args]
        return ' '.join(parts)
    if isinstance(node, CardinalityConstraint):
        return f'|{pretty_expr(node.set)}| {node.op} {pretty_expr(node.count)}'
    return repr(node)
