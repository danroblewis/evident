"""Combinatorial benchmark runner.

Sweeps, for the chosen tasks:
  task × scale × encoding(theory or multi-theory) × TACTIC SEQUENCE
where the tactic sequences are EVERY ordered sequence of tactics (with repetition)
of length 1..max_len, plus the empty baseline — i.e. each tactic alone, each
doubled, each pair, then length 3, 4, … up to max_len (== len(TACTICS) is the full
"until we run out of tactics" sweep, which is huge — see the count printed).

For each case: apply the sequence (timed once), solve (reps, min wall), check the
result against expected. Rows stream to a CSV for offline analysis; `--summarize`
reads it back and prints the best encoding per task and the best tactic sequence
per encoding.

Usage:
  python3 suite.py --max-len 2                 # run (default tasks/scales)
  python3 suite.py --max-len 3 --tasks dispatch
  python3 suite.py --summarize results.csv     # analyze a prior run
"""
import argparse
import csv
import itertools
import sys
import time
import z3
from suite_tasks import TASKS

# Generally-safe simplification tactics (cheap workhorses; ctx-solver-simplify is
# deliberately excluded — t01 measured it at 26 s, unusable in a sweep).
TACTICS = ["simplify", "propagate-values", "solve-eqs",
           "elim-term-ite", "propagate-ineqs", "ctx-simplify"]

FIELDS = ["task", "scale", "encoding", "theories", "combo", "combo_len",
          "result", "ok", "tactic_ms", "solve_ms", "total_ms",
          "size_before", "size_after"]


def combos(max_len):
    yield ()  # baseline: no tactic
    for k in range(1, max_len + 1):
        for combo in itertools.product(TACTICS, repeat=k):
            yield combo


def count_combos(max_len):
    return 1 + sum(len(TACTICS) ** k for k in range(1, max_len + 1))


def apply_combo(goal, combo):
    if not combo:
        return goal, 0.0, None
    t = z3.Tactic(combo[0]) if len(combo) == 1 else z3.Then(*combo)
    t0 = time.perf_counter()
    try:
        res = t(goal)
    except z3.Z3Exception:
        return None, (time.perf_counter() - t0) * 1000, "tactic_error"
    dt = (time.perf_counter() - t0) * 1000
    out = z3.Goal()
    for i in range(len(res)):
        sub = res[i]
        for j in range(len(sub)):
            out.add(sub[j])
    return out, dt, None


def measure(build, combo, reps, timeout_ms):
    g = build()
    size_before = len(g.sexpr())
    g2, tac_ms, err = apply_combo(g, combo)
    if err:
        return dict(result=err, tactic_ms=tac_ms, solve_ms=0.0, total_ms=tac_ms,
                    size_before=size_before, size_after=-1)
    size_after = len(g2.sexpr())
    expr = g2.as_expr()
    walls, result = [], None
    for _ in range(reps):
        s = z3.Solver()
        s.set("timeout", timeout_ms)
        s.add(expr)
        t0 = time.perf_counter()
        result = s.check()
        walls.append((time.perf_counter() - t0) * 1000)
    sms = min(walls)
    return dict(result=str(result), tactic_ms=tac_ms, solve_ms=sms,
                total_ms=tac_ms + sms, size_before=size_before, size_after=size_after)


def run(tasks, max_len, reps, timeout_ms, out_path):
    cases = sum(len(TASKS[t]["encodings"]) * len(TASKS[t]["scales"]) for t in tasks)
    total = cases * count_combos(max_len)
    print(f"tasks={tasks}  tactic-sequences/case={count_combos(max_len)}  "
          f"(enc×scale)={cases}  → {total} solves (×{reps} reps)", file=sys.stderr)
    done = 0
    with open(out_path, "w", newline="") as fh:
        w = csv.DictWriter(fh, fieldnames=FIELDS)
        w.writeheader()
        for task in tasks:
            spec = TASKS[task]
            for N in spec["scales"]:
                for enc, (theories, build, expected) in spec["encodings"].items():
                    for combo in combos(max_len):
                        m = measure(lambda: build(N), combo, reps, timeout_ms)
                        w.writerow({
                            "task": task, "scale": N, "encoding": enc,
                            "theories": "+".join(theories),
                            "combo": ">".join(combo) or "(none)",
                            "combo_len": len(combo),
                            "ok": m["result"] == expected,
                            **{k: (round(v, 2) if isinstance(v, float) else v)
                               for k, v in m.items()},
                        })
                        done += 1
                    fh.flush()
                    print(f"  {task}/{enc} N={N}: {done}/{total}", file=sys.stderr)
    print(f"wrote {out_path}", file=sys.stderr)


def summarize(path):
    rows = list(csv.DictReader(open(path)))
    for r in rows:
        r["total_ms"] = float(r["total_ms"]); r["solve_ms"] = float(r["solve_ms"])
        r["scale"] = int(r["scale"]); r["ok"] = r["ok"] == "True"
    tasks = sorted({r["task"] for r in rows})
    for task in tasks:
        tr = [r for r in rows if r["task"] == task]
        N = max(r["scale"] for r in tr)
        atN = [r for r in tr if r["scale"] == N]
        print(f"\n### {task}  (N={N})")
        # best encoding, baseline (no tactic)
        base = [r for r in atN if r["combo"] == "(none)" and r["ok"]]
        base.sort(key=lambda r: r["solve_ms"])
        print("  best encoding (no tactics):")
        for r in base:
            print(f"    {r['encoding']:10} {r['theories']:20} {r['solve_ms']:8.1f} ms")
        # best tactic sequence per encoding (by total_ms), vs its baseline
        print("  best tactic sequence per encoding (total_ms, vs baseline solve):")
        for enc in {r["encoding"] for r in atN}:
            er = [r for r in atN if r["encoding"] == enc and r["ok"]]
            if not er:
                continue
            b = next((r for r in er if r["combo"] == "(none)"), None)
            best = min(er, key=lambda r: r["total_ms"])
            bb = f"{b['solve_ms']:.1f}" if b else "?"
            flag = "  ← tactic helps" if b and best["total_ms"] < b["solve_ms"] - 0.05 else ""
            print(f"    {enc:10} base={bb:>8}ms  best={best['total_ms']:8.1f}ms "
                  f"via [{best['combo']}]{flag}")
        # canaries: separate genuine soundness violations (definite wrong answer)
        # from tactic-induced timeouts (unknown) and inapplicable tactics (error).
        unsound = [r for r in tr if r["result"] in ("sat", "unsat") and not r["ok"]]
        tmo = [r for r in tr if r["result"] == "unknown"]
        if unsound:
            print(f"  ⚠⚠ {len(unsound)} SOUNDNESS violations (tactic changed sat/unsat!) "
                  f"e.g. {unsound[0]['encoding']} [{unsound[0]['combo']}]")
        if tmo:
            combos_ = sorted({r["combo"] for r in tmo})
            print(f"  ⓘ {len(tmo)} tactic-induced timeouts (unknown) — a tactic HURT; "
                  f"e.g. combos {combos_[:2]}")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--max-len", type=int, default=2)
    ap.add_argument("--tasks", nargs="*", default=list(TASKS))
    ap.add_argument("--reps", type=int, default=2)
    ap.add_argument("--timeout", type=int, default=5000)
    ap.add_argument("--out", default="suite_results.csv")
    ap.add_argument("--summarize", metavar="CSV")
    a = ap.parse_args()
    if a.summarize:
        summarize(a.summarize)
    else:
        run(a.tasks, a.max_len, a.reps, a.timeout, a.out)
        summarize(a.out)
