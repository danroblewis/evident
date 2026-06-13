"""Task registry.

A TASK = one logical problem with several SEMANTICALLY-IDENTICAL encodings; each
encoding is `build(scale) -> z3.Goal`, tagged with the theory(ies) it uses (single
or multi), plus its expected result. The runner sweeps tactic sequences over each.

Theories covered: int, real, bool, bitvec, array, uf, set, tuple, datatype,
relations, seq, string, regex, fp.
"""
import math
import random
from dataclasses import dataclass
from typing import Callable
import z3

# ── infra ────────────────────────────────────────────────────────────────────
_UID = [0]


def _uid():
    _UID[0] += 1
    return _UID[0]


def G(*assertions):
    g = z3.Goal()
    for a in assertions:
        g.add(a)
    return g


@dataclass(frozen=True)
class Encoding:
    name: str
    theories: tuple
    build: Callable          # (scale) -> z3.Goal
    expected: str = "sat"


@dataclass(frozen=True)
class Task:
    name: str
    scales: tuple
    encodings: tuple         # of Encoding


TASKS: dict = {}


def task(name, scales, encodings):
    TASKS[name] = Task(name, tuple(scales), tuple(encodings))


# ── dispatch: invert a scrambled map  (key whose value is target) ─────────────
def _map(N):
    vals = [(i * 7 + 3) % N for i in range(N)]
    return vals, vals[N // 2]


def disp_arith(N):
    vals, target = _map(N)
    k = z3.Int("k")
    return G(0 <= k, k < N, (k * 7 + 3) % N == target)


def disp_ite(N):
    vals, target = _map(N)
    k = z3.Int("k")
    e = z3.IntVal(vals[N - 1])
    for i in range(N - 2, -1, -1):
        e = z3.If(k == i, z3.IntVal(vals[i]), e)
    return G(0 <= k, k < N, e == target)


def disp_array(N):
    vals, target = _map(N)
    A = z3.K(z3.IntSort(), z3.IntVal(-1))
    for i in range(N):
        A = z3.Store(A, i, vals[i])
    k = z3.Int("k")
    return G(0 <= k, k < N, z3.Select(A, k) == target)


def disp_func(N):
    vals, target = _map(N)
    f = z3.Function("f", z3.IntSort(), z3.IntSort())
    k = z3.Int("k")
    return G(*[f(i) == vals[i] for i in range(N)], 0 <= k, k < N, f(k) == target)


def disp_set(N):
    vals, target = _map(N)
    P, mk, _ = z3.TupleSort(f"P{_uid()}", [z3.IntSort(), z3.IntSort()])
    S = z3.EmptySet(P)
    for i in range(N):
        S = z3.SetAdd(S, mk(i, vals[i]))
    k, v = z3.Ints("k v")
    return G(z3.IsMember(mk(k, v), S), 0 <= k, k < N, v == target)


def disp_set_bv(N):               # multi-theory: set of (bitvec, bitvec) tuples
    vals, target = _map(N)
    w = max(1, math.ceil(math.log2(max(2, N))))
    BV = z3.BitVecSort(w)
    P, mk, _ = z3.TupleSort(f"PB{_uid()}", [BV, BV])
    S = z3.EmptySet(P)
    for i in range(N):
        S = z3.SetAdd(S, mk(z3.BitVecVal(i, w), z3.BitVecVal(vals[i], w)))
    k, v = z3.BitVec("kb", w), z3.BitVec("vb", w)
    return G(z3.IsMember(mk(k, v), S), z3.ULT(k, N), v == z3.BitVecVal(target, w))


task("dispatch", [50, 200], [
    Encoding("arith",  ("int",),                  disp_arith),
    Encoding("ite",    ("bool", "int"),           disp_ite),
    Encoding("array",  ("array", "int"),          disp_array),
    Encoding("func",   ("uf", "int"),             disp_func),
    Encoding("set",    ("set", "tuple"),          disp_set),
    Encoding("set_bv", ("set", "tuple", "bitvec"), disp_set_bv),
])


# ── coloring: 3-colour a planted-colorable graph ──────────────────────────────
_C = 3


def _graph_color(N, seed=7):
    rng = random.Random(seed)
    planted = [rng.randrange(_C) for _ in range(N)]
    return [(u, v) for u in range(N) for v in range(u + 1, N)
            if planted[u] != planted[v] and rng.random() < 0.12]


def col_int(N):
    c = [z3.Int(f"c{u}") for u in range(N)]
    return G(*[z3.And(0 <= x, x < _C) for x in c],
             *[c[u] != c[v] for u, v in _graph_color(N)])


def col_bitvec(N):
    w = max(1, math.ceil(math.log2(_C)))
    c = [z3.BitVec(f"c{u}", w) for u in range(N)]
    return G(*[z3.ULT(x, _C) for x in c],
             *[c[u] != c[v] for u, v in _graph_color(N)])


def col_onehot(N):
    x = [[z3.Bool(f"x{u}_{k}") for k in range(_C)] for u in range(N)]
    cons = []
    for u in range(N):
        cons += [z3.AtLeast(*x[u], 1), z3.AtMost(*x[u], 1)]
    for u, v in _graph_color(N):
        cons += [z3.Not(z3.And(x[u][k], x[v][k])) for k in range(_C)]
    return G(*cons)


def col_enum(N):
    Color, _cols = z3.EnumSort(f"Color{_uid()}", [f"k{k}" for k in range(_C)])
    c = [z3.Const(f"c{u}", Color) for u in range(N)]
    return G(*[c[u] != c[v] for u, v in _graph_color(N)])


task("coloring", [20, 60], [
    Encoding("int",    ("int",),      col_int),
    Encoding("bitvec", ("bitvec",),   col_bitvec),
    Encoding("onehot", ("bool",),     col_onehot),
    Encoding("enum",   ("datatype",), col_enum),
])


# ── reachability: is Tr reachable from S?  (graph split into 2 edge-closed halves)
def _graph_reach(N, seed=7):
    rng = random.Random(seed)
    half = N // 2
    edges = [(i, i + 1) for i in range(half - 1)]   # a path through the first half
    for a in range(N):
        for b in range(N):
            if a != b and (a < half) == (b < half) and rng.random() < 0.08:
                edges.append((a, b))
    return edges, 0, half - 1               # S, reachable target (Tr)


def reach_unroll_bool(N):
    edges, S, T = _graph_reach(N)
    preds = {v: [] for v in range(N)}
    for u, v in edges:
        preds[v].append(u)
    reach = [[z3.Bool(f"r{i}_{v}") for v in range(N)] for i in range(N + 1)]
    cons = [reach[0][v] == z3.BoolVal(v == S) for v in range(N)]
    for i in range(N):
        for v in range(N):
            inc = z3.Or(*[reach[i][u] for u in preds[v]]) if preds[v] else z3.BoolVal(False)
            cons.append(reach[i + 1][v] == z3.Or(reach[i][v], inc))
    cons.append(reach[N][T])
    return G(*cons)


def reach_unroll_set(N):
    edges, S, T = _graph_reach(N)
    succ = {u: [] for u in range(N)}
    for u, v in edges:
        succ[u].append(v)
    Int = z3.IntSort()
    fr = [z3.Const(f"fr{i}", z3.SetSort(Int)) for i in range(N + 1)]
    cons = [fr[0] == z3.SetAdd(z3.EmptySet(Int), S)]
    for i in range(N):
        nxt = fr[i]
        for u in range(N):
            for v in succ[u]:
                nxt = z3.If(z3.IsMember(u, fr[i]), z3.SetAdd(nxt, v), nxt)
        cons.append(fr[i + 1] == nxt)
    cons.append(z3.IsMember(T, fr[N]))
    return G(*cons)


def reach_special(N):              # TransitiveClosure, closed-world; UNSAT ⇒ reachable
    edges, S, T = _graph_reach(N)
    eset = set(edges)
    R = z3.Function(f"R{_uid()}", z3.IntSort(), z3.IntSort(), z3.BoolSort())
    TC = z3.TransitiveClosure(R)
    cons = []
    for u in range(N):
        for v in range(N):
            if u != v:
                cons.append(R(u, v) if (u, v) in eset else z3.Not(R(u, v)))
    cons.append(z3.Not(TC(S, T)))
    return G(*cons)


task("reachability", [20, 60], [
    Encoding("unroll_bool", ("bool",),      reach_unroll_bool),
    Encoding("unroll_set",  ("set",),       reach_unroll_set),
    Encoding("special",     ("relations",), reach_special, "unsat"),
])


# ── arith_system: N ordered vars summing to a target ──────────────────────────
def arith_int(N):
    xs = [z3.Int(f"n{i}") for i in range(N)]
    return G(*[xs[i] < xs[i + 1] for i in range(N - 1)], z3.Sum(xs) == N * 100)


def arith_real(N):
    xs = [z3.Real(f"r{i}") for i in range(N)]
    return G(*[xs[i] < xs[i + 1] for i in range(N - 1)], z3.Sum(xs) == N * 100)


def arith_real_nl(N):              # NRA — a product forces nonlinear reasoning
    xs = [z3.Real(f"r{i}") for i in range(N)]
    return G(*[xs[i] < xs[i + 1] for i in range(N - 1)],
             xs[0] * xs[1] >= 1, z3.Sum(xs) == N * 100)


def arith_bitvec(N):
    w = 32
    xs = [z3.BitVec(f"b{i}", w) for i in range(N)]
    summ = xs[0]
    for x in xs[1:]:
        summ = summ + x
    return G(*[z3.ULT(xs[i], xs[i + 1]) for i in range(N - 1)],
             summ == z3.BitVecVal(N * 100, w))


task("arith_system", [6, 20], [
    Encoding("int",     ("int",),    arith_int),
    Encoding("real",    ("real",),   arith_real),
    Encoding("real_nl", ("real",),   arith_real_nl),
    Encoding("bitvec",  ("bitvec",), arith_bitvec),
])


# ── string_match: length-L string containing "ab" and ending "z" ──────────────
def str_string(L):
    st = z3.String("st")
    return G(z3.Length(st) == L, z3.Contains(st, z3.StringVal("ab")),
             z3.SuffixOf(z3.StringVal("z"), st))


def str_regex(L):
    star = z3.Star(z3.Range("a", "z"))
    re = z3.Intersect(z3.Concat(star, z3.Re(z3.StringVal("ab")), star),
                      z3.Concat(star, z3.Re(z3.StringVal("z"))))
    st = z3.String("st")
    return G(z3.Length(st) == L, z3.InRe(st, re))


task("string_match", [6, 12], [
    Encoding("string", ("string",),          str_string),
    Encoding("regex",  ("string", "regex"),  str_regex),
])


# ── seq_build: bounded Seq(Int) of length L containing 7 ───────────────────────
def seq_seq(L):
    sq = z3.Const("sq", z3.SeqSort(z3.IntSort()))
    return G(z3.Length(sq) == L, z3.Contains(sq, z3.Unit(z3.IntVal(7))))


task("seq_build", [6, 12], [
    Encoding("seq", ("seq",), seq_seq),
])


# ── fp_solve: L positive non-NaN Float32s with positive sum ───────────────────
def fp_fp(L):
    F = z3.Float32()
    rm = z3.RNE()
    xs = [z3.FP(f"x{i}", F) for i in range(L)]
    summ = xs[0]
    for x in xs[1:]:
        summ = z3.fpAdd(rm, summ, x)
    cons = [z3.fpGT(x, z3.FPVal(0.0, F)) for x in xs]
    cons += [z3.Not(z3.fpIsNaN(x)) for x in xs]
    return G(*cons, z3.fpGT(summ, z3.FPVal(0.0, F)))


task("fp_solve", [4, 12], [
    Encoding("fp", ("fp",), fp_fp),
])


def all_theories():
    return sorted({t for tk in TASKS.values() for e in tk.encodings for t in e.theories})
