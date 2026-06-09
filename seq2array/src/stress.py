"""
Stress test: scale a bounded-Seq problem and look for cases where Z3's
sequence theory returns `unknown` / times out while the unrolled array+len
form stays decidable and fast.

Scenario family (parametric in N):
  s : (Seq Int), (<= (seq.len s) N), len = N
  strictly increasing:  forall i. s[i] < s[i+1]
  range-bounded:        forall i. 0 <= s[i] <= K
With K = N-2 and len = N this is UNSAT by pigeonhole (N strictly increasing
values cannot fit in N-1 slots).  Distinctness/pigeonhole over a symbolic
sequence is exactly the kind of thing the seq theory handles poorly.

Also a SAT variant with K = N-1 (a forced 0..N-1 ramp).
"""

import subprocess, time, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from transform import (Env, IntLit as Lit, SeqLen, Nth, Cmp, BoolOp,
                       ForallAdj, ForallIdx, build_seq_doc, build_array_doc)

Z3 = "/usr/local/bin/z3"

def run(doc, t_ms):
    t0 = time.perf_counter()
    p = subprocess.run([Z3, f"-t:{t_ms}", "-in"], input=doc,
                       capture_output=True, text=True)
    dt = time.perf_counter() - t0
    out = p.stdout.strip().splitlines()
    return (out[0] if out else "?"), dt

def build(N, K):
    e = Env()
    s = e.seq("s", N)
    asserts = [
        Cmp("=", SeqLen(s), Lit(N)),
        ForallAdj(s, lambda a, b: Cmp("<", a, b)),
        ForallIdx(s, lambda el: BoolOp("and",
                    [Cmp(">=", el, Lit(0)), Cmp("<=", el, Lit(K))])),
    ]
    return e, asserts

def main():
    t_ms = 5000
    print(f"per-query timeout = {t_ms} ms\n")
    print(f"{'N':>3} {'kind':<7} {'expect':<7} "
          f"{'seq_res':<9}{'seq_s':>9}  {'arr_res':<9}{'arr_s':>9}")
    print("-" * 64)
    for N in [4, 6, 8, 10, 12, 15, 18, 20, 25]:
        for K, expect in [(N - 2, "unsat"), (N - 1, "sat")]:
            e, a = build(N, K)
            sdoc = build_seq_doc(e, a)
            adoc = build_array_doc(e, a)
            sres, st = run(sdoc, t_ms)
            ares, at = run(adoc, t_ms)
            flag = ""
            if sres not in ("sat", "unsat"):
                flag = "  <-- seq UNKNOWN/timeout"
            if sres in ("sat","unsat") and ares in ("sat","unsat") and sres != ares:
                flag = "  <-- DIVERGENCE"
            kind = "pigeon" if expect == "unsat" else "ramp"
            print(f"{N:>3} {kind:<7} {expect:<7} "
                  f"{sres:<9}{st:>9.3f}  {ares:<9}{at:>9.3f}{flag}")
    print()

if __name__ == "__main__":
    main()
