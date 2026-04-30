from lark import Transformer as LarkTransformer, Token, Tree, v_args
from .ast import *


def _str(token) -> str:
    return str(token)


class EvidentTransformer(LarkTransformer):

    # ── Top level ────────────────────────────────────────────────────────────

    def start(self, items):
        stmts = [s for s in items if s is not None]
        return Program(statements=stmts)

    # ── Schema keyword ────────────────────────────────────────────────────────

    def kw_schema(self, items): return "schema"
    def kw_type(self, items): return "type"
    def kw_claim(self, items): return "claim"
    def schema_kw(self, items): return items[0]

    # ── Schema declarations ──────────────────────────────────────────────────

    # ── Enum declarations ─────────────────────────────────────────────────────

    def import_stmt(self, items):
        # STRING token includes surrounding quotes — strip them
        raw = str(items[0])
        return ImportStmt(path=raw[1:-1])

    def enum_decl(self, items):
        # items = [NAME, NAME, NAME, ...] — type name followed by variant names
        name = _str(items[0])
        variants = [_str(t) for t in items[1:]]
        return EnumDecl(name=name, variants=variants)

    def param_decl_item(self, items):
        return Param(names=items[0], set=items[1])

    def param_decl_list(self, items):
        return list(items)

    def schema_block_params(self, items):
        keyword = items[0]
        name    = _str(items[1])
        params  = items[2]
        body    = items[3] if len(items) > 3 else []
        return SchemaDecl(keyword=keyword, name=name, params=params, body=body)

    def schema_block_params_no_body(self, items):
        keyword = items[0]
        name    = _str(items[1])
        params  = items[2]
        return SchemaDecl(keyword=keyword, name=name, params=params, body=[])

    def schema_block(self, items):
        # items = [schema_kw_str, NAME, body_list]
        keyword = items[0]
        name = _str(items[1])
        body = items[2] if len(items) > 2 else []
        return SchemaDecl(keyword=keyword, name=name, params=[], body=body)

    def schema_alias_simple(self, items):
        # items = [schema_kw_str, NAME, NAME]
        keyword = items[0]
        name = _str(items[1])
        alias = _str(items[2])
        body = [ApplicationConstraint(name=alias, args=[], mappings=[])]
        return SchemaDecl(keyword=keyword, name=name, params=[], body=body)

    def schema_alias_inline(self, items):
        # items = [schema_kw_str, NAME, NAME, inline_mapping, ...]
        keyword = items[0]
        name = _str(items[1])
        alias = _str(items[2])
        mappings = [m for m in items[3:] if isinstance(m, InlineMapping)]
        body = [ApplicationConstraint(name=alias, args=[], mappings=mappings)]
        return SchemaDecl(keyword=keyword, name=name, params=[], body=body)

    def params(self, items):
        return items  # list of Param

    def param(self, items):
        name_list = items[0]
        # skip IN_KW token
        expr = items[-1]
        return Param(names=name_list, set=expr)

    def name_list(self, items):
        return [_str(t) for t in items]

    # ── Body ─────────────────────────────────────────────────────────────────

    def body(self, items):
        return items  # list of body items (constraints etc.)

    def body_constraint(self, items):
        return items[0]

    def body_passthrough(self, items):
        return items[0]

    def body_schema(self, items):
        return items[0]

    def body_evident(self, items):
        return items[0]

    def multi_mem_in(self, items):
        # NAME ("," NAME)+ ∈ expr  →  names=[…], set=expr
        names = [_str(t) for t in items[:-1]]
        return MultiMembershipDecl(names=names, set=items[-1])

    def passthrough_plain(self, items):
        return PassthroughItem(name=_str(items[0]), mappings=[])

    def passthrough_renamed(self, items):
        name = _str(items[0])
        mappings = [m for m in items[1:] if isinstance(m, InlineMapping)]
        return PassthroughItem(name=name, mappings=mappings)

    def pass_rename_item(self, items):
        # sub_var ↦ parent_var  →  InlineMapping(slot=sub_var, value=Identifier(parent_var))
        return InlineMapping(slot=_str(items[0]), value=Identifier(name=_str(items[1])))

    def passthrough(self, items):
        return PassthroughItem(name=_str(items[0]), mappings=[])

    # ── Evident blocks ───────────────────────────────────────────────────────

    def evident_block(self, items):
        patterns = []
        guard = None
        body = []
        for item in items:
            if isinstance(item, list) and item and isinstance(item[0], (
                    PatternIdentifier, PatternLiteral, PatternEmptyList,
                    PatternCons, PatternRecord, PatternWildcard)):
                patterns.extend(item)
            elif isinstance(item, list) and item and not isinstance(item[0], (
                    PatternIdentifier, PatternLiteral, PatternEmptyList,
                    PatternCons, PatternRecord, PatternWildcard)):
                body = item
            else:
                if not isinstance(item, list):
                    if isinstance(item, (PatternIdentifier, PatternLiteral, PatternEmptyList,
                                         PatternCons, PatternRecord, PatternWildcard)):
                        patterns.append(item)
                    elif item is not None:
                        guard = item
        return EvidentBlock(patterns=patterns, guard=guard, body=body)

    def evident_top(self, items):
        # Same structure as evident_block but body may be absent
        return self.evident_block(items)

    # Pattern args
    def pat_empty_list(self, items):
        return PatternEmptyList()

    def pat_cons(self, items):
        # [head | tail] - items are head and tail patterns
        if len(items) == 2:
            return PatternCons(head=items[0], tail=items[1])
        # Multiple: build nested cons
        result = items[-1]
        for item in reversed(items[:-1]):
            result = PatternCons(head=item, tail=result)
        return result

    def pat_record(self, items):
        return PatternRecord(fields=items)

    def pat_identifier(self, items):
        return PatternIdentifier(name=_str(items[0]))

    def pat_literal(self, items):
        return PatternLiteral(value=items[0])

    def pat_tuple(self, items):
        # Tuple pattern - treat as record-like or just keep as cons chain
        # For now, represent as identifier with the tuple structure
        return PatternLiteral(value=TupleLiteral(elements=list(items)))

    def field_pat_binding(self, items):
        return PatternField(name=_str(items[0]), binding=_str(items[1]))

    def field_pat_shorthand(self, items):
        return PatternField(name=_str(items[0]), binding=None)

    # ── Constraints ──────────────────────────────────────────────────────────

    def forall_constraint(self, items):
        # items = [binding_list, constraint] (keyword filtered)
        bindings = items[0]
        body = items[1]
        return UniversalConstraint(bindings=bindings, body=body)

    def body_forall_indented(self, items):
        # items = [binding_list, constraint] (keywords filtered)
        bindings = items[0]
        body = items[1]
        return UniversalConstraint(bindings=bindings, body=body)

    def body_forall_indented_nodedent(self, items):
        bindings = items[0]
        body = items[1]
        return UniversalConstraint(bindings=bindings, body=body)

    def exists_some(self, items):
        bindings = items[0]
        body = items[1]
        return ExistentialConstraint(quantifier='∃', bindings=bindings, body=body)

    def exists_unique(self, items):
        bindings = items[0]
        body = items[1]
        return ExistentialConstraint(quantifier='∃!', bindings=bindings, body=body)

    def exists_none(self, items):
        bindings = items[0]
        body = items[1]
        return ExistentialConstraint(quantifier='¬∃', bindings=bindings, body=body)

    def logic_or_constraint(self, items):
        if len(items) == 1:
            return items[0]
        # items alternate: expr, op, expr, op, expr...
        # With OR_OP tokens mixed in
        operands = [items[i] for i in range(0, len(items), 2)]
        ops = [items[i] for i in range(1, len(items), 2)]
        result = operands[0]
        for i, op in enumerate(ops):
            result = LogicConstraint(op='∨', left=result, right=operands[i+1])
        return result

    def logic_and_constraint(self, items):
        if len(items) == 1:
            return items[0]
        # Conjunction - items may have AND_OP tokens
        operands = [items[i] for i in range(0, len(items), 2)]
        ops = [items[i] for i in range(1, len(items), 2)]
        result = operands[0]
        for i, op in enumerate(ops):
            result = LogicConstraint(op='∧', left=result, right=operands[i+1])
        return result

    def logic_implies_constraint(self, items):
        if len(items) == 1:
            return items[0]
        operands = [items[i] for i in range(0, len(items), 2)]
        ops = [items[i] for i in range(1, len(items), 2)]
        result = operands[0]
        for i, op in enumerate(ops):
            result = LogicConstraint(op='⇒', left=result, right=operands[i+1])
        return result

    def not_constraint(self, items):
        # items[0] = NOT_OP token, items[1] = constraint
        return LogicConstraint(op='¬', left=None, right=items[1])

    # The grammar uses aliases (-> logic_or, -> logic_and, -> logic_implies,
    # -> logic_not) that differ from the method names above.  When Lark calls
    # the alias method the operator tokens are already filtered, so items is
    # just [left, right] (or [right] for not) — not the alternating
    # [expr, op, expr] that the *_constraint methods expect.
    def logic_or(self, items):
        return LogicConstraint(op='∨', left=items[0], right=items[1])
    def logic_and(self, items):
        return LogicConstraint(op='∧', left=items[0], right=items[1])
    def logic_implies(self, items):
        return LogicConstraint(op='⇒', left=items[0], right=items[1])
    def logic_not(self, items):
        return LogicConstraint(op='¬', left=None, right=items[0])

    def paren_constraint(self, items):
        return items[0]

    # Membership constraints (keyword is filtered; items = [left_expr, right_expr])
    # ── Chained comparisons ──────────────────────────────────────────────────

    def cmp_lt(self, items):  return '<'
    def cmp_gt(self, items):  return '>'
    def cmp_lte(self, items): return '≤'
    def cmp_gte(self, items): return '≥'
    def cmp_eq(self, items):  return '='
    def cmp_neq(self, items): return '≠'

    def arith_chain(self, items):
        # items alternates: expr, op, expr, op, expr, ...
        exprs = items[0::2]
        ops   = items[1::2]
        constraints = [
            ArithmeticConstraint(op=ops[i], left=exprs[i], right=exprs[i + 1])
            for i in range(len(ops))
        ]
        result = constraints[0]
        for c in constraints[1:]:
            result = LogicConstraint(op='∧', left=result, right=c)
        return result

    def _maybe_regex(self, node):
        """If node is a StringLiteral with __REGEX__ prefix, return RegexLiteral."""
        from parser.src.normalizer import decode_regex_string
        if isinstance(node, StringLiteral):
            pattern = decode_regex_string(node.value)
            if pattern is not None:
                return RegexLiteral(pattern=pattern)
        return node

    def mem_in(self, items):
        left  = items[0]
        right = self._maybe_regex(items[1])
        # /regex/ ∋ s was normalised to "__REGEX__..." ∈ s — flip it back
        if isinstance(right, RegexLiteral) or isinstance(left, RegexLiteral):
            # left is expr (could be regex from /re/ ∋ s normalisation)
            left  = self._maybe_regex(left)
            # If left is the regex and right is the string, swap for semantics
            if isinstance(left, RegexLiteral):
                left, right = right, left
        return MembershipConstraint(op='∈', left=left, right=right)

    def mem_contains(self, items):
        # haystack ∋ needle  (string containment)
        # Also handles "__REGEX__pattern" ∋ s (right side is string)
        left  = self._maybe_regex(items[0])
        right = items[1]
        if isinstance(left, RegexLiteral):
            # /re/ ∋ s  →  s ∈ /re/
            return MembershipConstraint(op='∈', left=right, right=left)
        return MembershipConstraint(op='∋', left=left, right=right)

    def mem_not_contains(self, items):
        left  = self._maybe_regex(items[0])
        right = items[1]
        if isinstance(left, RegexLiteral):
            return MembershipConstraint(op='∉', left=right, right=left)
        return MembershipConstraint(op='∌', left=left, right=right)

    def mem_inline_enum(self, items):
        # items = [left_expr, NAME, NAME, ...]
        left = items[0]
        variants = [_str(t) for t in items[1:]]
        return MembershipConstraint(op='∈', left=left, right=InlineEnumExpr(variants=variants))

    def mem_not_in(self, items):
        return MembershipConstraint(op='∉', left=items[0], right=items[1])

    def mem_subset(self, items):
        return MembershipConstraint(op='⊆', left=items[0], right=items[1])

    def mem_superset(self, items):
        return MembershipConstraint(op='⊇', left=items[0], right=items[1])

    # Arithmetic constraints
    def arith_eq(self, items):
        return ArithmeticConstraint(op='=', left=items[0], right=items[1])

    def arith_neq(self, items):
        return ArithmeticConstraint(op='≠', left=items[0], right=items[1])

    def arith_lt(self, items):
        return ArithmeticConstraint(op='<', left=items[0], right=items[1])

    def arith_gt(self, items):
        return ArithmeticConstraint(op='>', left=items[0], right=items[1])

    def arith_lte(self, items):
        return ArithmeticConstraint(op='≤', left=items[0], right=items[1])

    def arith_gte(self, items):
        return ArithmeticConstraint(op='≥', left=items[0], right=items[1])

    # Application constraints
    def app_args(self, items):
        name = _str(items[0])
        args = list(items[1:])
        return ApplicationConstraint(name=name, args=args, mappings=[])

    def app_inline_mappings(self, items):
        name = _str(items[0])
        mappings = [m for m in items[1:] if isinstance(m, InlineMapping)]
        return ApplicationConstraint(name=name, args=[], mappings=mappings)

    def app_block_mappings(self, items):
        name = _str(items[0])
        block_mappings = [m for m in items[1:] if isinstance(m, BlockMapping)]
        return ApplicationConstraint(name=name, args=[], mappings=[], block_mappings=block_mappings)

    def app_block_mappings_nl(self, items):
        name = _str(items[0])
        block_mappings = [m for m in items[1:] if isinstance(m, BlockMapping)]
        return ApplicationConstraint(name=name, args=[], mappings=[], block_mappings=block_mappings)

    def app_arg_name(self, items):
        return Identifier(name=_str(items[0]))

    def app_arg_literal(self, items):
        return items[0]

    def app_arg_tuple(self, items):
        if len(items) == 1:
            return items[0]
        return TupleLiteral(elements=list(items))

    def app_arg_list(self, items):
        if not items:
            return SetLiteral(elements=[])
        elems = items[0] if isinstance(items[0], list) else list(items)
        return SetLiteral(elements=elems)

    def app_arg_filter(self, items):
        # NAME "[" filter_cond "]" -> FilterExpr(Identifier(NAME), filter_cond)
        name = Identifier(name=_str(items[0]))
        cond = items[1]
        return FilterExpr(set=name, condition=cond)

    def app_arg_dot_field(self, items):
        return FieldAccess(obj=Identifier(name='.'), field=_str(items[0]))

    def app_arg_set(self, items):
        return items[0]

    def inline_mapping(self, items):
        return InlineMapping(slot=_str(items[0]), value=items[1])

    def block_mapping(self, items):
        # items = [NAME, expr] ("mapsto" filtered)
        return BlockMapping(slot=_str(items[0]), value=items[1])

    # ── Bindings ─────────────────────────────────────────────────────────────

    def binding_list(self, items):
        return list(items)

    def binding_names(self, items):
        # items = [name_list, expr] (keyword filtered)
        names = items[0]
        set_expr = items[1]
        return Binding(names=names, set=set_expr)

    def binding_tuple(self, items):
        # items = [name_list, expr] (keyword filtered)
        names = items[0]
        set_expr = items[1]
        return Binding(names=names, set=set_expr)

    def binding_distinct(self, items):
        # items = [name_list, NAME, expr] (keywords filtered)
        names = items[0]
        other = _str(items[1])
        set_expr = items[2]
        return Binding(names=names + [other], set=set_expr, distinct=True)

    # ── Forward rules ─────────────────────────────────────────────────────────

    def forward_rule(self, items):
        # items = [premise_list, app_constraint] (IMPLIES keyword filtered)
        premises = items[0]
        conclusion = items[1]
        return ForwardRule(premises=premises, conclusion=conclusion)

    def premise_list(self, items):
        return list(items)

    def premise(self, items):
        name = _str(items[0])
        args = list(items[1:])
        return ApplicationConstraint(name=name, args=args, mappings=[])

    # ── Assert statements ─────────────────────────────────────────────────────

    def assert_eq(self, items):
        name = _str(items[0])
        value = items[1]
        return AssertStmt(name=name, value=value, member_of=None, args=[])

    def assert_in(self, items):
        # items = [NAME, expr] (IN keyword filtered)
        name = _str(items[0])
        member_of = items[1]
        return AssertStmt(name=name, value=None, member_of=member_of, args=[])

    def assert_ground(self, items):
        name = _str(items[0])
        args = list(items[1:])
        return AssertStmt(name=name, value=None, member_of=None, args=args)

    # ── Query statements ──────────────────────────────────────────────────────

    def query_stmt(self, items):
        return QueryStmt(constraint=items[0])

    # ── Constraint statement ───────────────────────────────────────────────────

    def constraint_stmt(self, items):
        return ConstraintStmt(constraint=items[0])

    # ── Expressions ──────────────────────────────────────────────────────────

    def expr(self, items):
        return items[0]

    def chain_expr(self, items):
        if len(items) == 1:
            return items[0]
        # items alternate: expr, CHAIN_OP, expr
        operands = [items[i] for i in range(0, len(items), 2)]
        ops = [_str(items[i]) for i in range(1, len(items), 2)]
        result = operands[0]
        for i, op in enumerate(ops):
            chain_op = '·' if op in ('__CHAIN__',) else '⋈'
            result = ChainExpr(op=chain_op, left=result, right=operands[i+1])
        return result

    def juxt_expr(self, items):
        if len(items) == 1:
            return items[0]
        # Multiple items = juxtaposition = type application = cross product
        result = items[0]
        for item in items[1:]:
            result = BinaryExpr(op='×', left=result, right=item)
        return result

    def juxt_app(self, items):
        left = items[0]
        right = items[1]
        if isinstance(right, Token):
            right = Identifier(name=_str(right))
        return BinaryExpr(op='×', left=left, right=right)

    def juxt_dot_app(self, items):
        return BinaryExpr(op='×', left=items[0], right=items[1])

    def juxt_dot(self, items):
        return FieldAccess(obj=Identifier(name='.'), field=_str(items[0]))

    def additive_expr(self, items):
        return items[0]

    def str_concat_expr(self, items):
        return BinaryExpr(op='++', left=items[0], right=items[1])

    def str_starts_with(self, items):
        return ArithmeticConstraint(op='starts_with', left=items[0], right=items[1])

    def str_ends_with(self, items):
        return ArithmeticConstraint(op='ends_with', left=items[0], right=items[1])

    def str_contains(self, items):
        return ArithmeticConstraint(op='contains', left=items[0], right=items[1])

    def str_matches(self, items):
        # right side is a STRING token — strip quotes and store as StringLiteral
        pattern = str(items[1])[1:-1]  # strip surrounding quotes
        return ArithmeticConstraint(op='matches', left=items[0], right=StringLiteral(value=pattern))

    def add_expr(self, items):
        return BinaryExpr(op='+', left=items[0], right=items[1])

    def sub_expr(self, items):
        return BinaryExpr(op='-', left=items[0], right=items[1])

    def union_expr(self, items):
        return BinaryExpr(op='∪', left=items[0], right=items[1])

    def mult_expr(self, items):
        return items[0]

    def mul_expr(self, items):
        return BinaryExpr(op='*', left=items[0], right=items[1])

    def div_expr(self, items):
        return BinaryExpr(op='/', left=items[0], right=items[1])

    def intersect_expr(self, items):
        return BinaryExpr(op='∩', left=items[0], right=items[1])

    def diff_expr(self, items):
        return BinaryExpr(op='\\', left=items[0], right=items[1])

    def cross_expr(self, items):
        return BinaryExpr(op='×', left=items[0], right=items[1])

    def unary_expr(self, items):
        return items[0]

    def unary_not(self, items):
        return UnaryExpr(op='¬', operand=items[1])

    def unary_neg(self, items):
        return UnaryExpr(op='-', operand=items[0])

    def cardinality_expr(self, items):
        return CardinalityExpr(set=items[0])

    def seq_length(self, items):
        return CardinalityExpr(set=items[0])

    def seq_type_expr(self, items):
        return SeqType(element_name=_str(items[0]))

    def seq_literal(self, items):
        return items[0]  # seq_empty_lit or seq_elems_lit

    def seq_empty_lit(self, items):
        return SeqLiteral(elements=[])

    def seq_elems_lit(self, items):
        return SeqLiteral(elements=list(items))

    def postfix_expr(self, items):
        return items[0]

    def field_access(self, items):
        return FieldAccess(obj=items[0], field=_str(items[1]))

    def tuple_index(self, items):
        return TupleIndex(obj=items[0], index=int(str(items[1])))

    def filter_expr(self, items):
        return FilterExpr(set=items[0], condition=items[1])

    def filter_dot_field(self, items):
        # .field = value
        field = _str(items[0])
        value = items[1]
        return ArithmeticConstraint(op='=',
                                     left=FieldAccess(obj=Identifier(name='.'), field=field),
                                     right=value)

    def filter_expr_cond(self, items):
        return items[0]

    def primary(self, items):
        return items[0]

    def paren_expr(self, items):
        return items[0]

    def identifier_expr(self, items):
        return Identifier(name=_str(items[0]))

    def identifier(self, items):
        return Identifier(name=_str(items[0]))

    # ── Set expressions ───────────────────────────────────────────────────────

    def empty_set(self, items):
        return EmptySet()

    def range_expr(self, items):
        return RangeLiteral(from_=items[0], to=items[1])

    def set_comprehension(self, items):
        return items[0]  # comprehension_body returns SetComprehension

    def shorthand_comprehension(self, items):
        return items[0]

    def shorthand_compr(self, items):
        # items = [NAME, expr, constraint] (IN filtered)
        name = _str(items[0])
        set_expr = items[1]
        condition = items[2]
        output = Identifier(name=name)
        binding = Binding(names=[name], set=set_expr)
        gen1 = ComprehensionGenerator(binding=binding)
        gen2 = ComprehensionGenerator(constraint=condition)
        return SetComprehension(output=output, generators=[gen1, gen2])

    def set_literal(self, items):
        return SetLiteral(elements=list(items))

    def comprehension_body(self, items):
        output = items[0]
        generators = items[1]  # list of ComprehensionGenerator
        return SetComprehension(output=output, generators=generators)

    def generator_list(self, items):
        return list(items)

    def gen_binding(self, items):
        # items = [name_list, expr]
        names = items[0]
        set_expr = items[1]
        return ComprehensionGenerator(binding=Binding(names=names, set=set_expr))

    def gen_tuple_binding(self, items):
        # items = [expr1, expr2, ..., set_expr] (keyword filtered)
        # Represent as a membership constraint: (e1, e2) ∈ set_expr
        tuple_exprs = items[:-1]
        set_expr = items[-1]
        tuple_val = TupleLiteral(elements=list(tuple_exprs))
        constraint = MembershipConstraint(op='∈', left=tuple_val, right=set_expr)
        return ComprehensionGenerator(constraint=constraint)

    def gen_expr(self, items):
        return ComprehensionGenerator(constraint=items[0])

    def tuple_literal(self, items):
        return TupleLiteral(elements=list(items))

    def tuple_paren_expr(self, items):
        return TupleLiteral(elements=list(items))

    def list_expr(self, items):
        if not items:
            return SetLiteral(elements=[])
        return SetLiteral(elements=items[0] if isinstance(items[0], list) else list(items))

    def list_elems(self, items):
        return list(items)

    # ── Literals ──────────────────────────────────────────────────────────────

    def real_lit(self, items):
        return RealLiteral(value=float(str(items[0])))

    def int_lit(self, items):
        return NatLiteral(value=int(str(items[0])))

    def string_lit(self, items):
        raw = str(items[0])
        # strip surrounding quotes
        return StringLiteral(value=raw[1:-1])

    def bool_true(self, items):
        return BoolLiteral(value=True)

    def bool_false(self, items):
        return BoolLiteral(value=False)
