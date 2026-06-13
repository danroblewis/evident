"""Benchmark 01b — FORWARD lookup, the common case.

Unlike b01 (invert the map: search), here we follow the map forward: start at a
known key and chase it L steps — k_{i} = lookup(k_{i-1}) — then check the
endpoint. This is the realistic "follow the references" pattern, and the key is
always determined, so the table encodings should *evaluate* rather than *search*.

Same 5 encodings as b01; sweep N, fixed chain length L.
"""
import z3
from bench import bench, table

A_, B_, L = 7, 3, 30


def vals_of(N):
    return [(i * A_ + B_) % N for i in range(N)]


def follow(N, K0):
    k = K0
    for _ in range(L):
        k = (k * A_ + B_) % N
    return k


def add_chain(s, N, K0, expected, link):
    ks = [z3.Int(f"k{i}") for i in range(L + 1)]
    s.add(ks[0] == K0)
    for v in ks:
        s.add(0 <= v, v < N)
    for i in range(1, L + 1):
        link(s, ks[i - 1], ks[i])
    s.add(ks[L] == expected)


def b_arith(N, vals, K0, exp):
    s = z3.Solver()
    add_chain(s, N, K0, exp, lambda s, p, n: s.add(n == (p * A_ + B_) % N))
    return s


def b_ite(N, vals, K0, exp):
    def lk(x):
        e = z3.IntVal(vals[N - 1])
        for i in range(N - 2, -1, -1):
            e = z3.If(x == i, z3.IntVal(vals[i]), e)
        return e
    s = z3.Solver()
    add_chain(s, N, K0, exp, lambda s, p, n: s.add(n == lk(p)))
    return s


def b_array(N, vals, K0, exp):
    A = z3.K(z3.IntSort(), z3.IntVal(-1))
    for i in range(N):
        A = z3.Store(A, i, vals[i])
    s = z3.Solver()
    add_chain(s, N, K0, exp, lambda s, p, n: s.add(n == z3.Select(A, p)))
    return s


def b_func(N, vals, K0, exp):
    f = z3.Function("f", z3.IntSort(), z3.IntSort())
    s = z3.Solver()
    for i in range(N):
        s.add(f(i) == vals[i])
    add_chain(s, N, K0, exp, lambda s, p, n: s.add(n == f(p)))
    return s


def b_set(N, vals, K0, exp):
    P, mk, _ = z3.TupleSort("P", [z3.IntSort(), z3.IntSort()])
    S = z3.EmptySet(P)
    for i in range(N):
        S = z3.SetAdd(S, mk(i, vals[i]))
    s = z3.Solver()
    add_chain(s, N, K0, exp, lambda s, p, n: s.add(z3.IsMember(mk(p, n), S)))
    return s


ENCODINGS = [("arith", b_arith), ("ite", b_ite), ("array", b_array),
             ("func", b_func), ("set", b_set)]

if __name__ == "__main__":
    print(f"forward chain, L={L} steps")
    rows = []
    for N in (50, 200, 1000):
        vals, K0 = vals_of(N), 0
        exp = follow(N, K0)
        for label, fn in ENCODINGS:
            m = bench(lambda fn=fn, N=N, v=vals, e=exp: fn(N, v, 0, e),
                      reps=2, timeout_ms=10_000)
            rows.append({"label": label, "N": N, **m})
    table(rows)
