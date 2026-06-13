"""Benchmark 02 — finite-domain CSP (graph C-coloring), one problem across theories.

A random graph, planted to be C-colorable (edges added only between
differently-colored nodes), so it's SAT; the solver must *find* a valid coloring.
This is the "bounded under-determined" shape the language wants to be good at.

Four encodings of "each node gets one of C colors, adjacent nodes differ":
  int     — c_u ∈ [0,C);  c_u ≠ c_v per edge
  bitvec  — c_u : BitVec(w), ULT(c_u, C);  c_u ≠ c_v
  onehot  — C booleans/node, exactly-one (AtLeast/AtMost 1);  ¬(x_uk ∧ x_vk)
  enum    — c_u : Color datatype;  c_u ≠ c_v
"""
import random
import math
import z3
from bench import bench, table

C = 3
DENSITY = 0.12   # edge prob among differently-colored pairs


def graph(N, seed=7):
    rng = random.Random(seed)
    planted = [rng.randrange(C) for _ in range(N)]
    edges = [(u, v) for u in range(N) for v in range(u + 1, N)
             if planted[u] != planted[v] and rng.random() < DENSITY]
    return edges


def b_int(N, edges):
    c = [z3.Int(f"c{u}") for u in range(N)]
    s = z3.Solver()
    for x in c:
        s.add(0 <= x, x < C)
    for u, v in edges:
        s.add(c[u] != c[v])
    return s


def b_bitvec(N, edges):
    w = max(1, math.ceil(math.log2(C)))
    c = [z3.BitVec(f"c{u}", w) for u in range(N)]
    s = z3.Solver()
    for x in c:
        s.add(z3.ULT(x, C))
    for u, v in edges:
        s.add(c[u] != c[v])
    return s


def b_onehot(N, edges):
    x = [[z3.Bool(f"x{u}_{k}") for k in range(C)] for u in range(N)]
    s = z3.Solver()
    for u in range(N):
        s.add(z3.AtLeast(*x[u], 1), z3.AtMost(*x[u], 1))
    for u, v in edges:
        for k in range(C):
            s.add(z3.Not(z3.And(x[u][k], x[v][k])))
    return s


_enum_ctr = [0]


def b_enum(N, edges):
    _enum_ctr[0] += 1   # unique sort name per build (global ctx keeps declarations)
    Color, cols = z3.EnumSort(f"Color{_enum_ctr[0]}", [f"k{k}" for k in range(C)])
    c = [z3.Const(f"c{u}", Color) for u in range(N)]
    s = z3.Solver()
    for u, v in edges:
        s.add(c[u] != c[v])
    return s


ENCODINGS = [("int", b_int), ("bitvec", b_bitvec), ("onehot", b_onehot),
             ("enum", b_enum)]

if __name__ == "__main__":
    rows = []
    for N in (20, 60, 150):
        edges = graph(N)
        for label, fn in ENCODINGS:
            m = bench(lambda fn=fn, N=N, e=edges: fn(N, e), reps=2, timeout_ms=10_000)
            rows.append({"label": label, "N": N, "edges": len(edges), **m})
    table(rows, cols=("label", "N", "edges", "result", "rlimit", "min_ms"))
