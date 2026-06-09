"""
Test battery: ~12 small formulas over bounded Seqs, both sat and unsat.

Each test defines:
  - an Env (declares bounded seq vars + scalars)
  - a list of top-level boolean asserts (IR)
  - the EXPECTED sat/unsat result (ground truth, reasoned by hand)
  - optional `model_check`: given a reconstructed dict {seqname: [elems], ...,
    scalars}, return True if the model is a genuine satisfying sequence.

The harness builds BOTH the seq-theory doc and the array+len doc, runs z3 on
each, and asserts they agree with each other and with the expected truth.
"""

from transform import (
    Env, IntLit, IntVar, ElemVar, BinArith, SeqLen, Nth,
    Cmp, BoolOp, Member, ForallIdx, ForallAdj, SeqEq,
    SeqVar, Unit, Concat, Empty,
)

def L(v): return IntLit(v)

TESTS = []

def test(name, expected, unsupported_note=None):
    def reg(fn):
        TESTS.append((name, fn, expected, unsupported_note))
        return fn
    return reg


# 1. membership SAT
@test("01_member_sat", "sat")
def t01():
    e = Env()
    s = e.seq("s", 5)
    x = e.elemv("x")
    asserts = [
        Cmp(">=", SeqLen(s), L(1)),
        Member(s, x),
        Cmp("=", x, L(42)),
    ]
    def chk(m): return 42 in m["s"]
    return e, asserts, chk


# 2. membership UNSAT (all elems 0, but must contain 7)
@test("02_member_unsat", "unsat")
def t02():
    e = Env()
    s = e.seq("s", 5)
    asserts = [
        Cmp(">=", SeqLen(s), L(1)),
        ForallIdx(s, lambda el: Cmp("=", el, L(0))),
        Member(s, L(7)),
    ]
    return e, asserts, None


# 3. length constraint SAT
@test("03_len_sat", "sat")
def t03():
    e = Env()
    s = e.seq("s", 5)
    asserts = [Cmp("=", SeqLen(s), L(3))]
    def chk(m): return len(m["s"]) == 3
    return e, asserts, chk


# 4. length constraint UNSAT (len 7 but bound 5)
@test("04_len_unsat", "unsat")
def t04():
    e = Env()
    s = e.seq("s", 5)
    asserts = [Cmp("=", SeqLen(s), L(7))]
    return e, asserts, None


# 5. sortedness SAT
@test("05_sorted_sat", "sat")
def t05():
    e = Env()
    s = e.seq("s", 6)
    asserts = [
        Cmp("=", SeqLen(s), L(4)),
        ForallAdj(s, lambda a, b: Cmp("<=", a, b)),
    ]
    def chk(m):
        xs = m["s"]
        return all(xs[i] <= xs[i+1] for i in range(len(xs)-1))
    return e, asserts, chk


# 6. sorted but contradictory concrete values UNSAT
@test("06_sorted_unsat", "unsat")
def t06():
    e = Env()
    s = e.seq("s", 6)
    asserts = [
        Cmp(">=", SeqLen(s), L(2)),
        ForallAdj(s, lambda a, b: Cmp("<=", a, b)),
        Cmp("=", Nth(s, L(0)), L(5)),
        Cmp("=", Nth(s, L(1)), L(3)),
    ]
    return e, asserts, None


# 7. concat length SAT
@test("07_concat_len_sat", "sat")
def t07():
    e = Env()
    a = e.seq("a", 3)
    b = e.seq("b", 3)
    c = Concat(a, b)
    asserts = [
        Cmp("=", SeqLen(a), L(2)),
        Cmp("=", SeqLen(b), L(3)),
        Cmp("=", SeqLen(c), L(5)),
    ]
    def chk(m): return len(m["a"]) == 2 and len(m["b"]) == 3
    return e, asserts, chk


# 8. concat membership SAT (x lives in the b-half)
@test("08_concat_member_sat", "sat")
def t08():
    e = Env()
    a = e.seq("a", 3)
    b = e.seq("b", 3)
    c = Concat(a, b)
    asserts = [
        Cmp("=", SeqLen(a), L(2)),
        Cmp("=", SeqLen(b), L(2)),
        Cmp("=", Nth(b, L(0)), L(99)),
        Member(c, L(99)),
    ]
    def chk(m): return 99 in (m["a"] + m["b"])
    return e, asserts, chk


# 9. concat length UNSAT (2+2 cannot equal 5)
@test("09_concat_len_unsat", "unsat")
def t09():
    e = Env()
    a = e.seq("a", 3)
    b = e.seq("b", 3)
    c = Concat(a, b)
    asserts = [
        Cmp("=", SeqLen(a), L(2)),
        Cmp("=", SeqLen(b), L(2)),
        Cmp("=", SeqLen(c), L(5)),
    ]
    return e, asserts, None


# 10. element predicate (all positive) SAT
@test("10_allpos_sat", "sat")
def t10():
    e = Env()
    s = e.seq("s", 5)
    asserts = [
        Cmp(">=", SeqLen(s), L(2)),
        ForallIdx(s, lambda el: Cmp(">", el, L(0))),
    ]
    def chk(m): return all(v > 0 for v in m["s"])
    return e, asserts, chk


# 11. element predicate contradiction UNSAT
@test("11_allpos_unsat", "unsat")
def t11():
    e = Env()
    s = e.seq("s", 5)
    asserts = [
        Cmp(">=", SeqLen(s), L(1)),
        ForallIdx(s, lambda el: Cmp(">", el, L(0))),
        Cmp("<=", Nth(s, L(0)), L(0)),
    ]
    return e, asserts, None


# 12. seq equality SAT (s equals a unit)
@test("12_eq_unit_sat", "sat")
def t12():
    e = Env()
    s = e.seq("s", 4)
    x = e.elemv("x")
    asserts = [
        SeqEq(s, Unit(x)),
        Cmp("=", x, L(7)),
    ]
    def chk(m): return m["s"] == [7]
    return e, asserts, chk


# 13. seq equality UNSAT (len mismatch)
@test("13_eq_unit_unsat", "unsat")
def t13():
    e = Env()
    s = e.seq("s", 4)
    x = e.elemv("x")
    asserts = [
        SeqEq(s, Unit(x)),
        Cmp("=", SeqLen(s), L(2)),
    ]
    return e, asserts, None


# 14. combined: sorted AND member of a specific value, SAT
@test("14_sorted_member_sat", "sat")
def t14():
    e = Env()
    s = e.seq("s", 6)
    asserts = [
        Cmp("=", SeqLen(s), L(4)),
        ForallAdj(s, lambda a, b: Cmp("<=", a, b)),
        Member(s, L(10)),
        ForallIdx(s, lambda el: BoolOp("and", [Cmp(">=", el, L(0)),
                                               Cmp("<=", el, L(20))])),
    ]
    def chk(m):
        xs = m["s"]
        return all(xs[i] <= xs[i+1] for i in range(len(xs)-1)) and 10 in xs
    return e, asserts, chk
