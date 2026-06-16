"""recfun_bench — does wrapping everything in RecFunction cost us?

A language where "function"/"class" == RecFunction makes EVERY helper a recursive
function symbol, even non-recursive ones. This benchmarks whether that has hidden
perf cost. For each problem we compare forms (inline / unrolled / closed-form vs
recfun), forward and backward, and sweep EVERY tactic + a few combos over each,
recording result + apply-ms + solve-ms (and tactic errors).

Writes results/recfun_bench.csv and prints a summary. Run from prototype/:
    python3 recfun_bench.py
"""
import csv
import os
import time
import z3

_uid = [0]
def uid():
    _uid[0] += 1
    return _uid[0]


PROBLEMS = []
def prob(name, form, build, note=""):
    PROBLEMS.append((name, form, build, note))


# ── non-recursive WRAPPING: dbl(x)=2x nested D deep (recfun used as a plain fn) ─
D = 12
def wrap_inline():
    x = z3.Int(f"x{uid()}"); e = x
    for _ in range(D):
        e = 2 * e
    g = z3.Goal(); g.add(x == 7, e == 7 * 2 ** D); return g

def wrap_recfun():
    u = uid()
    dbl = z3.RecFunction(f"dbl{u}", z3.IntSort(), z3.IntSort())
    y = z3.Int(f"y{u}"); z3.RecAddDefinition(dbl, [y], 2 * y)   # NOT recursive
    x = z3.Int(f"x{u}"); e = x
    for _ in range(D):
        e = dbl(e)
    g = z3.Goal(); g.add(x == 7, e == 7 * 2 ** D); return g

prob("wrap_double", "inline", wrap_inline, "non-recursive, nested 2x")
prob("wrap_double", "recfun", wrap_recfun, "non-recursive, dbl() as RecFunction")


# ── linear recursion: sum_to(K) forward ──────────────────────────────────────
K = 30
def sum_recfun():
    u = uid(); n = z3.Int(f"n{u}")
    f = z3.RecFunction(f"sum{u}", z3.IntSort(), z3.IntSort())
    z3.RecAddDefinition(f, [n], z3.If(n <= 0, 0, n + f(n - 1)))
    r = z3.Int(f"r{u}"); g = z3.Goal(); g.add(f(z3.IntVal(K)) == r); return g

def sum_unroll():
    u = uid(); v = [z3.Int(f"s{u}_{i}") for i in range(K + 1)]
    g = z3.Goal(); g.add(v[0] == 0)
    for i in range(1, K + 1):
        g.add(v[i] == v[i - 1] + i)
    return g

def sum_closed():
    u = uid(); r = z3.Int(f"r{u}")
    g = z3.Goal(); g.add(r == K * (K + 1) // 2); return g

prob("sum_to_fwd", "recfun", sum_recfun)
prob("sum_to_fwd", "unroll", sum_unroll)
prob("sum_to_fwd", "closed", sum_closed)


# ── linear recursion BACKWARD: find n with sum_to(n) == target ───────────────
def sum_bwd_recfun():
    u = uid(); n = z3.Int(f"n{u}")
    f = z3.RecFunction(f"sumb{u}", z3.IntSort(), z3.IntSort())
    z3.RecAddDefinition(f, [n], z3.If(n <= 0, 0, n + f(n - 1)))
    x = z3.Int(f"x{u}")
    g = z3.Goal(); g.add(f(x) == K * (K + 1) // 2, x >= 0, x <= K + 5); return g

prob("sum_to_bwd", "recfun", sum_bwd_recfun, "synthesis: solve for the argument")


# ── factorial (forward) ──────────────────────────────────────────────────────
KF = 8
def fact_recfun():
    u = uid(); n = z3.Int(f"n{u}")
    f = z3.RecFunction(f"fact{u}", z3.IntSort(), z3.IntSort())
    z3.RecAddDefinition(f, [n], z3.If(n <= 0, 1, n * f(n - 1)))
    r = z3.Int(f"r{u}"); g = z3.Goal(); g.add(f(z3.IntVal(KF)) == r); return g

def fact_unroll():
    u = uid(); v = [z3.Int(f"f{u}_{i}") for i in range(KF + 1)]
    g = z3.Goal(); g.add(v[0] == 1)
    for i in range(1, KF + 1):
        g.add(v[i] == v[i - 1] * i)
    return g

prob("factorial", "recfun", fact_recfun, "nonlinear (mul)")
prob("factorial", "unroll", fact_unroll, "nonlinear (mul)")


# ── fib (branching recursion — two self-calls) ───────────────────────────────
KFIB = 18
def fib_recfun():
    u = uid(); n = z3.Int(f"n{u}")
    f = z3.RecFunction(f"fib{u}", z3.IntSort(), z3.IntSort())
    z3.RecAddDefinition(f, [n], z3.If(n < 2, n, f(n - 1) + f(n - 2)))
    r = z3.Int(f"r{u}"); g = z3.Goal(); g.add(f(z3.IntVal(KFIB)) == r); return g

def fib_unroll():
    u = uid(); v = [z3.Int(f"fb{u}_{i}") for i in range(KFIB + 1)]
    g = z3.Goal(); g.add(v[0] == 0, v[1] == 1)
    for i in range(2, KFIB + 1):
        g.add(v[i] == v[i - 1] + v[i - 2])
    return g

prob("fib", "recfun", fib_recfun, "branching: does Z3 share f(n-1)/f(n-2)?")
prob("fib", "unroll", fib_unroll)


# ── structural recursion over a recursive datatype: list length ──────────────
def list_recfun():
    u = uid()
    L = z3.Datatype(f"L{u}")
    L.declare("cons", ("hd", z3.IntSort()), ("tl", L))
    L.declare("nil")
    L = L.create()
    xs = z3.Const(f"xs{u}", L)
    length = z3.RecFunction(f"len{u}", L, z3.IntSort())
    z3.RecAddDefinition(length, [xs], z3.If(L.is_nil(xs), 0, 1 + length(L.tl(xs))))
    lit = L.nil
    for i in range(12):
        lit = L.cons(z3.IntVal(i), lit)
    r = z3.Int(f"r{u}"); g = z3.Goal(); g.add(length(lit) == r); return g

prob("list_length", "recfun", list_recfun, "structural recursion over a datatype")


# ── the sweep ────────────────────────────────────────────────────────────────
TACTICS = z3.tactics()
COMBOS = [("simplify", "solve-eqs"), ("simplify", "propagate-values"),
          ("propagate-values", "solve-eqs"), ("ctx-simplify", "simplify"),
          ("elim-term-ite", "simplify"), ("simplify", "ctx-simplify"),
          ("propagate-values", "simplify"), ("solve-eqs", "simplify")]


def run(build, seq, apply_to=1500, solve_to=3000):
    g = build()
    apply_ms = 0.0
    if seq:
        base = z3.Then(*[z3.Tactic(t) for t in seq]) if len(seq) > 1 \
            else z3.Tactic(seq[0])
        pipe = z3.TryFor(base, apply_to)        # bound tactic apply (some hang)
        t0 = time.perf_counter()
        try:
            res = pipe(g)
            apply_ms = (time.perf_counter() - t0) * 1000
            goal = z3.Goal()
            for i in range(len(res)):
                for j in range(len(res[i])):
                    goal.add(res[i][j])
            g = goal
        except z3.Z3Exception:
            el = (time.perf_counter() - t0) * 1000
            tag = "tactic_to" if el >= apply_to * 0.85 else "tactic_err"
            return tag, round(el, 2), 0.0
    s = z3.Solver(); s.set("timeout", solve_to); s.add(g.as_expr())
    t0 = time.perf_counter()
    r = s.check()
    solve_ms = (time.perf_counter() - t0) * 1000
    return str(r), round(apply_ms, 2), round(solve_ms, 2)


def main():
    rows = []
    total = len(PROBLEMS) * (1 + len(TACTICS) + len(COMBOS))
    done = 0
    for name, form, build, note in PROBLEMS:
        for seq in [[]] + [[t] for t in TACTICS] + [list(c) for c in COMBOS]:
            res, a, sv = run(build, seq)
            rows.append({"problem": name, "form": form, "note": note,
                         "tactic": ">".join(seq) if seq else "(none)",
                         "result": res, "apply_ms": a, "solve_ms": sv,
                         "total_ms": round(a + sv, 2)})
            done += 1
        print(f"  {name}/{form}: {done}/{total}", flush=True)
    out = os.path.join(os.path.dirname(__file__), "results", "recfun_bench.csv")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, "w", newline="") as fh:
        w = csv.DictWriter(fh, fieldnames=list(rows[0].keys()))
        w.writeheader(); w.writerows(rows)
    print(f"\nwrote {out}  ({len(rows)} rows)")


if __name__ == "__main__":
    main()
