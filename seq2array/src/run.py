"""
Harness: for each test, build the seq-theory doc and the array+len doc, run
z3 on both, compare sat/unsat to each other and to expected ground truth,
time both, and (for sat array cases) reconstruct the model and run the
python-side consistency checker.

Usage:  python3 run.py            # full battery
        python3 run.py --dump     # also write each test's SMT2 to out/
"""

import subprocess, time, sys, os, re

sys.path.insert(0, os.path.dirname(__file__))
from transform import (Env, build_seq_doc, build_array_doc, ArrayEmitter,
                       Unsupported)
import tests as T

Z3 = "/usr/local/bin/z3"
OUTDIR = os.path.join(os.path.dirname(__file__), "..", "out")
TIMEOUT_MS = 10000   # per-query soft timeout


def run_z3(doc, timeout_ms=TIMEOUT_MS):
    """Run z3 on an SMT2 string; return (result, wall_seconds, raw_stdout)."""
    t0 = time.perf_counter()
    p = subprocess.run([Z3, f"-t:{timeout_ms}", "-in"],
                       input=doc, capture_output=True, text=True)
    dt = time.perf_counter() - t0
    out = p.stdout.strip()
    first = out.splitlines()[0].strip() if out else ""
    return first, dt, out


# ---- tiny s-expr parser for (get-value ...) output -----------------------

def parse_sexpr(s):
    toks = re.findall(r'\(|\)|[^\s()]+', s)
    pos = 0
    def parse():
        nonlocal pos
        if toks[pos] == '(':
            pos += 1
            lst = []
            while toks[pos] != ')':
                lst.append(parse())
            pos += 1
            return lst
        else:
            t = toks[pos]; pos += 1
            return t
    out = []
    while pos < len(toks):
        out.append(parse())
    return out


def as_int(v):
    # v is either a token like "3" or a list ['-', '5']
    if isinstance(v, list) and len(v) == 2 and v[0] == '-':
        return -int(v[1])
    return int(v)


def reconstruct_model(env, asserts):
    """Run the array doc with get-value queries; return {seqname: [elems]}."""
    em = ArrayEmitter(env)
    decls = em.decls()
    body = [em.b(a) for a in asserts]
    lines = ["(set-logic QF_AUFLIA)"]
    lines += decls + em.extra_decls + em.bound_asserts() + em.extra_asserts
    for s in body:
        lines.append(f"(assert {s})")
    lines.append("(check-sat)")
    # build get-value queries for every top-level seq var
    queries = []
    for name, N in env.seq_vars.items():
        arr, ln = em.varmap[name]
        queries.append(ln)
        for k in range(N):
            queries.append(f"(select {arr} {k})")
    lines.append(f"(get-value ({' '.join(queries)}))")
    doc = "\n".join(lines) + "\n"
    p = subprocess.run([Z3, "-in"], input=doc, capture_output=True, text=True)
    out = p.stdout.strip()
    if not out.startswith("sat"):
        return None, out
    # strip leading 'sat'
    rest = out[out.index("("):]
    pairs = parse_sexpr(rest)[0]   # list of [expr, value]
    vals = {}
    for pair in pairs:
        expr, value = pair[0], pair[1]
        key = expr if isinstance(expr, str) else " ".join(_flat(expr))
        vals[key] = as_int(value)
    # assemble per-seqvar
    model = {}
    for name, N in env.seq_vars.items():
        arr, ln = em.varmap[name]
        length = vals[ln]
        elems = []
        for k in range(N):
            key = f"select {arr} {k}"
            elems.append(vals[key])
        model[name] = elems[:length]
    # also expose scalars
    return model, out


def _flat(x):
    if isinstance(x, str):
        return [x]
    out = []
    for e in x:
        out += _flat(e)
    return out


def main():
    dump = "--dump" in sys.argv
    if dump:
        os.makedirs(OUTDIR, exist_ok=True)
    rows = []
    n_ok = n_fail = 0
    tot_seq = tot_arr = 0.0
    for name, fn, expected, note in T.TESTS:
        try:
            env, asserts, chk = fn()
        except Exception as e:
            rows.append((name, "BUILD-ERR", str(e), 0, 0, ""))
            n_fail += 1
            continue
        seq_doc = build_seq_doc(env, asserts)
        try:
            arr_doc = build_array_doc(env, asserts)
            arr_ok = True
        except Unsupported as e:
            rows.append((name, "UNSUPPORTED", str(e), 0, 0, ""))
            continue
        if dump:
            open(os.path.join(OUTDIR, f"{name}.seq.smt2"), "w").write(seq_doc)
            open(os.path.join(OUTDIR, f"{name}.arr.smt2"), "w").write(arr_doc)

        seq_res, seq_t, _ = run_z3(seq_doc)
        arr_res, arr_t, _ = run_z3(arr_doc)
        tot_seq += seq_t; tot_arr += arr_t

        # model sanity for sat array cases
        model_note = ""
        if arr_res == "sat" and chk is not None:
            model, _ = reconstruct_model(env, asserts)
            if model is None:
                model_note = "model-extract-FAIL"
            else:
                model_note = "model-OK" if chk(model) else f"model-BAD:{model}"

        agree = (seq_res == arr_res)
        match_truth = (arr_res == expected and seq_res == expected)
        status = "OK" if (agree and match_truth and "BAD" not in model_note
                          and "FAIL" not in model_note) else "FAIL"
        if status == "OK":
            n_ok += 1
        else:
            n_fail += 1
        rows.append((name, status,
                     f"seq={seq_res} arr={arr_res} exp={expected}",
                     seq_t, arr_t, model_note))

    # report
    print(f"{'TEST':<26}{'STATUS':<7}{'DETAIL':<34}"
          f"{'seq(s)':>9}{'arr(s)':>9}{'speedup':>9}  model")
    print("-" * 120)
    for name, status, detail, st, at, mn in rows:
        sp = (st / at) if at > 0 else 0
        print(f"{name:<26}{status:<7}{detail:<34}"
              f"{st:>9.4f}{at:>9.4f}{sp:>8.1f}x  {mn}")
    print("-" * 120)
    sp = (tot_seq / tot_arr) if tot_arr else 0
    print(f"{'TOTAL':<26}{'':<7}{f'{n_ok} ok / {n_fail} fail':<34}"
          f"{tot_seq:>9.4f}{tot_arr:>9.4f}{sp:>8.1f}x")
    return 0 if n_fail == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
