#!/usr/bin/env python3
"""Test the VALUE-SYMMETRY witness fold (Ana #271/#16/#122/#257).

The witness walker emits EVERY distinct assignment, including the 3! relabelings of an
interchangeable-colour 3-colouring. `symmetry.fold_witnesses` collapses each symmetry orbit to one
canonical representative + a count — but ONLY when the symmetry is PROVABLE from the source. The
whole risk is UNSOUNDNESS: folding two genuinely-distinct witnesses. So this test pins BOTH
directions — folds when (and only when) interchangeability is certain, never otherwise.

Three model shapes, each its own soundness check:

  - COLOURING (enum Hue = Red|Green|Blue; constraints only say adjacent differ; no colour NAMED;
    no ordering) → values PROVABLY interchangeable. The raw witnesses fold into orbits of 3! = 6,
    every distinct colouring collapses to ONE rep "(×6 symmetric)".
  - NAMED (a constraint pins `a = Red`) → Red is distinguished, so the enum is NOT folded; every
    raw witness stays its own orbit (no over-claim).
  - ORDERED (an enum-typed var compared with `≤`) → an ordering is imposed, NOT folded.

Run from repo root: `python3 ide/test_symmetry.py` (exit non-zero on any failure)."""
import sys
import tempfile

sys.path.insert(0, "ide/web")

from runtime_io import _run_query                            # noqa: E402
from solve import _enumerate                                 # noqa: E402
from symmetry import fold_witnesses, interchangeable_enums   # noqa: E402

# COLOURING: a triangle 3-colouring. NO colour named in a constraint, NO ordering — Red/Green/Blue
# are interchangeable. The 6 distinct solutions are one orbit under S_3 (permute the 3 colours).
COLORING = (
    "enum Hue = Red | Green | Blue\n"
    "claim coloring\n"
    "    a ∈ Hue\n"
    "    b ∈ Hue\n"
    "    c ∈ Hue\n"
    "    a ≠ b\n"
    "    b ≠ c\n"
    "    a ≠ c\n")

# NAMED: same shape, but `a = Red` pins one colour by name → Red is distinguished. NOT interchangeable.
NAMED = (
    "enum Hue = Red | Green | Blue\n"
    "claim coloring\n"
    "    a ∈ Hue\n"
    "    b ∈ Hue\n"
    "    c ∈ Hue\n"
    "    a = Red\n"
    "    a ≠ b\n"
    "    b ≠ c\n"
    "    a ≠ c\n")

# ORDERED: an enum-typed var compared with `≤` imposes an order on the values → NOT interchangeable.
ORDERED = (
    "enum Hue = Red | Green | Blue\n"
    "claim ordered\n"
    "    a ∈ Hue\n"
    "    b ∈ Hue\n"
    "    a ≤ b\n"
    "    a ≠ b\n")


def _enum(src, claim, work, limit=40):
    rclaim, sols, complete, err = _enumerate(src, claim, None, limit, work)
    if err and not sols:
        raise RuntimeError(f"enumerate failed: {err}")
    return sols, complete


def main():
    fails = []

    # ── COLOURING: PROVABLY interchangeable → folds 6→1 per orbit ────────────
    with tempfile.TemporaryDirectory() as work:
        if "Hue" not in interchangeable_enums(COLORING):
            fails.append("coloring: expected Hue PROVABLY interchangeable (no value named, no order)")
        sols, complete = _enum(COLORING, "coloring", work)
        raw_n = len(sols)
        folded, sets, raw = fold_witnesses(COLORING, sols)
        if "Hue" not in sets:
            fails.append(f"coloring fold: expected Hue in folded_sets, got {sets!r}")
        # Every orbit is the full S_3 (6 relabelings of a proper 3-colouring) — all mult 6.
        if not folded or any(o["multiplicity"] != 6 for o in folded):
            fails.append(f"coloring fold: expected every orbit ×6, got "
                         f"{[o['multiplicity'] for o in folded]!r}")
        if raw_n and len(folded) != raw_n // 6:
            fails.append(f"coloring fold: expected {raw_n // 6} orbits from {raw_n} raw, "
                         f"got {len(folded)}")
        if sum(o["multiplicity"] for o in folded) != raw_n:
            fails.append("coloring fold: multiplicities must sum to the raw witness count")
        # SOUNDNESS — each canonical rep is itself a genuine, DISTINCT witness of the claim. The rep's
        # bindings are a real solution (it's a raw witness we kept verbatim), and no two reps share an
        # orbit key, so we never merged two genuinely-different colourings.
        for o in folded:
            r = _run_query(COLORING, "coloring", o["bindings"], work)
            if not (r.get("ok") and r.get("satisfied")):
                fails.append(f"coloring fold: a canonical rep is NOT a real witness: {o['bindings']!r}")
                break

    # ── NAMED: a value named in a constraint → NOT folded (Red distinguished) ─
    with tempfile.TemporaryDirectory() as work:
        if interchangeable_enums(NAMED):
            fails.append(f"named: a constraint names `Red` — Hue must NOT be interchangeable, "
                         f"got {interchangeable_enums(NAMED)!r}")
        sols, _ = _enum(NAMED, "coloring", work)
        folded, sets, raw = fold_witnesses(NAMED, sols)
        if sets:
            fails.append(f"named fold: expected NO folded sets (Red named), got {sets!r}")
        if len(folded) != len(sols) or any(o["multiplicity"] != 1 for o in folded):
            fails.append("named fold: every witness must stay its own orbit (mult 1) — no over-claim")

    # ── ORDERED: enum compared with ≤ → NOT folded ──────────────────────────
    with tempfile.TemporaryDirectory() as work:
        if interchangeable_enums(ORDERED):
            fails.append(f"ordered: `a ≤ b` orders Hue — must NOT be interchangeable, "
                         f"got {interchangeable_enums(ORDERED)!r}")
        sols, _ = _enum(ORDERED, "ordered", work)
        folded, sets, raw = fold_witnesses(ORDERED, sols)
        if sets:
            fails.append(f"ordered fold: expected NO folded sets (ordering imposed), got {sets!r}")
        if len(folded) != len(sols):
            fails.append("ordered fold: every witness must stay its own orbit — no over-claim")

    if fails:
        print("SYMMETRY-FOLD TEST FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print("✓ symmetry fold: coloring folds S_3 orbits ×6 (each rep a real witness); NAMED and "
          "ORDERED enums stay UNFOLDED (sound — no over-claim)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
