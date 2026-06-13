"""Outputs: CSV, JSON, a markdown report, and a text summary."""
import csv
import json
from . import tactics, profiling
from .tasks import TASKS

FIELDS = ["task", "scale", "encoding", "theories", "combo", "combo_len",
          "result", "ok", "tactic_ms", "solve_ms", "total_ms", "rlimit"]


def write_csv(rows, path):
    with open(path, "w", newline="") as fh:
        w = csv.DictWriter(fh, fieldnames=FIELDS)
        w.writeheader()
        for r in rows:
            w.writerow({k: r.get(k) for k in FIELDS})


def write_json(rows, path):
    json.dump(rows, open(path, "w"), indent=0)


def _load(path):
    rows = list(csv.DictReader(open(path)))
    for r in rows:
        for k in ("solve_ms", "total_ms", "tactic_ms"):
            r[k] = float(r[k])
        r["scale"] = int(r["scale"])
        r["ok"] = r["ok"] in ("True", "true", True)
    return rows


def markdown(rows_or_csv, path, source=""):
    rows = _load(rows_or_csv) if isinstance(rows_or_csv, str) else rows_or_csv
    theories = sorted({t for r in rows for t in r["theories"].split("+")})
    out = ["# Z3 theory × encoding × tactic — benchmark report", "",
           f"{len(rows)} cases{(' from `' + source + '`') if source else ''}. "
           "`baseline` = no-tactic solve; `best` = fastest tactic sequence (apply+solve).",
           "", f"**Theories exercised ({len(theories)}):** "
           + ", ".join(f"`{t}`" for t in theories), ""]
    for task in sorted({r["task"] for r in rows}):
        tr = [r for r in rows if r["task"] == task]
        N = max(r["scale"] for r in tr)
        atN = [r for r in tr if r["scale"] == N]
        out += [f"## {task}  (N={N})", "",
                "| encoding | theories | result | baseline ms | best ms | best tactic sequence |",
                "|---|---|---|--:|--:|---|"]
        ranked = []
        for enc in {r["encoding"] for r in atN}:
            er = [r for r in atN if r["encoding"] == enc]
            ok = [r for r in er if r["ok"]]
            base = next((r for r in er if r["combo"] == "(none)"), None)
            best = min(ok, key=lambda r: r["total_ms"]) if ok else None
            ranked.append((enc, er[0]["theories"],
                           base["result"] if base else "?",
                           base["solve_ms"] if base else float("inf"),
                           best["total_ms"] if best else float("inf"),
                           best["combo"] if best else "—"))
        for enc, th, res, b, bt, combo in sorted(ranked, key=lambda x: x[3]):
            bs = f"{b:.1f}" if b != float("inf") else "—"
            bts = f"{bt:.1f}" if bt != float("inf") else "—"
            out.append(f"| {enc} | {th} | {res} | {bs} | {bts} | `{combo}` |")
        out.append("")
    open(path, "w").write("\n".join(out))


def model_diff(rows_or_csv, path):
    """For each encoding's WINNING tactic sequence, tabulate how the model
    changed: baseline vs after, with the largest operation-count movers.
    Rebuilds goals from the task registry; the CSV only names the sequence."""
    rows = _load(rows_or_csv) if isinstance(rows_or_csv, str) else rows_or_csv
    out = ["# How the winning tactic reshaped each model", "",
           "Per encoding (at its largest scale): the baseline model vs the model "
           "after its fastest tactic sequence. `Δsym`/`Δnodes` are distinct "
           "symbols and DAG nodes; *movers* are the operations whose count "
           "changed most (the structural reason for the speedup).", ""]
    for task in sorted({r["task"] for r in rows}):
        tr = [r for r in rows if r["task"] == task]
        N = max(r["scale"] for r in tr)
        atN = [r for r in tr if r["scale"] == N]
        out += [f"## {task}  (N={N})", "",
                "| encoding | best sequence | Δnodes | Δsym | top operation movers |",
                "|---|---|--:|--:|---|"]
        for enc_name in sorted({r["encoding"] for r in atN}):
            er = [r for r in atN if r["encoding"] == enc_name]
            enc = next(e for e in TASKS[task].encodings if e.name == enc_name)
            ok = [r for r in er if r["ok"]]
            best = min(ok, key=lambda r: r["total_ms"]) if ok else None
            if not best or best["combo"] == "(none)":
                out.append(f"| {enc_name} | `(none)` | — | — | baseline already best |")
                continue
            g0 = enc.build(N)
            g1, _, err = tactics.apply(g0, tactics.parse(best["combo"]))
            if err or g1 is None:
                out.append(f"| {enc_name} | `{best['combo']}` | — | — | (tactic error) |")
                continue
            p0, p1 = profiling.profile(g0), profiling.profile(g1)
            sc, movers = profiling.diff(p0, p1, top=4)
            dn = sc["dag_nodes"][2]
            ds = sc["symbols"][2]
            mv = ", ".join(f"{k} {b}→{a}" for k, b, a in movers) or "—"
            out.append(f"| {enc_name} | `{best['combo']}` | {dn:+d} | {ds:+d} | {mv} |")
        out.append("")
    open(path, "w").write("\n".join(out))


def summarize(rows_or_csv):
    rows = _load(rows_or_csv) if isinstance(rows_or_csv, str) else rows_or_csv
    for task in sorted({r["task"] for r in rows}):
        tr = [r for r in rows if r["task"] == task]
        N = max(r["scale"] for r in tr)
        atN = [r for r in tr if r["scale"] == N]
        print(f"\n### {task}  (N={N})")
        base = sorted((r for r in atN if r["combo"] == "(none)" and r["ok"]),
                      key=lambda r: r["solve_ms"])
        for r in base:
            print(f"  {r['encoding']:11} {r['theories']:22} {r['solve_ms']:8.1f} ms")
        for enc in {r["encoding"] for r in atN}:
            er = [r for r in atN if r["encoding"] == enc and r["ok"]]
            if not er:
                continue
            b = next((r for r in er if r["combo"] == "(none)"), None)
            best = min(er, key=lambda r: r["total_ms"])
            if b and best["total_ms"] < b["solve_ms"] - 0.05:
                print(f"    {enc}: {b['solve_ms']:.1f}→{best['total_ms']:.1f}ms "
                      f"via [{best['combo']}]")
        unsound = [r for r in tr if r["result"] in ("sat", "unsat") and not r["ok"]]
        tmo = [r for r in tr if r["result"] == "unknown"]
        if unsound:
            print(f"  ⚠⚠ {len(unsound)} SOUNDNESS violations (tactic changed sat/unsat)")
        if tmo:
            print(f"  ⓘ {len(tmo)} tactic-induced timeouts (a tactic HURT)")
