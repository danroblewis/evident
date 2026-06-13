"""Task registry for the combinatorial benchmark suite.

A TASK = a logical problem with several SEMANTICALLY-IDENTICAL encodings, each
tagged with the theory(ies) it uses (single- or multi-theory). Every encoding is
a `build(N) -> z3.Goal` plus the expected result, so the runner can (a) apply any
tactic sequence to the goal and (b) check correctness.

Encodings reuse the validated builds in b01_dispatch / b02_coloring /
b03_reachability — one source of truth.
"""
import math
import z3
import b01_dispatch as d
import b02_coloring as c
import b03_reachability as r


def goal_from(solver):
    g = z3.Goal()
    for a in solver.assertions():
        g.add(a)
    return g


# ---- adapters: prepare task data, return a Goal ----
def _disp(N, fn):
    vals, target = d.make(N)
    return goal_from(fn(N, vals, target))


def _color(N, fn):
    return goal_from(fn(N, c.graph(N)))


def _reach(N, fn):
    edges, S, Tr, _Tu, _dist = r.pick(N)
    return goal_from(fn(N, edges, S, Tr))   # the REACHABLE query


# ---- mixed-theory encoding: dispatch as a set of (bitvec, bitvec) tuples ----
def _disp_set_bv(N):
    import b01_dispatch as d
    vals, target = d.make(N)
    w = max(1, math.ceil(math.log2(max(2, N))))
    BV = z3.BitVecSort(w)
    P, mk, _ = z3.TupleSort(f"PB{N}", [BV, BV])
    S = z3.EmptySet(P)
    for i in range(N):
        S = z3.SetAdd(S, mk(z3.BitVecVal(i, w), z3.BitVecVal(vals[i], w)))
    k, v = z3.BitVec("kb", w), z3.BitVec("vb", w)
    g = z3.Goal()
    g.add(z3.IsMember(mk(k, v), S), z3.ULT(k, N), v == z3.BitVecVal(target, w))
    return g


# theory tags use a small controlled vocabulary so we can pivot on them.
TASKS = {
    "dispatch": {
        "scales": [50, 200],
        "encodings": {
            "arith":  (["int"],            lambda N: _disp(N, d.b_arith),  "sat"),
            "ite":    (["bool", "int"],    lambda N: _disp(N, d.b_ite),    "sat"),
            "array":  (["array", "int"],   lambda N: _disp(N, d.b_array),  "sat"),
            "func":   (["uf", "int"],      lambda N: _disp(N, d.b_func),   "sat"),
            "set":    (["set", "tuple"],   lambda N: _disp(N, d.b_set),    "sat"),
            "set_bv": (["set", "tuple", "bitvec"], _disp_set_bv,           "sat"),
        },
    },
    "coloring": {
        "scales": [20, 60],
        "encodings": {
            "int":    (["int"],       lambda N: _color(N, c.b_int),    "sat"),
            "bitvec": (["bitvec"],    lambda N: _color(N, c.b_bitvec), "sat"),
            "onehot": (["bool"],      lambda N: _color(N, c.b_onehot), "sat"),
            "enum":   (["datatype"],  lambda N: _color(N, c.b_enum),   "sat"),
        },
    },
    "reachability": {
        "scales": [20, 60],
        "encodings": {
            # special's SAT/UNSAT is inverted (¬TC unsat ⇒ reachable)
            "special":     (["relations"], lambda N: _reach(N, r.b_special),     "unsat"),
            "unroll_bool": (["bool"],      lambda N: _reach(N, r.b_unroll_bool), "sat"),
            "unroll_set":  (["set"],       lambda N: _reach(N, r.b_unroll_set),  "sat"),
        },
    },
}
