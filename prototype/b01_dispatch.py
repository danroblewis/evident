"""Benchmark 01 — dispatch / inverse lookup, one problem across theories.

Problem: a table maps key i ↦ vals[i] = (i*7 + 3) mod N (a scrambled map).
Query: find a key k ∈ [0,N) whose value is `target` (SAT by construction).
The solver must *invert* the map — real search, not evaluation.

Five semantically-identical encodings:
  arith  — no table; the map's formula `(k*7+3)%N == target`  (structure beats data)
  ite    — the ternary spine  If(k==0, v0, If(k==1, v1, …))
  array  — Store the table into an Array, Select(A, k)
  func   — EUF: f(i)==vi axioms, query f(k)
  set    — relations-as-tuple-sets: (k, v) ∈ Set of (i, vi) tuples
"""
import z3
from bench import bench, table

MOD_A, MOD_B = 7, 3


def make(N):
    vals = [(i * MOD_A + MOD_B) % N for i in range(N)]
    target = vals[N // 2]
    return vals, target


def b_arith(N, vals, target):
    k = z3.Int("k")
    s = z3.Solver()
    s.add(0 <= k, k < N, (k * MOD_A + MOD_B) % N == target)
    return s


def b_ite(N, vals, target):
    k = z3.Int("k")
    val_k = z3.IntVal(vals[N - 1])
    for i in range(N - 2, -1, -1):
        val_k = z3.If(k == i, z3.IntVal(vals[i]), val_k)
    s = z3.Solver()
    s.add(0 <= k, k < N, val_k == target)
    return s


def b_array(N, vals, target):
    A = z3.K(z3.IntSort(), z3.IntVal(-1))
    for i in range(N):
        A = z3.Store(A, i, vals[i])
    k = z3.Int("k")
    s = z3.Solver()
    s.add(0 <= k, k < N, z3.Select(A, k) == target)
    return s


def b_func(N, vals, target):
    f = z3.Function("f", z3.IntSort(), z3.IntSort())
    k = z3.Int("k")
    s = z3.Solver()
    for i in range(N):
        s.add(f(i) == vals[i])
    s.add(0 <= k, k < N, f(k) == target)
    return s


def b_set(N, vals, target):
    P, mk, _ = z3.TupleSort("P", [z3.IntSort(), z3.IntSort()])
    S = z3.EmptySet(P)
    for i in range(N):
        S = z3.SetAdd(S, mk(i, vals[i]))
    k, v = z3.Ints("k v")
    s = z3.Solver()
    s.add(z3.IsMember(mk(k, v), S), 0 <= k, k < N, v == target)
    return s


ENCODINGS = [("arith", b_arith), ("ite", b_ite), ("array", b_array),
             ("func", b_func), ("set", b_set)]

if __name__ == "__main__":
    rows = []
    for N in (50, 200, 1000):
        vals, target = make(N)
        for label, fn in ENCODINGS:
            m = bench(lambda fn=fn, N=N, v=vals, t=target: fn(N, v, t),
                      reps=3, timeout_ms=30_000)
            rows.append({"label": label, "N": N, **m})
    table(rows)
