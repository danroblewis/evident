"""recfun_bench_parallel — the scaled stress sweep, run across cores.

Same cells as recfun_bench_large.py, but each (problem, form, tactic) cell runs
in its OWN process via multiprocessing.Pool. That sidesteps Z3's global-context
gotcha (re-declaring recfun/sort names collides *within* a process; separate
processes each get a clean context) and uses the box's ~24 cores.

Measurement caveat: with WORKERS solves in flight at once they contend for memory
bandwidth, so absolute solve_ms inflates vs an isolated serial run. That is fine
for RANKING tactics (which help / hurt / flip soundness). For the clean headline
numbers, re-measure the baselines with recfun_bench_baselines.py (serial,
isolated). Each row here carries `workers` so the load context is on record.

Writes results/recfun_bench_large.csv. Run from prototype/:
    python3 recfun_bench_parallel.py [WORKERS]
"""
import csv
import os
import sys
import time
import multiprocessing as mp
import z3

SOLVE_TO = 20_000
APPLY_TO = 5_000

COMBOS = [("simplify", "solve-eqs"), ("simplify", "propagate-values"),
          ("propagate-values", "solve-eqs"), ("elim-term-ite", "simplify"),
          ("propagate-values", "simplify"), ("solve-eqs", "simplify")]
CURATED = ["simplify", "propagate-values", "solve-eqs", "elim-predicates",
           "ctx-solver-simplify", "elim-term-ite", "smt", "qflia", "auflia",
           "macro-finder", "add-bounds", "nla2bv"]


# ── builders: pure functions of scale, fresh names each call (own process) ───
_uid = [0]
def uid():
    _uid[0] += 1
    return _uid[0]


def build_sum_fwd(K):
    u = uid(); n = z3.Int(f"n{u}")
    f = z3.RecFunction(f"sum{u}", z3.IntSort(), z3.IntSort())
    z3.RecAddDefinition(f, [n], z3.If(n <= 0, 0, n + f(n - 1)))
    r = z3.Int(f"r{u}")
    g = z3.Goal(); g.add(f(z3.IntVal(K)) == r); return g


def build_sum_closed(K):
    u = uid(); r = z3.Int(f"r{u}")
    g = z3.Goal(); g.add(r == K * (K + 1) // 2); return g


def build_sum_bwd(K):
    u = uid(); n = z3.Int(f"n{u}")
    f = z3.RecFunction(f"sumb{u}", z3.IntSort(), z3.IntSort())
    z3.RecAddDefinition(f, [n], z3.If(n <= 0, 0, n + f(n - 1)))
    x = z3.Int(f"x{u}")
    g = z3.Goal(); g.add(f(x) == K * (K + 1) // 2, x >= 0, x <= K + 5); return g


def build_wide_sum(MK):
    M, K = MK
    g = z3.Goal(); target = K * (K + 1) // 2
    for _ in range(M):
        u = uid(); n = z3.Int(f"n{u}")
        f = z3.RecFunction(f"sw{u}", z3.IntSort(), z3.IntSort())
        z3.RecAddDefinition(f, [n], z3.If(n <= 0, 0, n + f(n - 1)))
        x = z3.Int(f"x{u}")
        g.add(f(x) == target, x >= 0, x <= K + 5)
    return g


BUILDERS = {"sum_fwd": build_sum_fwd, "sum_closed": build_sum_closed,
            "sum_bwd": build_sum_bwd, "wide_sum": build_wide_sum}


def cell(task):
    """Run one (problem, form, builder-key, scale, tactic-seq) cell. Picklable
    plain-data in, plain-dict out — so it crosses the process boundary."""
    name, form, bkey, scale, seq, note = task
    g = BUILDERS[bkey](scale)
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
            return {"problem": name, "form": form, "note": note,
                    "tactic": ">".join(seq), "result": tag,
                    "apply_ms": round(el, 2), "solve_ms": 0.0,
                    "total_ms": round(el, 2)}
    s = z3.Solver(); s.set("timeout", SOLVE_TO); s.add(g.as_expr())
    t0 = time.perf_counter()
    r = s.check()
    solve_ms = (time.perf_counter() - t0) * 1000
    return {"problem": name, "form": form, "note": note,
            "tactic": ">".join(seq) if seq else "(none)", "result": str(r),
            "apply_ms": round(apply_ms, 2), "solve_ms": round(solve_ms, 2),
            "total_ms": round(apply_ms + solve_ms, 2)}


def meaningful_tactics():
    path = os.path.join(os.path.dirname(__file__), "results", "recfun_bench.csv")
    ran, seen = [], set()
    for r in csv.DictReader(open(path)):
        if r["form"] == "recfun" and r["result"] in ("sat", "unsat"):
            t = r["tactic"]
            if ">" in t or t == "(none)" or t in seen:
                continue
            seen.add(t); ran.append(t)
    return ran


def seqs(policy):
    if policy == "curated":
        return [[]] + [[t] for t in CURATED]
    return [[]] + [[t] for t in meaningful_tactics()] + [list(c) for c in COMBOS]


# problem, form, builder-key, scale, policy, note
SPEC = [
    ("sum_fwd",  "recfun", "sum_fwd",    2_000_000, "curated",
     "forward linear depth, 2M unfoldings"),
    ("sum_fwd",  "closed", "sum_closed", 2_000_000, "curated",
     "closed-form lowering of the same"),
    ("sum_bwd",  "recfun", "sum_bwd",    140,       "full",
     "backward synthesis, K=140"),
    ("wide_sum", "recfun", "wide_sum",   (8, 40),   "full",
     "8 independent recfun symbols, each backward"),
]


def main():
    workers = int(sys.argv[1]) if len(sys.argv) > 1 else 20
    tasks = []
    for name, form, bkey, scale, policy, note in SPEC:
        for seq in seqs(policy):
            tasks.append((name, form, bkey, scale, seq, note))
    print(f"{len(tasks)} cells over {workers} workers", flush=True)

    out = os.path.join(os.path.dirname(__file__), "results",
                       "recfun_bench_large.csv")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    fh = open(out, "w", newline="")
    cols = ["problem", "form", "note", "tactic", "result", "apply_ms",
            "solve_ms", "total_ms", "workers"]
    w = csv.DictWriter(fh, fieldnames=cols); w.writeheader(); fh.flush()

    t0 = time.perf_counter(); done = 0
    with mp.Pool(workers) as pool:
        for row in pool.imap_unordered(cell, tasks):
            row["workers"] = workers
            w.writerow(row); fh.flush()
            done += 1
            print(f"  [{done}/{len(tasks)}] {row['problem']}/{row['form']} "
                  f"{row['tactic'][:22]:<22} {row['result']:<9} "
                  f"s={row['solve_ms']:9.1f}  ({time.perf_counter()-t0:5.0f}s)",
                  flush=True)
    fh.close()
    print(f"\nwrote {out}  ({done} cells in {time.perf_counter()-t0:.0f}s)")


if __name__ == "__main__":
    main()
