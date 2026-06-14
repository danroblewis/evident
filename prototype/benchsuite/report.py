"""Outputs: CSV, JSON, a markdown report, and a text summary."""
import csv
import json
import os
import hashlib
import difflib
from . import tactics, profiling, pretty
from .tasks import TASKS


def _safe_pp(g):
    """Faithful pretty-print of a Goal, never raising (reports fall through)."""
    try:
        return pretty.goal(g)
    except Exception as ex:                  # a prettifier bug must not kill a report
        return f"(prettify failed: {type(ex).__name__}: {ex})"


def _diff_block(before, after, after_label, cap=400):
    """A markdown ```diff render of before→after, or a note if too large."""
    bl, al = before.splitlines(), after.splitlines()
    if max(len(bl), len(al)) > cap:
        return (f"_models too large to inline ({len(bl)} → {len(al)} lines); "
                f"see the files above_")
    diff = list(difflib.unified_diff(bl, al, "before.smt2", after_label, lineterm="", n=2))
    if not diff:
        return "_identical to the baseline_"
    return "```diff\n" + "\n".join(diff) + "\n```"

FIELDS = ["task", "scale", "encoding", "theories", "combo", "combo_len",
          "result", "ok", "tactic_ms", "solve_ms", "total_ms", "rlimit", "note"]


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


def translations(rows_or_csv, outdir, theory=None, cap=400, dump_files=True):
    """Record the before/after smt2 for every case and index them in one report.

    For each (task, encoding, scale) group: dump the baseline model once, then
    every tactic sequence's resulting model — deduped by content (many sequences
    collapse to the same translation), so each distinct model is one file and the
    report lists every sequence that produces it. Groups are ordered best-first
    (fastest solving encoding first); rows within a group are ordered by total ms.
    Regenerates smt2 from the registry; the CSV supplies timing/result only.

    `theory` (e.g. 'set') restricts to encodings that use that theory — a focused
    report. `cap` is the inline-diff line ceiling (larger models link out).
    `dump_files=True`: `outdir` is a directory; write `index.md` + a `smt2/` tree.
    `dump_files=False`: `outdir` is a single self-contained `.md` path; inline the
    diffs and DON'T write any smt2 (over-cap models just note their size)."""
    rows = _load(rows_or_csv) if isinstance(rows_or_csv, str) else rows_or_csv
    if theory:
        keep = {(t.name, e.name) for t in TASKS.values() for e in t.encodings
                if theory in e.theories}
        rows = [r for r in rows if (r["task"], r["encoding"]) in keep]
    smtdir = os.path.join(outdir, "smt2") if dump_files else None
    if dump_files:
        os.makedirs(smtdir, exist_ok=True)

    groups = {}
    for r in rows:
        groups.setdefault((r["task"], r["encoding"], r["scale"]), []).append(r)

    def gbest(g):
        return min((r["total_ms"] for r in g if r["ok"]), default=float("inf"))

    what = (f"`{theory}`-theory model" if theory else "Every model")
    files_note = ("Each group's baseline is dumped once as `before.smt2` and each "
                  "distinct post-tactic model as `vNN_<hash>.smt2` under `smt2/`; "
                  if dump_files else "Each distinct post-tactic model is shown as "
                  "an inline diff below its group (self-contained — no smt2 files); ")
    head = [f"# {what} translations — before and after each tactic", "",
            f"{('Restricted to the **`' + theory + '`** theory. ') if theory else ''}"
            f"{len(rows)} cases over {len(groups)} (task, encoding, scale) groups. "
            f"{files_note}sequences yielding an identical model are grouped. Groups "
            "ordered fastest-encoding-first; rows within a group by total ms "
            "(apply + solve).", ""]
    summary = ["## Best translation per group", "",
               "| task | encoding | N | result | best ms | Δnodes | models |",
               "|---|---|--:|---|--:|--:|--:|"]
    body = []

    for task in sorted({k[0] for k in groups}):
        gkeys = sorted([k for k in groups if k[0] == task], key=lambda k: gbest(groups[k]))
        body.append(f"## {task}\n")
        for (t, enc, scale) in gkeys:
            g = groups[(t, enc, scale)]
            enc_obj = next(e for e in TASKS[t].encodings if e.name == enc)
            if enc_obj.build is None:        # solve-only: no Goal to translate
                continue
            base = enc_obj.build(scale)
            gname = f"{t}__{enc}__N{scale}"
            if dump_files:
                gdir = os.path.join(smtdir, gname)
                os.makedirs(gdir, exist_ok=True)
            before_sx = base.sexpr()
            if dump_files:
                open(os.path.join(gdir, "before.smt2"), "w").write(before_sx)
            base_nodes = profiling.profile(base)["dag_nodes"]

            variants, rowinfo = {}, []     # sexpr-hash -> [filename, nodes, sexpr, count]
            for r in g:
                seq = tactics.parse(r["combo"])
                if not seq:
                    rowinfo.append((r, "before.smt2", 0)); continue
                g2, _, err = tactics.apply(base, seq)
                if err or g2 is None:
                    rowinfo.append((r, None, None)); continue
                sx = g2.sexpr()
                h = hashlib.sha1(sx.encode()).hexdigest()[:8]
                if h not in variants:
                    fn = f"v{len(variants):02d}_{h}.smt2"
                    if dump_files:
                        open(os.path.join(gdir, fn), "w").write(sx)
                    variants[h] = [fn, profiling.profile(g2)["dag_nodes"], sx, 0,
                                   _safe_pp(g2)]
                variants[h][3] += 1
                fn, nodes = variants[h][0], variants[h][1]
                rowinfo.append((r, fn, nodes - base_nodes))

            rowinfo.sort(key=lambda ri: (not ri[0]["ok"], ri[0]["total_ms"]))
            ok = [r for r in g if r["ok"]]
            best = min(ok, key=lambda r: r["total_ms"]) if ok else None
            bms = f"{best['total_ms']:.2f}" if best else "—"
            bres = best["result"] if best else (g[0]["result"])
            bdn = next((dn for r, f, dn in rowinfo if best and r is best and dn is not None), 0)
            summary.append(f"| {t} | {enc} | {scale} | {bres} | {bms} | "
                           f"{bdn:+d} | {1 + len(variants)} |")

            def model_cell(fn):
                if not fn:
                    return "(tactic error)"
                return f"[`{fn}`](smt2/{gname}/{fn})" if dump_files else f"`{fn}`"
            baseline = (f"[`before.smt2`](smt2/{gname}/before.smt2)" if dump_files
                        else "`before.smt2`")
            before_pp = _safe_pp(base)
            body += [f"### {enc}  (N={scale}) — {1 + len(variants)} distinct models "
                     f"over {len(g)} sequences",
                     f"baseline: {baseline} ({base_nodes} nodes)", "",
                     "<details><summary>baseline model (faithful Z3-AST view)</summary>\n",
                     "```", before_pp, "```", "", "</details>", "",
                     "| rank | tactic sequence | result | total ms | Δnodes | model |",
                     "|--:|---|---|--:|--:|---|"]
            for i, (r, fn, dn) in enumerate(rowinfo, 1):
                dns = "—" if dn is None else f"{dn:+d}"
                body.append(f"| {i} | `{r['combo']}` | {r['result']} | "
                            f"{r['total_ms']:.2f} | {dns} | {model_cell(fn)} |")
            body.append("")

            body.append("**diffs vs baseline** — faithful AST view, before → after "
                         "(one per distinct model):\n")
            for fn, nodes, sx, cnt, pp in sorted(variants.values(), key=lambda v: v[1]):
                body += [f"<details><summary><code>{fn}</code> — {cnt} sequence(s), "
                         f"{nodes - base_nodes:+d} nodes</summary>\n",
                         _diff_block(before_pp, pp, fn, cap), "", "</details>", ""]
            body.append("")

    text = "\n".join(head + summary + [""] + body)
    if dump_files:
        report_path = os.path.join(outdir, "index.md")
        open(report_path, "w").write(text)
        return report_path, sum(len(fs) for _, _, fs in os.walk(smtdir))
    os.makedirs(os.path.dirname(outdir) or ".", exist_ok=True)
    open(outdir, "w").write(text)
    return outdir, 0


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
