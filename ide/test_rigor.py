#!/usr/bin/env python3
"""#285 honesty marker — view_rigor classifies each diagram as proven / exhaustive / sampled, so a
sampled cloud is never mistaken for a proof. The classification is correctness-sensitive (a wrong class
is dishonest), so pin it: abstract-Z3 views are 'proven'; the enumerate family is 'exhaustive' only when
the graph is COMPLETE (not capped, not continuous) else 'sampled'; trajectory views are 'sampled'.
"""
import sys

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from render import view_rigor                            # noqa: E402

CASES = [
    ("terminal_map", 0, 0, "proven"),                   # abstract Z3 over the one-step relation
    ("reachable_region", 0, 0, "proven"),               # k-induction bounding box
    ("solution_space", 0, 0, "proven"),
    ("solution_structure", 0, 0, "proven"),
    ("function_guards", 0, 0, "proven"),                # the compiled structure
    ("state_graph", 0, 0, "exhaustive"),                # the COMPLETE bounded-discrete graph
    ("state_graph", 1, 0, "sampled"),                   # capped → not exhaustive, honestly downgraded
    ("state_graph", 0, 1, "sampled"),                   # continuous → can't enumerate
    ("reachability_tree", 0, 0, "exhaustive"),
    ("phase_portrait", 0, 0, "sampled"),                # sampled trajectories
    ("cobweb", 0, 0, "sampled"),
]


def main():
    fails = []
    for view, capped, cont, want in CASES:
        got = view_rigor(view, capped, cont)
        if got != want:
            fails.append(f"view_rigor({view!r}, capped={capped}, cont={cont}) = {got!r}, want {want!r}")
    if fails:
        print("RIGOR-MARKER FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print("✓ rigor (#285): the honesty marker classifies abstract-Z3 views proven, the complete "
          "bounded-discrete graph exhaustive, a capped/continuous fallback or a trajectory view sampled "
          "— a sampled cloud never reads as a proof")
    return 0


if __name__ == "__main__":
    sys.exit(main())
