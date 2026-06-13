"""Benchmark 03 — reachability / transitive closure, one problem across engines.

Problem: a directed graph; is target T reachable from source S? (least-fixpoint /
recursion — the shape grammars and recursive claims have.) The graph is split into
two halves with edges only *within* a half, so a first-half→second-half query is
GUARANTEED unreachable (a clean cut); the reachable query is a genuine multi-hop
path. Fixed seed, ground truth checked by BFS. Both directions are measured.

Four semantically-identical encodings of "is T reachable from S?":
  special    — Z3 TransitiveClosure(R) over a CLOSED-world relation (assert R(u,v)
               for edges, ¬R for every non-edge pair). reachable(S,T) ⟺ ¬TC(S,T)
               is UNSAT (the closure FORCES the pair). O(N²) world-closure.
  fixedpoint — Z3 Datalog/Horn (μZ): reach(x,y):-edge(x,y); reach(x,z):-reach(x,y),
               edge(y,z); facts for edges; query reach(S,T). Closed-world LEAST
               fixpoint — the grammar/recursive-claim model. sat⟺reachable.
  unroll-bool— bounded BFS unrolled K=N layers as Bool arrays: reach_0={S},
               reach_{i+1}[v] = reach_i[v] ∨ ∃u(reach_i[u] ∧ edge(u,v)); ask
               reach_K[T]. The hand-rolled bounded fixpoint.
  unroll-set — same bounded fixpoint, frontier as Z3 Set(Int) grown with SetAdd
               per layer (does the set theory explode on fixpoint frontiers like it
               did on dispatch?).

bench() wraps Solver.check(); Fixedpoint.query isn't a Solver, so it gets an inline
timer mirroring bench's metrics (min wall over reps + result; no rlimit — μZ keeps
separate stats).
"""
import random
import time
from collections import defaultdict, deque
import z3
from bench import bench, table

DENSITY = 0.1
SEED = 11


def gen(N):
    """Two-halves graph: edges only within a half ⇒ a first→second cut is unreachable."""
    rng = random.Random(SEED)
    half = N // 2
    edges = [(u, v) for u in range(N) for v in range(N)
             if u != v and (u < half) == (v < half) and rng.random() < DENSITY]
    return edges, half


def _bfs(adj, s):
    dist = {s: 0}
    q = deque([s])
    while q:
        x = q.popleft()
        for y in adj[x]:
            if y not in dist:
                dist[y] = dist[x] + 1
                q.append(y)
    return dist


def pick(N):
    """S in the first half with the deepest reach; Tr = farthest reachable node,
    Tu = an unreachable node in the other half. Returns (edges, S, Tr, Tu, hops)."""
    edges, half = gen(N)
    adj = defaultdict(list)
    for u, v in edges:
        adj[u].append(v)
    best = None
    for s in range(half):
        d = _bfs(adj, s)
        far = max(d.values())
        if best is None or far > best[1]:
            best = (s, far, d)
    S, _, dist = best
    Tr = max(dist, key=lambda k: dist[k])
    Tu = next(n for n in range(half, N) if n not in dist)
    return edges, S, Tr, Tu, dist[Tr]


# ── encodings: each returns a fresh Solver asking "is T reachable from S?" ──
# For Solver-based encodings the SAT/UNSAT convention is normalized by the caller
# (see run_case) to a Python bool `reachable`.

def b_special(N, edges, S, T):
    """reachable(S,T) ⟺ ¬TC(S,T) is UNSAT. We add ¬TC(S,T); UNSAT means reachable."""
    R = z3.Function("R", z3.IntSort(), z3.IntSort(), z3.BoolSort())
    TC = z3.TransitiveClosure(R)
    eset = set(edges)
    s = z3.Solver()
    for u in range(N):
        for v in range(N):
            s.add(R(u, v) if (u, v) in eset else z3.Not(R(u, v)))
    s.add(z3.Not(TC(S, T)))
    return s  # UNSAT ⇒ reachable


def b_unroll_bool(N, edges, S, T):
    """Bool layers reach[i][v]; ask reach[K][T]. SAT ⇒ reachable."""
    K = N
    pred = defaultdict(list)
    for u, v in edges:
        pred[v].append(u)   # predecessors of v
    r = [[z3.Bool(f"r{i}_{v}") for v in range(N)] for i in range(K + 1)]
    s = z3.Solver()
    for v in range(N):
        s.add(r[0][v] == z3.BoolVal(v == S))
    for i in range(K):
        for v in range(N):
            step = z3.Or(r[i][v], *[r[i][u] for u in pred[v]]) if pred[v] else r[i][v]
            s.add(r[i + 1][v] == step)
    s.add(r[K][T])
    return s  # SAT ⇒ reachable


def b_unroll_set(N, edges, S, T):
    """Frontier as Set(Int), grown SetAdd per layer; ask T ∈ frontier[K]. SAT ⇒ reachable."""
    K = N
    Iset = z3.SetSort(z3.IntSort())
    f = [z3.Const(f"f{i}", Iset) for i in range(K + 1)]
    s = z3.Solver()
    s.add(f[0] == z3.SetAdd(z3.EmptySet(z3.IntSort()), S))
    for i in range(K):
        nxt = f[i]
        for (u, v) in edges:
            nxt = z3.If(z3.IsMember(u, f[i]), z3.SetAdd(nxt, v), nxt)
        s.add(f[i + 1] == nxt)
    s.add(z3.IsMember(T, f[K]))
    return s  # SAT ⇒ reachable


# 'special' answers reachable via UNSAT; the others via SAT. This maps the solver
# result to the Python bool `reachable`.
SOLVER_ENC = [
    ("special",     b_special,     lambda r: r == "unsat"),
    ("unroll-bool", b_unroll_bool, lambda r: r == "sat"),
    ("unroll-set",  b_unroll_set,  lambda r: r == "sat"),
]


def b_fixedpoint(N, edges, S, T, timeout_ms):
    """Datalog least-fixpoint. Returns (reachable_bool, min_ms, result_str). Inline
    timed (μZ isn't a Solver). sat ⇒ reachable."""
    walls, res = [], None
    for _ in range(2):
        fp = z3.Fixedpoint()
        fp.set("timeout", timeout_ms)
        IS = z3.IntSort()
        edge = z3.Function("edge", IS, IS, z3.BoolSort())
        reach = z3.Function("reach", IS, IS, z3.BoolSort())
        fp.register_relation(edge, reach)
        x, y, zz = z3.Ints("x y z")
        fp.declare_var(x, y, zz)
        fp.rule(reach(x, y), [edge(x, y)])
        fp.rule(reach(x, zz), [reach(x, y), edge(y, zz)])
        for (u, v) in edges:
            fp.rule(edge(u, v))
        t = time.perf_counter()
        res = fp.query(reach(S, T))
        walls.append((time.perf_counter() - t) * 1000.0)
    rs = str(res)
    return (rs == "sat"), min(walls), rs


def run_case(N, edges, S, T, expected, rows, timeout_ms):
    # solver encodings
    for label, fn, to_reach in SOLVER_ENC:
        m = bench(lambda fn=fn, N=N, e=edges, S=S, T=T: fn(N, e, S, T),
                  reps=2, timeout_ms=timeout_ms)
        got = to_reach(m["result"]) if m["result"] != "unknown" else None
        ok = "TIMEOUT" if m["result"] == "unknown" else ("ok" if got == expected else "WRONG")
        rows.append({"label": label, "N": N, "query": f"{S}->{T}",
                     "reach": str(expected), "result": m["result"],
                     "ck": ok, "rlimit": m["rlimit"], "min_ms": m["min_ms"]})
    # fixedpoint (inline)
    fp_reach, fp_ms, fp_res = b_fixedpoint(N, edges, S, T, timeout_ms)
    ok = "TIMEOUT" if fp_res == "unknown" else ("ok" if fp_reach == expected else "WRONG")
    rows.append({"label": "fixedpoint", "N": N, "query": f"{S}->{T}",
                 "reach": str(expected), "result": fp_res, "ck": ok,
                 "rlimit": None, "min_ms": fp_ms})


if __name__ == "__main__":
    TIMEOUT = 10_000
    rows = []
    for N in (20, 60, 150):
        edges, S, Tr, Tu, hops = pick(N)
        # reachable query (multi-hop), then unreachable (cross-cut)
        run_case(N, edges, S, Tr, True, rows, TIMEOUT)
        run_case(N, edges, S, Tu, False, rows, TIMEOUT)
        print(f"# N={N}: {len(edges)} edges; reachable {S}->{Tr} ({hops} hops), "
              f"unreachable {S}->{Tu}", flush=True)

    print()
    table(rows, cols=("label", "N", "query", "reach", "result", "ck", "rlimit", "min_ms"))
    bad = [r for r in rows if r["ck"] == "WRONG"]
    if bad:
        print(f"\n!! {len(bad)} encodings DISAGREE with ground truth — buggy encoding")
    else:
        print("\nall encodings agree with BFS ground truth")
