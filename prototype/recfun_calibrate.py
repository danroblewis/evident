"""recfun_calibrate — find the scale at which each recursion shape's BASE case
(no tactic, recfun form, fresh Solver) crosses ~10 s, so we can size the
scaled benchmark. Probes a geometric ladder of scales per shape and stops at
the first that exceeds TARGET (or hits the per-solve timeout).

Run from prototype/:  python3 recfun_calibrate.py
"""
import time
import z3

TARGET = 10_000          # ms we want the base case to reach
SOLVE_TO = 40_000        # per-solve cap while probing (ms)

_uid = [0]
def uid():
    _uid[0] += 1
    return _uid[0]


def timed(build):
    g = build()
    s = z3.Solver(); s.set("timeout", SOLVE_TO)
    s.add(g.as_expr())
    t0 = time.perf_counter()
    r = s.check()
    return str(r), (time.perf_counter() - t0) * 1000


# ── forward linear recursion: sum_to(K) with a concrete arg ──────────────────
def sum_fwd(K):
    def build():
        u = uid(); n = z3.Int(f"n{u}")
        f = z3.RecFunction(f"sum{u}", z3.IntSort(), z3.IntSort())
        z3.RecAddDefinition(f, [n], z3.If(n <= 0, 0, n + f(n - 1)))
        r = z3.Int(f"r{u}")
        g = z3.Goal(); g.add(f(z3.IntVal(K)) == r); return g
    return build


# ── backward linear recursion: solve for n with sum_to(n) == target ──────────
def sum_bwd(K):
    def build():
        u = uid(); n = z3.Int(f"n{u}")
        f = z3.RecFunction(f"sumb{u}", z3.IntSort(), z3.IntSort())
        z3.RecAddDefinition(f, [n], z3.If(n <= 0, 0, n + f(n - 1)))
        x = z3.Int(f"x{u}")
        g = z3.Goal()
        g.add(f(x) == K * (K + 1) // 2, x >= 0, x <= K + 5); return g
    return build


# ── nonlinear forward: factorial(K) ──────────────────────────────────────────
def fact_fwd(K):
    def build():
        u = uid(); n = z3.Int(f"n{u}")
        f = z3.RecFunction(f"fact{u}", z3.IntSort(), z3.IntSort())
        z3.RecAddDefinition(f, [n], z3.If(n <= 0, 1, n * f(n - 1)))
        r = z3.Int(f"r{u}")
        g = z3.Goal(); g.add(f(z3.IntVal(K)) == r); return g
    return build


# ── branching forward: fib(K) (two self-calls; does sharing hold?) ───────────
def fib_fwd(K):
    def build():
        u = uid(); n = z3.Int(f"n{u}")
        f = z3.RecFunction(f"fib{u}", z3.IntSort(), z3.IntSort())
        z3.RecAddDefinition(f, [n], z3.If(n < 2, n, f(n - 1) + f(n - 2)))
        r = z3.Int(f"r{u}")
        g = z3.Goal(); g.add(f(z3.IntVal(K)) == r); return g
    return build


# ── structural forward: length of a K-element literal list ───────────────────
def list_fwd(K):
    def build():
        u = uid()
        L = z3.Datatype(f"L{u}")
        L.declare("cons", ("hd", z3.IntSort()), ("tl", L))
        L.declare("nil"); L = L.create()
        xs = z3.Const(f"xs{u}", L)
        length = z3.RecFunction(f"len{u}", L, z3.IntSort())
        z3.RecAddDefinition(length, [xs],
                            z3.If(L.is_nil(xs), 0, 1 + length(L.tl(xs))))
        lit = L.nil
        for i in range(K):
            lit = L.cons(z3.IntVal(i), lit)
        r = z3.Int(f"r{u}")
        g = z3.Goal(); g.add(length(lit) == r); return g
    return build


SHAPES = [
    ("sum_fwd",  sum_fwd,  [30, 100, 300, 1000, 3000, 10000, 30000]),
    ("sum_bwd",  sum_bwd,  [30, 60, 100, 150, 200, 300, 450]),
    ("fact_fwd", fact_fwd, [8, 30, 100, 300, 1000, 3000, 10000]),
    ("fib_fwd",  fib_fwd,  [18, 30, 60, 100, 200, 400, 800]),
    ("list_fwd", list_fwd, [12, 50, 150, 400, 1000, 3000, 8000]),
]


def main():
    print(f"calibrating to TARGET={TARGET} ms (solve cap {SOLVE_TO} ms)\n")
    for name, fam, ladder in SHAPES:
        print(name)
        for K in ladder:
            res, ms = timed(fam(K))
            flag = ""
            if res == "unknown":
                flag = "  <-- TIMEOUT/unknown"
            elif ms >= TARGET:
                flag = "  <-- crosses target"
            print(f"  K={K:>6}  {res:<7} {ms:9.1f} ms{flag}", flush=True)
            if res == "unknown" or ms >= TARGET:
                break
        print()


if __name__ == "__main__":
    main()
