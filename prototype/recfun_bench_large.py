"""recfun_bench_large — the SCALED stress test: base (no-tactic) cases sized to
>=10 s, to learn how RecFunction handles large models / high recursion-count load.

Calibration (recfun_calibrate*.py) established the regimes:
  • FORWARD depth is LINEAR and cheap — only reaches 10 s at ~1.5M unfoldings,
    and BUILD time stays ~0 (all cost is Z3's internal unfolding).
  • BACKWARD synthesis (solve for the argument) blows up with depth×domain —
    10 s at K~140.
  • WIDE models (many independent recfun symbols, each backward) blow up with the
    NUMBER of recursive symbols — 10 s at ~7 symbols.

So three scaled base cases, each >=10 s:
  sum_fwd   K=2,000,000  forward linear depth     (closed-form contrast included)
  sum_bwd   K=140        backward synthesis
  wide_sum  M=7, K=40    many recfun symbols, each backward

Sweep policy: the SEARCH cases get the FULL meaningful tactic set (every tactic
that actually runs on a recfun goal, read from results/recfun_bench.csv) + combos
— a rewrite could genuinely reshape the search. The forward-depth case gets a
CURATED subset (a goal rewrite can't change the unfold cost) plus the unsafe pair
to re-check soundness at scale, plus its closed-form lowering as the contrast.

Streams rows to results/recfun_bench_large.csv (flush per row, so partial runs
survive). Run from prototype/:  python3 recfun_bench_large.py
"""
import csv
import os
import time
import z3

SOLVE_TO = 20_000        # per-solve cap (ms)
APPLY_TO = 5_000         # per-tactic-apply cap (ms)

_uid = [0]
def uid():
    _uid[0] += 1
    return _uid[0]


# ── the three scaled shapes + the closed-form lowerings ──────────────────────
def sum_fwd(K):
    def build():
        u = uid(); n = z3.Int(f"n{u}")
        f = z3.RecFunction(f"sum{u}", z3.IntSort(), z3.IntSort())
        z3.RecAddDefinition(f, [n], z3.If(n <= 0, 0, n + f(n - 1)))
        r = z3.Int(f"r{u}")
        g = z3.Goal(); g.add(f(z3.IntVal(K)) == r); return g
    return build


def sum_closed(K):
    def build():
        u = uid(); r = z3.Int(f"r{u}")
        g = z3.Goal(); g.add(r == K * (K + 1) // 2); return g
    return build


def sum_bwd(K):
    def build():
        u = uid(); n = z3.Int(f"n{u}")
        f = z3.RecFunction(f"sumb{u}", z3.IntSort(), z3.IntSort())
        z3.RecAddDefinition(f, [n], z3.If(n <= 0, 0, n + f(n - 1)))
        x = z3.Int(f"x{u}")
        g = z3.Goal()
        g.add(f(x) == K * (K + 1) // 2, x >= 0, x <= K + 5); return g
    return build


def wide_sum(M, K):
    def build():
        g = z3.Goal(); target = K * (K + 1) // 2
        for _ in range(M):
            u = uid(); n = z3.Int(f"n{u}")
            f = z3.RecFunction(f"sw{u}", z3.IntSort(), z3.IntSort())
            z3.RecAddDefinition(f, [n], z3.If(n <= 0, 0, n + f(n - 1)))
            x = z3.Int(f"x{u}")
            g.add(f(x) == target, x >= 0, x <= K + 5)
        return g
    return build


# ── tactic universe ──────────────────────────────────────────────────────────
COMBOS = [("simplify", "solve-eqs"), ("simplify", "propagate-values"),
          ("propagate-values", "solve-eqs"), ("elim-term-ite", "simplify"),
          ("propagate-values", "simplify"), ("solve-eqs", "simplify")]

# curated subset for the forward-depth case (rewrites can't change unfold cost;
# we want the baseline, a few representative rewriters/solvers, the unsafe pair
# to re-check soundness at scale, and macro-finder)
CURATED = ["simplify", "propagate-values", "solve-eqs", "elim-predicates",
           "ctx-solver-simplify", "elim-term-ite", "smt", "qflia", "auflia",
           "macro-finder", "add-bounds", "nla2bv"]


def meaningful_tactics():
    """Every tactic that produced a real result on a recfun goal in the small
    sweep — i.e. it actually runs rather than instantly erroring on the shape."""
    path = os.path.join(os.path.dirname(__file__), "results", "recfun_bench.csv")
    ran = []
    seen = set()
    for r in csv.DictReader(open(path)):
        if r["form"] == "recfun" and r["result"] in ("sat", "unsat"):
            t = r["tactic"]
            if ">" in t or t == "(none)":
                continue
            if t not in seen:
                seen.add(t); ran.append(t)
    return ran


def seqs_for(policy):
    if policy == "curated":
        return [[]] + [[t] for t in CURATED]
    # full: baseline + every meaningful single tactic + combos
    return [[]] + [[t] for t in meaningful_tactics()] + [list(c) for c in COMBOS]


def run(build, seq):
    g = build()
    apply_ms = 0.0
    if seq:
        base = z3.Then(*[z3.Tactic(t) for t in seq]) if len(seq) > 1 \
            else z3.Tactic(seq[0])
        pipe = z3.TryFor(base, APPLY_TO)
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
            tag = "tactic_to" if el >= APPLY_TO * 0.85 else "tactic_err"
            return tag, round(el, 2), 0.0
    s = z3.Solver(); s.set("timeout", SOLVE_TO); s.add(g.as_expr())
    t0 = time.perf_counter()
    r = s.check()
    solve_ms = (time.perf_counter() - t0) * 1000
    return str(r), round(apply_ms, 2), round(solve_ms, 2)


# name, form, build, sweep-policy, note
PROBLEMS = [
    ("sum_fwd",  "recfun", sum_fwd(2_000_000), "curated",
     "forward linear depth, 2M unfoldings"),
    ("sum_fwd",  "closed", sum_closed(2_000_000), "curated",
     "closed-form lowering of the same"),
    ("sum_bwd",  "recfun", sum_bwd(140), "full",
     "backward synthesis, K=140"),
    ("wide_sum", "recfun", wide_sum(8, 40), "full",
     "8 independent recfun symbols, each backward"),
]


def main():
    out = os.path.join(os.path.dirname(__file__), "results",
                       "recfun_bench_large.csv")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    fh = open(out, "w", newline="")
    w = csv.DictWriter(fh, fieldnames=["problem", "form", "note", "tactic",
                                       "result", "apply_ms", "solve_ms",
                                       "total_ms"])
    w.writeheader(); fh.flush()
    t_start = time.perf_counter()
    for name, form, build, policy, note in PROBLEMS:
        seqs = seqs_for(policy)
        print(f"\n{name}/{form}  ({policy}, {len(seqs)} sequences)", flush=True)
        for i, seq in enumerate(seqs):
            res, a, sv = run(build, seq)
            tac = ">".join(seq) if seq else "(none)"
            w.writerow({"problem": name, "form": form, "note": note,
                        "tactic": tac, "result": res, "apply_ms": a,
                        "solve_ms": sv, "total_ms": round(a + sv, 2)})
            fh.flush()
            el = time.perf_counter() - t_start
            print(f"  [{i + 1}/{len(seqs)}] {tac:<22} {res:<9} "
                  f"a={a:7.1f} s={sv:9.1f}  (elapsed {el:6.0f}s)", flush=True)
    fh.close()
    print(f"\nwrote {out}")


if __name__ == "__main__":
    main()
