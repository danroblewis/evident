"""A FAITHFUL, width-aware Z3-AST pretty-printer.

This is NOT a set-theory recognizer. It renders the Z3 AST exactly as it is —
one node, one rendering — just in readable math symbols instead of smt2/Z3-AST
syntax. If the model uses `select`/`store`, you see `select`/`store`. If a tactic
rewrote a set membership into a disjunction of equalities, you see that
disjunction. The goal is to read *what Z3 actually has* without parsing sexprs,
so the effect of a tactic is visible as the structural change it really is.

Layout: a Wadler/Leijen document model (Text/Line/Nest/Group) laid out at a
target WIDTH — exactly how Z3's own printer decides line breaks. A subexpression
that fits on the current line stays inline; one that doesn't breaks, with its
parts indented. So short constraints are one line; long conjunctions /
disjunctions / store-chains break and indent. Nothing is merged or reinterpreted.

The only structural liberty is shared-subterm naming: the Z3 AST is a DAG, so any
subterm reached more than once is hoisted into a trailing `where` block of `sN`
bindings — exactly what Z3's `let`-printing does; it keeps output linear and
makes sharing legible.

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

WIDTH = 80                     # target line width for layout

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

# Z3 set ops, rendered with set symbols ONLY when the AST genuinely uses them.
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

# ── document model (Wadler/Leijen) ────────────────────────────────────────────
# Docs are tagged tuples: ('T', s) text · ('C', [docs]) concat · ('N', i, d) nest
# · ('G', d) group (flatten if it fits, else break) · ('L', flat) soft line (uses
# `flat` between items when flattened, newline+indent when broken) · ('H',) hard
# line (always a newline). A "string width" uses len() — fine for our symbols.


def _T(s):
    return ("T", s)


def _C(*ds):
    return ("C", list(ds))


def _N(i, d):
    return ("N", i, d)


def _G(d):
    return ("G", d)


def _L(flat=" "):
    return ("L", flat)


_HARD = ("H",)


def _fits(remaining, doc):
    stack = [doc]
    while remaining >= 0:
        if not stack:
            return True
        t = stack.pop()
        tag = t[0]
        if tag == "T":
            remaining -= len(t[1])
        elif tag == "C":
            stack.extend(reversed(t[1]))
        elif tag == "N":
            stack.append(t[2])
        elif tag == "G":
            stack.append(t[1])
        elif tag == "L":
            remaining -= len(t[1])
        elif tag == "H":
            return False              # a hard break can't flatten → group must break
    return False


def _layout(doc, width):
    out, col = [], 0
    stack = [(0, False, doc)]          # (indent, flat?, doc)
    while stack:
        ind, flat, d = stack.pop()
        tag = d[0]
        if tag == "T":
            out.append(d[1]); col += len(d[1])
        elif tag == "C":
            for s in reversed(d[1]):
                stack.append((ind, flat, s))
        elif tag == "N":
            stack.append((ind + d[1], flat, d[2]))
        elif tag == "L":
            if flat:
                out.append(d[1]); col += len(d[1])
            else:
                out.append("\n" + " " * ind); col = ind
        elif tag == "H":
            out.append("\n" + " " * ind); col = ind
        elif tag == "G":
            stack.append((ind, _fits(width - col, d[1]), d[1]))
    return "".join(out)


def _wrap(doc, prec, need):
    return _C(_T("("), doc, _T(")")) if prec < need else doc


def _infix(op, docs, prec):
    """Leading-operator layout: a / op b / op c — breaks before each operator
    only when the line overflows (soft). Used for ∨, arithmetic, comparisons."""
    parts = [docs[0]]
    for d in docs[1:]:
        parts += [_L(" "), _T(op + " "), d]
    return _G(_N(2, _C(*parts)))


def _infix_hard(op, docs):
    """Leading-operator layout that ALWAYS breaks before each operator. Used for
    ∧: a conjunction is a list of separate requirements, one per line — the
    operands align (no extra indent) at whatever indent the conjunction sits at."""
    parts = [docs[0]]
    for d in docs[1:]:
        parts += [_HARD, _T(op + " "), d]
    return _C(*parts)


def _call(name, arg_docs):
    if not arg_docs:
        return _T(name + "()")
    parts = [arg_docs[0]]
    for d in arg_docs[1:]:
        parts += [_T(","), _L(" "), d]
    return _G(_C(_T(name + "("), _N(len(name) + 1, _C(*parts)), _T(")")))


# ── helpers ───────────────────────────────────────────────────────────────────
def _is_lit(e):
    return (z3.is_int_value(e) or z3.is_rational_value(e) or z3.is_bv_value(e)
            or z3.is_true(e) or z3.is_false(e) or z3.is_string_value(e))


def _is_tuple_ctor(e):
    if e.decl().kind() != z3.Z3_OP_DT_CONSTRUCTOR or e.num_args() == 0:
        return False
    s = e.sort()
    return isinstance(s, z3.DatatypeSortRef) and s.num_constructors() == 1


# ── shared-subterm naming (DAG → `where`; same as Z3's let-printing) ──────────
_NAMES = {}


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
        if z3.is_app(e):
            stack.extend(e.children())
    return ref, order


def _def_doc(e):
    i = e.get_id()
    saved = _NAMES.pop(i, None)
    d = _p(e, [], 0)[0]
    if saved is not None:
        _NAMES[i] = saved
    return d


def _render_top(items, width):
    global _NAMES
    ref, order = _collect(items)
    _NAMES, n = {}, 0
    for e in order:
        i = e.get_id()
        if ref[i] >= 2 and z3.is_app(e) and e.num_args() > 0 and not _is_lit(e):
            n += 1
            _NAMES[i] = f"s{n}"
    parts = []
    for j, it in enumerate(items):
        if j:
            parts.append(_HARD)
        parts.append(_def_doc(it))
    shared = [e for e in order if e.get_id() in _NAMES]
    if shared:
        parts += [_HARD, _T("where")]
        for e in reversed(shared):                 # deepest-first
            nm = _NAMES[e.get_id()]
            parts += [_HARD, _C(_T(f"  {nm} = "), _N(4, _def_doc(e)))]
    return _layout(_C(*parts), width)


# ── core renderer: returns (doc, precedence). ONE node → ONE rendering. ────────
def _p(e, b, need=0):
    if not b:                                       # top level (no active binders)
        nm = _NAMES.get(e.get_id())
        if nm is not None:
            return _T(nm), P_ATOM

    if z3.is_quantifier(e):
        kind = "∀" if e.is_forall() else "∃"
        names = [e.var_name(i) for i in range(e.num_vars())]
        nb = b + list(reversed(names))
        body = _p(e.body(), nb, 0)[0]
        doc = _G(_C(_T(f"{kind} {', '.join(names)} :"), _N(2, _C(_L(" "), body))))
        return _wrap(doc, P_QUANT, need), P_QUANT

    if z3.is_var(e):
        idx = z3.get_var_index(e)
        return _T(b[-1 - idx] if idx < len(b) else f"?{idx}"), P_ATOM

    if not z3.is_app(e):
        return _T(str(e)), P_ATOM

    k = e.decl().kind()

    # literals / constants
    if z3.is_true(e):
        return _T("true"), P_ATOM
    if z3.is_false(e):
        return _T("false"), P_ATOM
    if z3.is_int_value(e) or z3.is_bv_value(e):
        return _T(str(e.as_long())), P_ATOM
    if z3.is_rational_value(e):
        return _T(str(e)), P_ATOM
    if z3.is_string_value(e):
        return _T('"' + e.as_string() + '"'), P_ATOM
    if k == z3.Z3_OP_CONST_ARRAY:
        return _C(_T("const("), _p(e.arg(0), b, 0)[0], _T(")")), P_ATOM
    if e.num_args() == 0:
        return _T(e.decl().name()), P_ATOM

    # logical
    if k == z3.Z3_OP_NOT:
        return _wrap(_C(_T("¬"), _p(e.arg(0), b, P_ATOM)[0]), P_NOT, need), P_NOT
    if k == z3.Z3_OP_AND:                            # ∧ always breaks (one req/line)
        ds = [_p(c, b, P_AND)[0] for c in e.children()]
        return _wrap(_infix_hard("∧", ds), P_AND, need), P_AND
    if k == z3.Z3_OP_OR:
        ds = [_p(c, b, P_AND + 1)[0] for c in e.children()]
        return _wrap(_infix("∨", ds, P_OR), P_OR, need), P_OR
    if k == z3.Z3_OP_IMPLIES:
        a, c = _p(e.arg(0), b, P_IMP + 1)[0], _p(e.arg(1), b, P_IMP)[0]
        doc = _G(_N(2, _C(a, _L(" "), _T("⇒ "), c)))
        return _wrap(doc, P_IMP, need), P_IMP
    if k == z3.Z3_OP_DISTINCT:
        return _wrap(_call("distinct", [_p(c, b, 0)[0] for c in e.children()]),
                     P_ATOM, need), P_ATOM

    # infix arithmetic / comparison / bitvector
    if k in _BIN:
        op, prec = _BIN[k]
        ds = [_p(c, b, prec + (1 if i else 0))[0] for i, c in enumerate(e.children())]
        return _wrap(_infix(op, ds, prec), prec, need), prec
    if k in (z3.Z3_OP_UMINUS, z3.Z3_OP_BNEG):
        return _wrap(_C(_T("−"), _p(e.arg(0), b, P_ATOM)[0]), P_NOT, need), P_NOT
    if k == z3.Z3_OP_BNOT:
        return _wrap(_C(_T("~"), _p(e.arg(0), b, P_ATOM)[0]), P_NOT, need), P_NOT

    # arrays (faithful: select / store / const)
    if k == z3.Z3_OP_SELECT:
        idx = _commas(e.children()[1:], b)
        return _C(_p(e.arg(0), b, P_ATOM)[0], _T("["), idx, _T("]")), P_ATOM
    if k == z3.Z3_OP_STORE:                         # iterate the spine (can be deep)
        updates, cur = [], e
        while cur.decl().kind() == z3.Z3_OP_STORE and not (
                not b and cur.get_id() in _NAMES):
            ch = cur.children()
            upd = _C(_T("["), _commas(ch[1:-1], b), _T(" ↦ "),
                     _p(ch[-1], b, 0)[0], _T("]"))
            updates.append(upd)
            cur = ch[0]
        parts = [_p(cur, b, P_ATOM)[0]]
        for upd in reversed(updates):
            parts += [_L(""), upd]
        return _G(_N(2, _C(*parts))), P_ATOM

    # genuine Z3 set ops
    if k in _SET_BIN:
        op, prec = _SET_BIN[k]
        ds = [_p(c, b, prec + 1)[0] for c in e.children()]
        return _wrap(_infix(op, ds, prec), prec, need), prec
    if k == z3.Z3_OP_SET_COMPLEMENT:
        return _C(_p(e.arg(0), b, P_ATOM)[0], _T("ᶜ")), P_ATOM
    if k == z3.Z3_OP_SET_CARD:
        return _C(_T("#"), _p(e.arg(0), b, P_ATOM)[0]), P_ATOM

    # ite (faithful: nested if/then/else; iterate the else-spine)
    if k == z3.Z3_OP_ITE:
        arms, cur = [], e
        while cur.decl().kind() == z3.Z3_OP_ITE and not (
                not b and cur.get_id() in _NAMES):
            c, t, els = cur.children()
            arms.append((_p(c, b, 0)[0], _p(t, b, 0)[0]))
            cur = els
        else_doc = _p(cur, b, 0)[0]
        # block style: `if c then` / indented body / `else if …` / `else` / body.
        # Soft lines flatten to one line when it all fits; otherwise it blocks out.
        parts = [_T("if "), arms[0][0], _T(" then"), _N(2, _C(_L(" "), arms[0][1]))]
        for c, t in arms[1:]:
            parts += [_L(" "), _T("else if "), c, _T(" then"), _N(2, _C(_L(" "), t))]
        parts += [_L(" "), _T("else"), _N(2, _C(_L(" "), else_doc))]
        return _wrap(_G(_C(*parts)), P_QUANT, need), P_QUANT

    # pseudo-boolean
    if k in (z3.Z3_OP_PB_AT_MOST, z3.Z3_OP_PB_AT_LEAST):
        nm = "at_most" if k == z3.Z3_OP_PB_AT_MOST else "at_least"
        bound = e.decl().params()[0] if e.decl().params() else "?"
        args = [_T(f"{bound};")] + [_p(c, b, 0)[0] for c in e.children()]
        return _call(nm, args), P_ATOM

    # array map: (_ map f) a b …
    if k == z3.Z3_OP_ARRAY_MAP:
        ps = e.decl().params()
        fn = ps[0].name() if ps and hasattr(ps[0], "name") else "?"
        return _call(f"map[{fn}]", [_p(c, b, 0)[0] for c in e.children()]), P_ATOM

    # sequence / string
    if k == z3.Z3_OP_SEQ_UNIT:
        return _C(_T("⟨"), _p(e.arg(0), b, 0)[0], _T("⟩")), P_ATOM

    # tuple constructor → (a, b); else generic application
    if _is_tuple_ctor(e):
        return _G(_C(_T("("), _N(1, _commas(e.children(), b)), _T(")"))), P_ATOM
    name = e.decl().name()
    if name in ("seq.len", "str.len"):
        return _C(_T("len("), _p(e.arg(0), b, 0)[0], _T(")")), P_ATOM
    return _call(_CLEAN_NAME.get(name, name), [_p(c, b, 0)[0] for c in e.children()]), P_ATOM


def _commas(children, b):
    parts = []
    for i, c in enumerate(children):
        if i:
            parts += [_T(","), _L(" ")]
        parts.append(_p(c, b, 0)[0])
    return _C(*parts)


# ── public API ────────────────────────────────────────────────────────────────
def expr(e, width=WIDTH):
    """Faithfully pretty-print one Z3 expression, wrapped to `width`."""
    return _render_top([e], width)


def goal(g, width=WIDTH):
    """Faithfully pretty-print a Goal/Solver as a constraint list (one assertion
    per line, each wrapped to `width`); subterms shared across assertions hoist
    into a trailing `where` block."""
    if hasattr(g, "assertions"):                 # a z3.Solver
        g = g.assertions()
    items = [g[i] for i in range(len(g))] if hasattr(g, "__len__") else list(g)
    if not items:
        return "(empty — trivially true / SAT)"
    return _render_top(items, width)
