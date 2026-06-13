"""A FAITHFUL Z3-AST pretty-printer.

This is NOT a set-theory recognizer. It renders the Z3 AST exactly as it is —
one node, one rendering — just in readable math symbols instead of smt2/Z3-AST
syntax. If the model uses `select`/`store`, you see `select`/`store`. If a tactic
rewrote a set membership into a disjunction of equalities, you see that
disjunction. The goal is to read *what Z3 actually has* without parsing sexprs,
so the effect of a tactic is visible as the structural change it really is.

The only structural liberty taken is shared-subterm naming: the Z3 AST is a DAG,
so any subterm reached more than once is hoisted into a trailing `where` block of
`sN` bindings — this is exactly what Z3's own `let`-printing does, and it both
keeps output linear and makes sharing legible. Nothing is merged or reinterpreted.

Symbol key (each maps to ONE Z3 op, no inference):
  ∧ ∨ ¬ ⇒ ⇔ ⊕   and or not implies iff xor
  = distinct      eq / n-ary distinct        ≤ < ≥ >   (ᵤ subscript = unsigned BV)
  + − · / ÷ mod   arith                        A[i]      select(A, i)
  A[i ↦ v]        store(A, i, v)               const(v)  constant array (K)
  ∪ ∩ ∖ ⊆ ᶜ      Z3 set ops (when present as set ops, not arrays)
  if … then … else  ite                        ⟨x⟩       seq.unit
"""
import sys
import z3

sys.setrecursionlimit(40000)   # backstop; deep store/ite spines are iterated below

# ── precedence (higher binds tighter; children below threshold get parens) ────
P_ATOM, P_SETOP, P_MUL, P_ADD, P_CMP, P_NOT, P_AND, P_OR, P_IMP, P_QUANT = \
    100, 85, 80, 70, 60, 55, 50, 40, 30, 20

_BIN = {
    z3.Z3_OP_ADD: ("+", P_ADD), z3.Z3_OP_SUB: ("−", P_ADD),
    z3.Z3_OP_MUL: ("·", P_MUL), z3.Z3_OP_DIV: ("/", P_MUL),
    z3.Z3_OP_IDIV: ("÷", P_MUL), z3.Z3_OP_MOD: ("mod", P_MUL),
    z3.Z3_OP_REM: ("rem", P_MUL), z3.Z3_OP_POWER: ("^", P_MUL),
    z3.Z3_OP_LE: ("≤", P_CMP), z3.Z3_OP_LT: ("<", P_CMP),
    z3.Z3_OP_GE: ("≥", P_CMP), z3.Z3_OP_GT: (">", P_CMP),
    z3.Z3_OP_EQ: ("=", P_CMP),
    z3.Z3_OP_IFF: ("⇔", P_IMP), z3.Z3_OP_XOR: ("⊕", P_OR),
    # bitvector arithmetic / comparisons (signedness preserved: ᵤ = unsigned)
    z3.Z3_OP_BADD: ("+", P_ADD), z3.Z3_OP_BSUB: ("−", P_ADD),
    z3.Z3_OP_BMUL: ("·", P_MUL), z3.Z3_OP_BUDIV: ("/ᵤ", P_MUL),
    z3.Z3_OP_BSDIV: ("/", P_MUL), z3.Z3_OP_BAND: ("&", P_MUL),
    z3.Z3_OP_BOR: ("|", P_ADD), z3.Z3_OP_BXOR: ("⊕", P_ADD),
    z3.Z3_OP_ULEQ: ("≤ᵤ", P_CMP), z3.Z3_OP_ULT: ("<ᵤ", P_CMP),
    z3.Z3_OP_UGEQ: ("≥ᵤ", P_CMP), z3.Z3_OP_UGT: (">ᵤ", P_CMP),
    z3.Z3_OP_SLEQ: ("≤", P_CMP), z3.Z3_OP_SLT: ("<", P_CMP),
    z3.Z3_OP_SGEQ: ("≥", P_CMP), z3.Z3_OP_SGT: (">", P_CMP),
}

# Z3 set ops, rendered with set symbols ONLY when the AST genuinely uses them
# (they are distinct op kinds; this is faithful, not inference).
_SET_BIN = {
    z3.Z3_OP_SET_UNION: ("∪", P_SETOP), z3.Z3_OP_SET_INTERSECT: ("∩", P_SETOP),
    z3.Z3_OP_SET_DIFFERENCE: ("∖", P_SETOP), z3.Z3_OP_SET_SUBSET: ("⊆", P_CMP),
}

_CLEAN_NAME = {
    "seq.contains": "contains", "str.contains": "contains",
    "str.suffixof": "suffix_of", "str.prefixof": "prefix_of",
    "seq.++": "++", "str.++": "++", "seq.at": "at", "str.at": "at",
    "str.substr": "substr", "seq.extract": "extract", "str.indexof": "index_of",
    "seq.nth": "nth",
}


def _wrap(txt, prec, need):
    return f"({txt})" if prec < need else txt


def _is_lit(e):
    return (z3.is_int_value(e) or z3.is_rational_value(e) or z3.is_bv_value(e)
            or z3.is_true(e) or z3.is_false(e) or z3.is_string_value(e))


def _is_tuple_ctor(e):
    """A datatype constructor of a single-constructor sort → render as a tuple.
    Faithful: a tuple IS that constructor; (a, b) is just notation for it."""
    if e.decl().kind() != z3.Z3_OP_DT_CONSTRUCTOR or e.num_args() == 0:
        return False
    s = e.sort()
    return isinstance(s, z3.DatatypeSortRef) and s.num_constructors() == 1


# ── shared-subterm naming (the Z3 AST is a DAG; Z3's own printer uses `let`) ───
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


# ── core renderer: returns (text, precedence). ONE node → ONE rendering. ───────
def _p(e, b, need=0):
    if not b:                               # only at top level (no active binders)
        nm = _NAMES.get(e.get_id())
        if nm is not None:
            return nm, P_ATOM

    # quantifiers
    if z3.is_quantifier(e):
        kind = "∀" if e.is_forall() else "∃"
        names = [e.var_name(i) for i in range(e.num_vars())]
        nb = b + list(reversed(names))      # de Bruijn: var 0 = innermost
        body = _p(e.body(), nb, 0)[0]
        return _wrap(f"{kind} {', '.join(names)} : {body}", P_QUANT, need), P_QUANT

    # bound variable (de Bruijn index into binder stack)
    if z3.is_var(e):
        idx = z3.get_var_index(e)
        return (b[-1 - idx] if idx < len(b) else f"?{idx}"), P_ATOM

    if not z3.is_app(e):
        return str(e), P_ATOM

    k = e.decl().kind()

    # literals / constants
    if z3.is_true(e):
        return "true", P_ATOM
    if z3.is_false(e):
        return "false", P_ATOM
    if z3.is_int_value(e) or z3.is_bv_value(e):
        return str(e.as_long()), P_ATOM
    if z3.is_rational_value(e):
        return str(e), P_ATOM
    if z3.is_string_value(e):
        return '"' + e.as_string() + '"', P_ATOM
    if k == z3.Z3_OP_CONST_ARRAY:
        return f"const({_p(e.arg(0), b, 0)[0]})", P_ATOM
    if e.num_args() == 0:
        return e.decl().name(), P_ATOM

    # ── logical ──
    if k == z3.Z3_OP_NOT:
        return _wrap("¬" + _p(e.arg(0), b, P_ATOM)[0], P_NOT, need), P_NOT
    if k == z3.Z3_OP_AND:
        return _wrap(" ∧ ".join(_p(c, b, P_AND)[0] for c in e.children()), P_AND, need), P_AND
    if k == z3.Z3_OP_OR:                    # wrap ∧-groups for legibility (faithful)
        return _wrap(" ∨ ".join(_p(c, b, P_AND + 1)[0] for c in e.children()), P_OR, need), P_OR
    if k == z3.Z3_OP_IMPLIES:
        a, c = _p(e.arg(0), b, P_IMP + 1)[0], _p(e.arg(1), b, P_IMP)[0]
        return _wrap(f"{a} ⇒ {c}", P_IMP, need), P_IMP
    if k == z3.Z3_OP_DISTINCT:
        return _wrap("distinct(" + ", ".join(_p(c, b, 0)[0]
                     for c in e.children()) + ")", P_ATOM, need), P_ATOM

    # ── infix arithmetic / comparison / bitvector ──
    if k in _BIN:
        op, prec = _BIN[k]
        cs = [_p(c, b, prec + (1 if i else 0))[0] for i, c in enumerate(e.children())]
        return _wrap(f" {op} ".join(cs), prec, need), prec
    if k in (z3.Z3_OP_UMINUS, z3.Z3_OP_BNEG):
        return _wrap("−" + _p(e.arg(0), b, P_ATOM)[0], P_NOT, need), P_NOT
    if k == z3.Z3_OP_BNOT:
        return _wrap("~" + _p(e.arg(0), b, P_ATOM)[0], P_NOT, need), P_NOT

    # ── arrays (faithful: select / store / const) ──
    if k == z3.Z3_OP_SELECT:
        idx = ", ".join(_p(c, b, 0)[0] for c in e.children()[1:])
        return f"{_p(e.arg(0), b, P_ATOM)[0]}[{idx}]", P_ATOM
    if k == z3.Z3_OP_STORE:                 # iterate the spine (can be 1000s deep)
        updates, cur = [], e
        while cur.decl().kind() == z3.Z3_OP_STORE and not (
                not b and cur.get_id() in _NAMES):
            ch = cur.children()
            idx = ", ".join(_p(c, b, 0)[0] for c in ch[1:-1])
            updates.append(f"[{idx} ↦ {_p(ch[-1], b, 0)[0]}]")
            cur = ch[0]
        return _p(cur, b, P_ATOM)[0] + "".join(reversed(updates)), P_ATOM

    # ── genuine Z3 set ops (distinct AST kinds, so faithful to render as sets) ──
    if k in _SET_BIN:
        op, prec = _SET_BIN[k]
        cs = [_p(c, b, prec + 1)[0] for c in e.children()]
        return _wrap(f" {op} ".join(cs), prec, need), prec
    if k == z3.Z3_OP_SET_COMPLEMENT:
        return _p(e.arg(0), b, P_ATOM)[0] + "ᶜ", P_ATOM
    if k == z3.Z3_OP_SET_CARD:
        return f"#{_p(e.arg(0), b, P_ATOM)[0]}", P_ATOM

    # ── ite (faithful: nested if/then/else, NOT collapsed to match) ──
    if k == z3.Z3_OP_ITE:                   # iterate the else-spine (can be deep)
        arms, cur = [], e
        while cur.decl().kind() == z3.Z3_OP_ITE and not (
                not b and cur.get_id() in _NAMES):
            c, t, els = cur.children()
            arms.append((_p(c, b, 0)[0], _p(t, b, 0)[0]))
            cur = els
        s = _p(cur, b, 0)[0]
        for c, t in reversed(arms):
            s = f"if {c} then {t} else {s}"
        return _wrap(s, P_QUANT, need), P_QUANT

    # ── pseudo-boolean (faithful: the op + its bound) ──
    if k in (z3.Z3_OP_PB_AT_MOST, z3.Z3_OP_PB_AT_LEAST):
        nm = "at_most" if k == z3.Z3_OP_PB_AT_MOST else "at_least"
        bound = e.decl().params()[0] if e.decl().params() else "?"
        elems = ", ".join(_p(c, b, 0)[0] for c in e.children())
        return f"{nm}({bound}; {elems})", P_ATOM

    # ── array map: (_ map f) a b … ──
    if k == z3.Z3_OP_ARRAY_MAP:
        ps = e.decl().params()
        fn = ps[0].name() if ps and hasattr(ps[0], "name") else "?"
        return f"map[{fn}](" + ", ".join(_p(c, b, 0)[0] for c in e.children()) + ")", P_ATOM

    # ── sequence / string ──
    if k == z3.Z3_OP_SEQ_UNIT:
        return f"⟨{_p(e.arg(0), b, 0)[0]}⟩", P_ATOM

    # ── tuple constructor → (a, b); else generic application ──
    if _is_tuple_ctor(e):
        return "(" + ", ".join(_p(c, b, 0)[0] for c in e.children()) + ")", P_ATOM
    name = e.decl().name()
    if name in ("seq.len", "str.len"):
        return f"len({_p(e.arg(0), b, 0)[0]})", P_ATOM
    name = _CLEAN_NAME.get(name, name)
    return f"{name}(" + ", ".join(_p(c, b, 0)[0] for c in e.children()) + ")", P_ATOM


# ── public API ────────────────────────────────────────────────────────────────
def expr(e):
    """Faithfully pretty-print a single Z3 expression (DAG sharing → `where`)."""
    return _render_top([e])


def goal(g):
    """Faithfully pretty-print a Goal/Solver as a constraint list (one assertion
    per line); subterms shared across assertions hoist into a `where` block."""
    items = [g[i] for i in range(len(g))] if hasattr(g, "__len__") else list(g)
    if not items:
        return "(empty — trivially true / SAT)"
    return _render_top(items)
