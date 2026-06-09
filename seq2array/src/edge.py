"""
Edge probe: out-of-bounds indexing.

Z3 spec: (seq.nth s i) for i<0 or i>=len(s) is UNDERSPECIFIED (some fixed but
unknown value of the element sort).  In the array rewrite, (select s_arr i)
for an out-of-range i is ALSO some unknown value -- but it is a DIFFERENT
unknown, drawn from the array's free interpretation, with no relation to what
the seq theory would pick.

This matters only for formulas whose satisfiability depends on an
out-of-bounds read.  We construct one and check whether the two encodings can
disagree.

Formula:  s : Seq Int, len s = 2 (bound 4), and we ASSERT something about
s[3] (out of bounds): s[3] = 7  AND  s[3] = 8.   This is a direct
contradiction on a SINGLE term, so both encodings must be unsat (the term,
whatever its value, can't equal both 7 and 8).

Second:   s[3] = 7 only.  Both should be sat (the unknown OOB value is free).
We report what each does.  A divergence here would be a real soundness bug.
"""
import subprocess, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from transform import (Env, IntLit as Lit, SeqLen, Nth, Cmp, BoolOp,
                       build_seq_doc, build_array_doc)
Z3 = "/usr/local/bin/z3"

def run(doc):
    p = subprocess.run([Z3, "-in"], input=doc, capture_output=True, text=True)
    return p.stdout.strip().splitlines()[0]

def probe(name, asserts_fn):
    e = Env(); s = e.seq("s", 4)
    asserts = asserts_fn(s)
    sres = run(build_seq_doc(e, asserts))
    ares = run(build_array_doc(e, asserts))
    flag = "  <-- DIVERGENCE" if sres != ares else ""
    print(f"{name:<34} seq={sres:<7} arr={ares:<7}{flag}")

def main():
    print("Out-of-bounds indexing probes (s[3] with len 2):\n")
    probe("oob contradiction (s[3]=7 & =8)",
          lambda s: [Cmp("=", SeqLen(s), Lit(2)),
                     Cmp("=", Nth(s, Lit(3)), Lit(7)),
                     Cmp("=", Nth(s, Lit(3)), Lit(8))])
    probe("oob single read (s[3]=7)",
          lambda s: [Cmp("=", SeqLen(s), Lit(2)),
                     Cmp("=", Nth(s, Lit(3)), Lit(7))])
    probe("in-bounds read (s[1]=7)",
          lambda s: [Cmp("=", SeqLen(s), Lit(2)),
                     Cmp("=", Nth(s, Lit(1)), Lit(7))])
    print("\nNote: agreement on sat/unsat here is incidental, NOT guaranteed:")
    print("the OOB value is an UNCONSTRAINED free term in both theories, so")
    print("any formula whose truth hinges on a SPECIFIC OOB value is fragile")
    print("in BOTH encodings.  The rewrite preserves 'some free value' but not")
    print("'the SAME free value Z3's seq theory would choose'.")

if __name__ == "__main__":
    main()
