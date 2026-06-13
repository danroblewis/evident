#!/usr/bin/env python3
"""benchsuite CLI — combinatorial Z3 benchmarks.

  python3 run.py run --max-len 2                  # full sweep, all tasks
  python3 run.py run --tasks dispatch --max-len 3
  python3 run.py report results/run.csv           # regenerate md/json
  python3 run.py profile dispatch set 200 --tactics blast   # model AST diff
  python3 run.py list                             # tasks / theories / tactics
"""
import argparse
import os
import z3
from benchsuite import runner, report, tactics, profiling
from benchsuite.tasks import TASKS, all_theories


def _tac(token):
    if token in ("blast", "blast_select_store"):
        return tactics.Tactic("simplify", (("blast_select_store", True),))
    return tactics.Tactic(token)


def cmd_run(a):
    os.makedirs(os.path.dirname(a.out) or ".", exist_ok=True)
    rows = runner.run(a.tasks, a.max_len, a.reps, a.timeout)
    report.write_csv(rows, a.out)
    report.summarize(rows)
    if a.json:
        report.write_json(rows, a.json)
    md = a.markdown or os.path.splitext(a.out)[0] + ".md"
    report.markdown(rows, md, source=a.out)
    mdiff = os.path.splitext(a.out)[0] + "-modeldiff.md"
    report.model_diff(rows, mdiff)
    print(f"\nwrote {a.out}, {md}, {mdiff}" + (f", {a.json}" if a.json else ""))


def cmd_report(a):
    if a.markdown:
        report.markdown(a.csv, a.markdown, source=a.csv)
    if a.model_diff:
        report.model_diff(a.csv, a.model_diff)
        print(f"wrote {a.model_diff}")
    if a.translations:
        path, n = report.translations(a.csv, a.translations)
        print(f"wrote {path} + {n} smt2 files under {a.translations}/smt2/")
    if a.json:
        report.write_json(report._load(a.csv), a.json)
    report.summarize(a.csv)


def cmd_profile(a):
    enc = next(e for e in TASKS[a.task].encodings if e.name == a.encoding)
    seq = tuple(_tac(t) for t in a.tactics.split(",") if t)
    g0 = enc.build(a.scale)
    p0 = profiling.profile(g0)
    g1, ms, err = tactics.apply(g0, seq)
    if err:
        print("tactic error"); return
    p1 = profiling.profile(g1)
    scalars, movers = profiling.diff(p0, p1)
    print(f"# {a.task}/{a.encoding} N={a.scale}  tactics=[{tactics.seq_str(seq)}]  "
          f"(apply {ms:.1f} ms)\n")
    print(f"  {'metric':12} {'before':>10} {'after':>10} {'Δ':>10}")
    for k, (b, av, d) in scalars.items():
        print(f"  {k:12} {b:>10} {av:>10} {d:>+10}")
    print(f"\n  {'operation':22} {'before':>8} {'after':>8} {'Δ':>8}")
    for name, b, av in movers:
        print(f"  {name:22} {b:>8} {av:>8} {av - b:>+8}")
    if a.export:
        os.makedirs(a.export, exist_ok=True)
        base = f"{a.export}/{a.task}_{a.encoding}_{a.scale}"
        open(base + "_before.smt2", "w").write(g0.sexpr())
        open(base + "_after.smt2", "w").write(g1.sexpr())
        print(f"\n  exported {base}_{{before,after}}.smt2")


def cmd_list(a):
    print("z3", z3.get_version_string())
    print(f"\ntheories ({len(all_theories())}): {', '.join(all_theories())}")
    print(f"\ntactics: {', '.join(str(t) for t in tactics.TACTICS)}")
    print(f"\ntactic sequences per case: "
          + ", ".join(f"L={k}:{tactics.count(k)}" for k in range(1, len(tactics.TACTICS) + 1)))
    print("\ntasks:")
    for name, tk in TASKS.items():
        encs = ", ".join(f"{e.name}({'+'.join(e.theories)})" for e in tk.encodings)
        print(f"  {name:14} scales={list(tk.scales)}  encodings: {encs}")


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)

    r = sub.add_parser("run", help="run the combinatorial sweep")
    r.add_argument("--tasks", nargs="*", default=list(TASKS))
    r.add_argument("--max-len", type=int, default=2)
    r.add_argument("--reps", type=int, default=1)
    r.add_argument("--timeout", type=int, default=4000)
    r.add_argument("--out", default="results/run.csv")
    r.add_argument("--json")
    r.add_argument("--markdown")
    r.set_defaults(fn=cmd_run)

    rp = sub.add_parser("report", help="regenerate md/json from a CSV")
    rp.add_argument("csv")
    rp.add_argument("--markdown")
    rp.add_argument("--model-diff", dest="model_diff",
                    help="write a per-encoding baseline-vs-winning-tactic model diff")
    rp.add_argument("--translations",
                    help="DIR: dump before/after smt2 for every case + index.md")
    rp.add_argument("--json")
    rp.set_defaults(fn=cmd_report)

    pr = sub.add_parser("profile", help="model AST diff before/after tactics")
    pr.add_argument("task"); pr.add_argument("encoding"); pr.add_argument("scale", type=int)
    pr.add_argument("--tactics", default="simplify,propagate-values,solve-eqs,simplify")
    pr.add_argument("--export")
    pr.set_defaults(fn=cmd_profile)

    ls = sub.add_parser("list", help="list tasks / theories / tactics")
    ls.set_defaults(fn=cmd_list)

    args = ap.parse_args()
    args.fn(args)
