"""A Z3 → Evident-ish prettifier.

The Z3 AST is already a set-theoretic structure wearing an Array/store/select
costume. This renderer walks an expr/Goal and prints it with set-theory notation
(∈ ∪ ∩ ⊆ ∀ ∃ ⇒ ∧ ∨ ¬ ≤ ≥ ≠), recognizing the patterns that matter so the
surface reads like Evident did — NOT a 1:1 operator swap.

The recognizers (the point of the whole thing):
  • set membership      select(store-chain, key)            → key ∈ {a, b, c}
  • tuple-set membership (or (and (= k a)(= v b)) …)        → (k, v) ∈ {(a,b), …}
  • range               (and (≤ lo x) (< x hi))             → lo ≤ x < hi
  • ite-chain on equality if k=0 then a else if k=1 …       → match k { 0 ⇒ a, … }

`blast_select_store` rewrites the first form into the second; both render to the
same `∈ {…}` — so the prettifier shows the lowering preserves the set meaning.
"""
import z3

# ── precedence (higher binds tighter; children below threshold get parens) ────
P_ATOM, P_SETOP, P_MUL, P_ADD, P_CMP, P_NOT, P_AND, P_OR, P_IMP, P_QUANT = \
    100, 85, 80, 70, 60, 55, 50, 40, 30, 20

_BIN = {
    z3.Z3_OP_ADD: ("+", P_ADD), z3.Z3_OP_SUB: ("−", P_ADD),
    z3.Z3_OP_MUL: ("·", P_MUL), z3.Z3_OP_DIV: ("/", P_MUL),
    z3.Z3_OP_IDIV: ("÷", P_MUL), z3.Z3_OP_MOD: ("mod", P_MUL),
    z3.Z3_OP_LE: ("≤", P_CMP), z3.Z3_OP_LT: ("<", P_CMP),
    z3.Z3_OP_GE: ("≥", P_CMP), z3.Z3_OP_GT: (">", P_CMP),
    z3.Z3_OP_EQ: ("=", P_CMP),
    z3.Z3_OP_IFF: ("⇔", P_IMP), z3.Z3_OP_XOR: ("⊕", P_OR),
    # bitvector (a useful subset)
    z3.Z3_OP_BADD: ("+", P_ADD), z3.Z3_OP_BSUB: ("−", P_ADD),
    z3.Z3_OP_BMUL: ("·", P_MUL),
    z3.Z3_OP_ULEQ: ("≤", P_CMP), z3.Z3_OP_ULT: ("<", P_CMP),
    z3.Z3_OP_UGEQ: ("≥", P_CMP), z3.Z3_OP_UGT: (">", P_CMP),
    z3.Z3_OP_SLEQ: ("≤", P_CMP), z3.Z3_OP_SLT: ("<", P_CMP),
    z3.Z3_OP_SGEQ: ("≥", P_CMP), z3.Z3_OP_SGT: (">", P_CMP),
}


def _wrap(txt, prec, need):
    return f"({txt})" if prec < need else txt


def _is_true(e):
    return z3.is_true(e)


def _is_false(e):
    return z3.is_false(e)


def _is_tuple_ctor(e):
    """A datatype constructor of a single-constructor sort → render as a tuple."""
    if e.decl().kind() != z3.Z3_OP_DT_CONSTRUCTOR or e.num_args() == 0:
        return False
    s = e.sort()
    return isinstance(s, z3.DatatypeSortRef) and s.num_constructors() == 1


# ── array rendering: a Bool-valued array is a SET; otherwise a MAP ────────────
def _render_array(e, b):
    """Flatten a store-chain. Bool elem sort → set '{a,b,c}' / '∅ ∪ {…}'.
    Other elem sort → map literal '{0 ↦ 3, 1 ↦ 4, _ ↦ d}'. Returns (text, braced)."""
    adds, base = [], e
    while base.decl().kind() == z3.Z3_OP_STORE:
        adds.append((base.arg(1), base.arg(2)))
        base = base.arg(0)
    adds.reverse()
    is_set = e.sort().range().kind() == z3.Z3_BOOL_SORT

    if not is_set:                                  # MAP literal
        pairs = [f"{_p(i, b, 0)[0]} ↦ {_p(v, b, 0)[0]}" for i, v in adds]
        if base.decl().kind() == z3.Z3_OP_CONST_ARRAY:
            pairs.append(f"_ ↦ {_p(base.arg(0), b, 0)[0]}")
            return "{" + ", ".join(pairs) + "}", True
        return _p(base, b, P_ATOM)[0] + " with {" + ", ".join(pairs) + "}", False

    if base.decl().kind() == z3.Z3_OP_CONST_ARRAY:  # SET
        seed = "∅" if _is_false(base.arg(0)) else "U"
        seed_braced = seed == "∅"
    else:
        seed, seed_braced = _p(base, b, P_SETOP)[0], False

    pos = [i for i, v in adds if _is_true(v)]
    neg = [i for i, v in adds if _is_false(v)]
    other = [(i, v) for i, v in adds if not (_is_true(v) or _is_false(v))]
    if not other and seed_braced and not neg:
        return "{" + ", ".join(_p(i, b, 0)[0] for i in pos) + "}", True
    txt = seed
    if pos:
        txt += " ∪ {" + ", ".join(_p(i, b, 0)[0] for i in pos) + "}"
    if neg:
        txt += " ∖ {" + ", ".join(_p(i, b, 0)[0] for i in neg) + "}"
    for i, v in other:
        txt += f" with [{_p(i, b, 0)[0]} ↦ {_p(v, b, 0)[0]}]"
    return txt, False


# ── pattern recognizers (return rendered text or None) ────────────────────────
def _try_membership(e, b):
    """Bool-sorted select(S, key) → 'key ∈ {set}'."""
    if e.decl().kind() == z3.Z3_OP_SELECT and e.sort().kind() == z3.Z3_BOOL_SORT:
        S, key = e.arg(0), e.arg(1)
        return f"{_p(key, b, P_CMP)[0]} ∈ {_render_array(S, b)[0]}"
    return None


def _try_tuple_set(e, b):
    """(or (and (= x a) (= y c)) (and (= x d) (= y f)) …) → (x, y) ∈ {(a,c), …}.

    The post-blast_select_store form of a set membership. Requires every disjunct
    to be an equality, or a conjunction of equalities, over the SAME lhs vars."""
    if e.decl().kind() != z3.Z3_OP_OR or e.num_args() < 2:
        return None
    rows, keyset = [], None
    for d in e.children():
        eqs = list(d.children()) if d.decl().kind() == z3.Z3_OP_AND else [d]
        cols, vals = [], []
        for eq in eqs:
            if eq.decl().kind() != z3.Z3_OP_EQ:
                return None
            a0, a1 = eq.arg(0), eq.arg(1)
            # orient: variable side = key, value side = literal
            var, lit = (a0, a1) if z3.is_const(a1) and _is_lit(a1) else (a1, a0)
            cols.append(var.sexpr()); vals.append(lit)
        ks = tuple(cols)
        if keyset is None:
            keyset = ks
        elif ks != keyset:
            return None
        rows.append(vals)
    keytxt = ", ".join(keyset)
    keytxt = keytxt if len(keyset) == 1 else f"({keytxt})"
    body = ", ".join(("(" + ", ".join(_p(v, b, 0)[0] for v in r) + ")")
                     if len(r) > 1 else _p(r[0], b, 0)[0] for r in rows)
    return f"{keytxt} ∈ {{{body}}}"


def _is_lit(e):
    return (z3.is_int_value(e) or z3.is_rational_value(e) or z3.is_bv_value(e)
            or _is_true(e) or _is_false(e))


_NEGCMP = {z3.Z3_OP_LE: ">", z3.Z3_OP_LT: "≥", z3.Z3_OP_GE: "<", z3.Z3_OP_GT: "≤",
           z3.Z3_OP_EQ: "≠", z3.Z3_OP_ULEQ: ">", z3.Z3_OP_ULT: "≥",
           z3.Z3_OP_UGEQ: "<", z3.Z3_OP_UGT: "≤"}


def _try_negcmp(e, b):
    """¬(a ≤ b) → a > b — flip a negated comparison to read cleanly."""
    if e.decl().kind() == z3.Z3_OP_NOT:
        c = e.arg(0)
        if c.decl().kind() in _NEGCMP and c.num_args() == 2:
            return (f"{_p(c.arg(0), b, P_CMP)[0]} {_NEGCMP[c.decl().kind()]} "
                    f"{_p(c.arg(1), b, P_CMP)[0]}")
    return None


def _try_range(e, b):
    """(and (≤ lo x) (< x hi)) on a shared middle term → 'lo ≤ x < hi'."""
    if e.decl().kind() != z3.Z3_OP_AND or e.num_args() != 2:
        return None
    lo, hi = e.arg(0), e.arg(1)
    cmps = {z3.Z3_OP_LE: "≤", z3.Z3_OP_LT: "<", z3.Z3_OP_GE: "≥", z3.Z3_OP_GT: ">",
            z3.Z3_OP_ULEQ: "≤", z3.Z3_OP_ULT: "<"}
    if lo.decl().kind() not in cmps or hi.decl().kind() not in cmps:
        return None
    if lo.num_args() != 2 or hi.num_args() != 2:
        return None
    mid = lo.arg(1)
    if mid.sexpr() != hi.arg(0).sexpr():
        return None
    o1, o2 = cmps[lo.decl().kind()], cmps[hi.decl().kind()]
    return (f"{_p(lo.arg(0), b, P_CMP)[0]} {o1} {_p(mid, b, P_CMP)[0]} "
            f"{o2} {_p(hi.arg(1), b, P_CMP)[0]}")


def _try_match(e, b):
    """if k=0 then a else if k=1 then b … → match k { 0 ⇒ a, 1 ⇒ b, _ ⇒ d }.

    Only when the chain length ≥ 2 and every test is `same_var = literal`."""
    arms, cur, var = [], e, None
    while cur.decl().kind() == z3.Z3_OP_ITE:
        cond, then, els = cur.arg(0), cur.arg(1), cur.arg(2)
        if cond.decl().kind() != z3.Z3_OP_EQ:
            break
        a0, a1 = cond.arg(0), cond.arg(1)
        v, lit = (a0, a1) if _is_lit(a1) else (a1, a0)
        if not _is_lit(lit):
            break
        if var is None:
            var = v.sexpr()
        elif v.sexpr() != var:
            break
        arms.append((lit, then))
        cur = els
    if len(arms) < 2:
        return None
    arm_txt = " | ".join(f"{_p(lit, b, 0)[0]} ⇒ {_p(val, b, 0)[0]}" for lit, val in arms)
    return f"match {var} {{ {arm_txt} | _ ⇒ {_p(cur, b, 0)[0]} }}"


# ── shared-subterm naming (the Z3 AST is a DAG; without this it blows up) ──────
_NAMES = {}            # node id → 'sN' for shared, non-trivial subterms


def _collect(items):
    ref, order = {}, []
    stack = list(items)
    while stack:
        e = stack.pop()
        i = e.get_id()
        ref[i] = ref.get(i, 0) + 1
        if ref[i] > 1:
            continue
        order.append(e)
        if z3.is_app(e):                    # quantifiers are opaque: don't hoist
            stack.extend(e.children())       # shared subterms across binders
    return ref, order


def _render_def(e):
    """Render a node ignoring its own binding name (used at its definition)."""
    i = e.get_id()
    saved = _NAMES.pop(i, None)
    txt = _p(e, [], 0)[0]
    if saved is not None:
        _NAMES[i] = saved
    return txt


def _render_top(items):
    global _NAMES
    ref, order = _collect(items)
    _NAMES, n = {}, 0
    for e in order:
        i = e.get_id()
        if ref[i] >= 2 and z3.is_app(e) and e.num_args() > 0 and not _is_lit(e):
            n += 1
            _NAMES[i] = f"s{n}"
    body = [_render_def(it) for it in items]
    shared = [e for e in order if e.get_id() in _NAMES]
    if shared:
        body.append("where")
        for e in reversed(shared):          # deepest-first
            body.append(f"  {_NAMES[e.get_id()]} = {_render_def(e)}")
    return "\n".join(body)


# ── core renderer: returns (text, precedence) ─────────────────────────────────
def _p(e, b, need=0):
    if not b:                               # only at top level (no active binders)
        nm = _NAMES.get(e.get_id())
        if nm is not None:
            return nm, P_ATOM
    # quantifiers
    if z3.is_quantifier(e):
        kind = "∀" if e.is_forall() else "∃"
        names = [e.var_name(i) for i in range(e.num_vars())]
        nb = b + list(reversed(names))                 # de Bruijn: var 0 = innermost
        body = _p(e.body(), nb, 0)[0]
        head = ", ".join(names)
        return _wrap(f"{kind} {head} : {body}", P_QUANT, need), P_QUANT

    # bound variable (de Bruijn index into binder stack)
    if z3.is_var(e):
        idx = z3.get_var_index(e)
        name = b[-1 - idx] if idx < len(b) else f"?{idx}"
        return name, P_ATOM

    if not z3.is_app(e):
        return str(e), P_ATOM

    k = e.decl().kind()

    # literals / constants
    if _is_true(e):
        return "true", P_ATOM
    if _is_false(e):
        return "false", P_ATOM
    if z3.is_int_value(e) or z3.is_bv_value(e):
        return str(e.as_long()), P_ATOM
    if z3.is_rational_value(e):
        return str(e), P_ATOM
    if z3.is_string_value(e):
        return '"' + e.as_string() + '"', P_ATOM
    if k == z3.Z3_OP_CONST_ARRAY:
        return _render_array(e, b)[0], P_ATOM
    if e.num_args() == 0:
        return e.decl().name(), P_ATOM

    # recognizers first (most specific)
    for rec, prec in ((_try_range, P_CMP), (_try_membership, P_CMP),
                      (_try_tuple_set, P_CMP), (_try_negcmp, P_CMP),
                      (_try_match, P_ATOM)):
        out = rec(e, b)
        if out is not None:
            return _wrap(out, prec, need), prec

    # logical
    if k == z3.Z3_OP_NOT:
        return _wrap("¬" + _p(e.arg(0), b, P_NOT)[0], P_NOT, need), P_NOT
    if k == z3.Z3_OP_AND:
        s = " ∧ ".join(_p(c, b, P_AND)[0] for c in e.children())
        return _wrap(s, P_AND, need), P_AND
    if k == z3.Z3_OP_OR:
        s = " ∨ ".join(_p(c, b, P_OR)[0] for c in e.children())
        return _wrap(s, P_OR, need), P_OR
    if k == z3.Z3_OP_IMPLIES:
        a, c = _p(e.arg(0), b, P_IMP + 1)[0], _p(e.arg(1), b, P_IMP)[0]
        return _wrap(f"{a} ⇒ {c}", P_IMP, need), P_IMP

    # equality / distinct
    if k == z3.Z3_OP_DISTINCT:
        cs = [_p(c, b, P_CMP)[0] for c in e.children()]
        s = " ≠ ".join(cs) if len(cs) == 2 else "distinct{" + ", ".join(cs) + "}"
        return _wrap(s, P_CMP, need), P_CMP

    # infix binops (arith, comparisons, bv)
    if k in _BIN:
        op, prec = _BIN[k]
        cs = [_p(c, b, prec + (1 if i else 0))[0] for i, c in enumerate(e.children())]
        return _wrap(f" {op} ".join(cs), prec, need), prec
    if k == z3.Z3_OP_UMINUS:
        return _wrap("−" + _p(e.arg(0), b, P_NOT)[0], P_NOT, need), P_NOT

    # pseudo-boolean cardinality
    if k in (z3.Z3_OP_PB_AT_MOST, z3.Z3_OP_PB_AT_LEAST):
        op = "≤" if k == z3.Z3_OP_PB_AT_MOST else "≥"
        bound = e.decl().params()[0] if e.decl().params() else "?"
        elems = ", ".join(_p(c, b, 0)[0] for c in e.children())
        return _wrap(f"#{{{elems}}} {op} {bound}", P_CMP, need), P_CMP

    # sequence / string forms
    if k == z3.Z3_OP_SEQ_LENGTH:
        return f"#{_p(e.arg(0), b, P_ATOM)[0]}", P_ATOM
    if k == z3.Z3_OP_SEQ_UNIT:
        return f"⟨{_p(e.arg(0), b, 0)[0]}⟩", P_ATOM

    # arrays / sets that weren't caught as membership
    if k == z3.Z3_OP_SELECT:
        return f"{_p(e.arg(0), b, P_ATOM)[0]}[{_p(e.arg(1), b, 0)[0]}]", P_ATOM
    if k == z3.Z3_OP_STORE:
        return _render_array(e, b)[0], P_SETOP

    # ite (single, not a chain — chain handled by _try_match)
    if k == z3.Z3_OP_ITE:
        c, t, f = (_p(x, b, 0)[0] for x in e.children())
        return _wrap(f"if {c} then {t} else {f}", P_QUANT, need), P_QUANT

    # tuple constructor → (a, b); other datatype ctor/app → Name(a, b)
    if _is_tuple_ctor(e):
        return "(" + ", ".join(_p(c, b, 0)[0] for c in e.children()) + ")", P_ATOM

    # generic application, with seq/string names cleaned up
    name = e.decl().name()
    if name in ("seq.len", "str.len"):
        return f"#{_p(e.arg(0), b, P_ATOM)[0]}", P_ATOM
    name = _CLEAN_NAME.get(name, name)
    return f"{name}(" + ", ".join(_p(c, b, 0)[0] for c in e.children()) + ")", P_ATOM


_CLEAN_NAME = {
    "seq.contains": "contains", "str.contains": "contains",
    "str.suffixof": "suffix_of", "str.prefixof": "prefix_of",
    "seq.++": "++", "str.++": "++", "seq.at": "at", "str.at": "at",
    "str.substr": "substr", "seq.extract": "extract", "str.indexof": "index_of",
}


# ── public API ────────────────────────────────────────────────────────────────
def expr(e):
    """Pretty-print a single Z3 expression (shared subterms → `where` bindings)."""
    return _render_top([e])


def goal(g):
    """Pretty-print a Goal/Solver as a constraint list (one assertion per line);
    subterms shared across assertions are hoisted into a trailing `where` block."""
    items = [g[i] for i in range(len(g))] if hasattr(g, "__len__") else list(g)
    if not items:
        return "(empty — trivially SAT)"
    return _render_top(items)
