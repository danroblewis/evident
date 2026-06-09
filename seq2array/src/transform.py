"""
seq2array: a mechanical rewrite of *bounded-length* Z3 sequences into
(Array Int T) + an integer length variable.

The premise: every Seq variable `s` in a formula comes with an EXPLICIT
literal bound `(<= (seq.len s) N)`.  Because N is a concrete literal, every
sequence operation can be lowered to quantifier-free Array + Int arithmetic,
with bounded quantifiers UNROLLED into N-ary boolean combinations.  The
result lives in QF_AUFLIA (decidable, fast) instead of Z3's semi-decidable
sequence theory.

This module is a small expression IR plus two emitters:

    emit_seq(expr)    -> SMT-LIB using the Seq theory   (the "original")
    emit_array(expr)  -> SMT-LIB using Array + len      (the "rewrite")

A test is a list of top-level boolean assertions over a shared environment of
bounded seq vars.  `build_pair(...)` produces both SMT2 documents so we can
feed each to z3 and compare sat/unsat.

Element sort T is parametric (default Int).

----------------------------------------------------------------------------
Supported operations and how they lower
----------------------------------------------------------------------------
  seq.len s            -> s_len                            (a plain Int var)
  nth(s, i)            -> (select s_arr i)                 [caller guards 0<=i<len]
  unit(x)              -> SeqVal(arr=store(const,0,x), len=1)
  concat(a, b)         -> SeqVal: len = la+lb (NOT capped; bound asserted
                          separately so soundness is preserved), arr glued
                          via a chain of ite/store over a fresh array.
  member(s, x)         -> (or_{k=0..N-1} (k<len AND s_arr[k]=x))   [unrolled]
  forall_idx(s, P)     -> (and_{k=0..N-1} (k<len => P(s_arr[k])))  [unrolled]
  eq(s, t)             -> len_s=len_t AND (and_{k=0..N-1} k<len => s[k]=t[k])

Anything else (seq.extract, nested Seq, unbounded concat where la+lb has no
literal cap, regex) is reported as UNSUPPORTED rather than silently mangled.
"""

from __future__ import annotations
from dataclasses import dataclass, field
from typing import Callable


# --------------------------------------------------------------------------
# IR
# --------------------------------------------------------------------------

class Node:
    pass


# ---- integer / element scalar expressions --------------------------------

@dataclass
class IntLit(Node):
    v: int

@dataclass
class IntVar(Node):
    name: str

@dataclass
class ElemVar(Node):
    """A scalar of element sort T."""
    name: str

@dataclass
class BinArith(Node):
    op: str           # + - *
    a: Node
    b: Node

@dataclass
class SeqLen(Node):
    s: "SeqExpr"

@dataclass
class Nth(Node):
    """Element at index i.  Caller is responsible for 0<=i<len guards."""
    s: "SeqExpr"
    i: Node


# ---- boolean expressions -------------------------------------------------

@dataclass
class Cmp(Node):
    op: str           # = < <= > >=
    a: Node
    b: Node

@dataclass
class BoolOp(Node):
    op: str           # and or not =>
    args: list

@dataclass
class Member(Node):
    """exists i in [0,len). s[i] = x"""
    s: "SeqExpr"
    x: Node

@dataclass
class ForallIdx(Node):
    """forall i in [0,len). P(s[i])  -- P is a python lambda elem-term->Bool-Node"""
    s: "SeqExpr"
    pred: Callable[[Node], Node]

@dataclass
class ForallAdj(Node):
    """forall i in [0,len-1). P(s[i], s[i+1])  -- for sortedness etc."""
    s: "SeqExpr"
    pred: Callable[[Node, Node], Node]

@dataclass
class SeqEq(Node):
    a: "SeqExpr"
    b: "SeqExpr"


# ---- sequence-valued expressions -----------------------------------------

class SeqExpr(Node):
    pass

@dataclass
class SeqVar(SeqExpr):
    name: str
    bound: int        # literal N: 0 <= len <= N

@dataclass
class Unit(SeqExpr):
    x: Node

@dataclass
class Concat(SeqExpr):
    a: SeqExpr
    b: SeqExpr

@dataclass
class Empty(SeqExpr):
    pass


class Unsupported(Exception):
    pass


# --------------------------------------------------------------------------
# Environment: declares the seq vars and element sort
# --------------------------------------------------------------------------

@dataclass
class Env:
    elem_sort: str = "Int"
    seq_vars: dict = field(default_factory=dict)   # name -> bound
    int_vars: list = field(default_factory=list)
    elem_vars: list = field(default_factory=list)

    def seq(self, name, bound):
        self.seq_vars[name] = bound
        return SeqVar(name, bound)

    def intv(self, name):
        self.int_vars.append(name)
        return IntVar(name)

    def elemv(self, name):
        self.elem_vars.append(name)
        return ElemVar(name)


# --------------------------------------------------------------------------
# Static bound analysis on seq-valued expressions
# --------------------------------------------------------------------------

def seq_bound(s: SeqExpr) -> int:
    """Upper bound on len of a seq-valued expression, from literal bounds."""
    if isinstance(s, SeqVar):
        return s.bound
    if isinstance(s, Empty):
        return 0
    if isinstance(s, Unit):
        return 1
    if isinstance(s, Concat):
        return seq_bound(s.a) + seq_bound(s.b)
    raise Unsupported(f"no static bound for {type(s).__name__}")


# ==========================================================================
# EMITTER 1: Z3 Seq theory  ("original")
# ==========================================================================

class SeqEmitter:
    def __init__(self, env: Env):
        self.env = env

    def decls(self) -> list[str]:
        T = self.env.elem_sort
        out = []
        for name in self.env.seq_vars:
            out.append(f"(declare-const {name} (Seq {T}))")
        for name in self.env.int_vars:
            out.append(f"(declare-const {name} Int)")
        for name in self.env.elem_vars:
            out.append(f"(declare-const {name} {T})")
        return out

    def bound_asserts(self) -> list[str]:
        out = []
        for name, N in self.env.seq_vars.items():
            out.append(f"(assert (<= (seq.len {name}) {N}))")
        return out

    # ---- sequence-valued ----
    def seq(self, s: SeqExpr) -> str:
        if isinstance(s, SeqVar):
            return s.name
        if isinstance(s, Empty):
            return f"(as seq.empty (Seq {self.env.elem_sort}))"
        if isinstance(s, Unit):
            return f"(seq.unit {self.e(s.x)})"
        if isinstance(s, Concat):
            return f"(seq.++ {self.seq(s.a)} {self.seq(s.b)})"
        raise Unsupported(type(s).__name__)

    # ---- scalar / int / elem ----
    def e(self, n: Node) -> str:
        if isinstance(n, IntLit):
            return str(n.v) if n.v >= 0 else f"(- {-n.v})"
        if isinstance(n, IntVar):
            return n.name
        if isinstance(n, ElemVar):
            return n.name
        if isinstance(n, BinArith):
            return f"({n.op} {self.e(n.a)} {self.e(n.b)})"
        if isinstance(n, SeqLen):
            return f"(seq.len {self.seq(n.s)})"
        if isinstance(n, Nth):
            return f"(seq.nth {self.seq(n.s)} {self.e(n.i)})"
        raise Unsupported(type(n).__name__)

    # ---- boolean ----
    def b(self, n: Node) -> str:
        if isinstance(n, Cmp):
            return f"({n.op} {self.e(n.a)} {self.e(n.b)})"
        if isinstance(n, BoolOp):
            if n.op == "not":
                return f"(not {self.b(n.args[0])})"
            return f"({n.op} {' '.join(self.b(a) for a in n.args)})"
        if isinstance(n, Member):
            L = f"(seq.len {self.seq(n.s)})"
            arr = self.seq(n.s)
            return (f"(exists ((i Int)) (and (<= 0 i) (< i {L}) "
                    f"(= (seq.nth {arr} i) {self.e(n.x)})))")
        if isinstance(n, ForallIdx):
            L = f"(seq.len {self.seq(n.s)})"
            arr = self.seq(n.s)
            elem = Nth(n.s, IntVar("i"))
            body = self.b(n.pred(elem))
            return (f"(forall ((i Int)) (=> (and (<= 0 i) (< i {L})) {body}))")
        if isinstance(n, ForallAdj):
            L = f"(seq.len {self.seq(n.s)})"
            cur = Nth(n.s, IntVar("i"))
            nxt = Nth(n.s, BinArith("+", IntVar("i"), IntLit(1)))
            body = self.b(n.pred(cur, nxt))
            return (f"(forall ((i Int)) (=> (and (<= 0 i) (< i (- {L} 1))) {body}))")
        if isinstance(n, SeqEq):
            return f"(= {self.seq(n.a)} {self.seq(n.b)})"
        raise Unsupported(type(n).__name__)


# ==========================================================================
# EMITTER 2: Array + len  ("the rewrite")
# ==========================================================================

class ArrayEmitter:
    """
    Each seq-valued expression lowers to a SeqVal: an SMT array term + an SMT
    int length term + a static literal bound.  Top-level seq VARS get a fresh
    (Array Int T) const and an Int len const; compound seq values (unit,
    concat) are built as let-free inlined array/int terms.

    Bounded quantifiers are UNROLLED to the static literal bound, producing a
    quantifier-free formula in QF_AUFLIA.
    """

    def __init__(self, env: Env):
        self.env = env
        self.extra_decls: list[str] = []
        self.extra_asserts: list[str] = []
        self._gensym = 0
        # map seq var name -> (arr_term, len_term)
        self.varmap: dict[str, tuple[str, str]] = {}

    def fresh(self, prefix):
        self._gensym += 1
        return f"{prefix}{self._gensym}"

    def decls(self) -> list[str]:
        T = self.env.elem_sort
        out = []
        for name, N in self.env.seq_vars.items():
            arr = f"{name}_arr"
            ln = f"{name}_len"
            out.append(f"(declare-const {arr} (Array Int {T}))")
            out.append(f"(declare-const {ln} Int)")
            self.varmap[name] = (arr, ln)
        for name in self.env.int_vars:
            out.append(f"(declare-const {name} Int)")
        for name in self.env.elem_vars:
            out.append(f"(declare-const {name} {T})")
        return out

    def bound_asserts(self) -> list[str]:
        out = []
        for name, N in self.env.seq_vars.items():
            _, ln = self.varmap[name]
            out.append(f"(assert (and (<= 0 {ln}) (<= {ln} {N})))")
        return out

    # ---- lower a seq-valued expr to (arr_term, len_term, static_bound) ----
    def seqval(self, s: SeqExpr):
        T = self.env.elem_sort
        if isinstance(s, SeqVar):
            arr, ln = self.varmap[s.name]
            return arr, ln, s.bound
        if isinstance(s, Empty):
            arr = f"(as const (Array Int {T})) "  # placeholder; never indexed
            # use a fresh const array via 'as const' default
            arr = f"((as const (Array Int {T})) {self._default_elem()})"
            return arr, "0", 0
        if isinstance(s, Unit):
            base = f"((as const (Array Int {T})) {self._default_elem()})"
            arr = f"(store {base} 0 {self.e(s.x)})"
            return arr, "1", 1
        if isinstance(s, Concat):
            aarr, alen, abound = self.seqval(s.a)
            barr, blen, bbound = self.seqval(s.b)
            nb = abound + bbound
            # Build a fresh array C with:
            #   C[k] = aarr[k]            for 0 <= k < alen
            #   C[k] = barr[k-alen]       for alen <= k < alen+blen
            # We materialise it as a fresh declared array constrained by the
            # unrolled equations (k in 0..nb-1), which is sound because nb is a
            # static literal bound.
            carr = self.fresh("cat")
            clen = self.fresh("catlen")
            self.extra_decls.append(f"(declare-const {carr} (Array Int {T}))")
            self.extra_decls.append(f"(declare-const {clen} Int)")
            # length is exact sum (NOT capped -> soundness; the global bound
            # assertion on the *consuming* context is what enforces <= N)
            self.extra_asserts.append(f"(assert (= {clen} (+ {alen} {blen})))")
            for k in range(nb):
                # C[k] = (ite (< k alen) aarr[k] barr[k-alen])
                lhs = f"(select {carr} {k})"
                a_k = f"(select {aarr} {k})"
                b_k = f"(select {barr} (- {k} {alen}))"
                rhs = f"(ite (< {k} {alen}) {a_k} {b_k})"
                # only meaningful when k < clen
                self.extra_asserts.append(
                    f"(assert (=> (< {k} {clen}) (= {lhs} {rhs})))")
            return carr, clen, nb
        raise Unsupported(type(s).__name__)

    def _default_elem(self):
        return "0" if self.env.elem_sort == "Int" else "false"

    # ---- scalar / int / elem ----
    def e(self, n: Node) -> str:
        if isinstance(n, IntLit):
            return str(n.v) if n.v >= 0 else f"(- {-n.v})"
        if isinstance(n, IntVar):
            return n.name
        if isinstance(n, ElemVar):
            return n.name
        if isinstance(n, BinArith):
            return f"({n.op} {self.e(n.a)} {self.e(n.b)})"
        if isinstance(n, SeqLen):
            _, ln, _ = self.seqval(n.s)
            return ln
        if isinstance(n, Nth):
            arr, _, _ = self.seqval(n.s)
            return f"(select {arr} {self.e(n.i)})"
        raise Unsupported(type(n).__name__)

    # ---- boolean ----
    def b(self, n: Node) -> str:
        if isinstance(n, Cmp):
            return f"({n.op} {self.e(n.a)} {self.e(n.b)})"
        if isinstance(n, BoolOp):
            if n.op == "not":
                return f"(not {self.b(n.args[0])})"
            return f"({n.op} {' '.join(self.b(a) for a in n.args)})"
        if isinstance(n, Member):
            arr, ln, N = self.seqval(n.s)
            x = self.e(n.x)
            disj = []
            for k in range(N):
                disj.append(f"(and (< {k} {ln}) (= (select {arr} {k}) {x}))")
            if not disj:
                return "false"
            return f"(or {' '.join(disj)})"
        if isinstance(n, ForallIdx):
            arr, ln, N = self.seqval(n.s)
            conj = []
            for k in range(N):
                elem = _ArrSel(arr, k)
                conj.append(f"(=> (< {k} {ln}) {self.b(n.pred(elem))})")
            if not conj:
                return "true"
            return f"(and {' '.join(conj)})"
        if isinstance(n, ForallAdj):
            arr, ln, N = self.seqval(n.s)
            conj = []
            for k in range(N - 1):
                cur = _ArrSel(arr, k)
                nxt = _ArrSel(arr, k + 1)
                conj.append(f"(=> (< {k+1} {ln}) {self.b(n.pred(cur, nxt))})")
            if not conj:
                return "true"
            return f"(and {' '.join(conj)})"
        if isinstance(n, SeqEq):
            aarr, alen, an = self.seqval(n.a)
            barr, blen, bn = self.seqval(n.b)
            N = max(an, bn)
            parts = [f"(= {alen} {blen})"]
            for k in range(N):
                parts.append(
                    f"(=> (< {k} {alen}) (= (select {aarr} {k}) (select {barr} {k})))")
            return f"(and {' '.join(parts)})"
        raise Unsupported(type(n).__name__)


@dataclass
class _ArrSel(Node):
    """Pre-lowered array select, used to feed unrolled predicates."""
    arr: str
    idx: int


# teach both emitters to render a pre-lowered select
def _seq_e_arrsel(self, n):
    return f"(select {n.arr} {n.idx})"

# monkeypatch e() dispatch for _ArrSel
_orig_array_e = ArrayEmitter.e
def _array_e(self, n):
    if isinstance(n, _ArrSel):
        return f"(select {n.arr} {n.idx})"
    return _orig_array_e(self, n)
ArrayEmitter.e = _array_e


# --------------------------------------------------------------------------
# Document builder
# --------------------------------------------------------------------------

def build_seq_doc(env: Env, asserts: list[Node], logic=None) -> str:
    em = SeqEmitter(env)
    lines = []
    if logic:
        lines.append(f"(set-logic {logic})")
    lines += em.decls()
    lines += em.bound_asserts()
    for a in asserts:
        lines.append(f"(assert {em.b(a)})")
    lines.append("(check-sat)")
    return "\n".join(lines) + "\n"


def build_array_doc(env: Env, asserts: list[Node], logic="QF_AUFLIA",
                    get_model=False) -> str:
    em = ArrayEmitter(env)
    decls = em.decls()
    body = [em.b(a) for a in asserts]   # populates extra_decls/asserts
    lines = []
    if logic:
        lines.append(f"(set-logic {logic})")
    lines += decls
    lines += em.extra_decls
    lines += em.bound_asserts()
    lines += em.extra_asserts
    for s in body:
        lines.append(f"(assert {s})")
    lines.append("(check-sat)")
    if get_model:
        lines.append("(get-model)")
    return "\n".join(lines) + "\n"
