#!/usr/bin/env python3
"""Test multiple-entry routing through the IDE's `_export` (#290).

A program may now declare several fsms AND several claims; the runtime renders the LAST-DEFINED
fsm-or-claim in source order, and the IDE's entry picker can override that with an explicit name.
This pins the schema.json the export produces for the six routing cases plus the override + single
cases. The schema carries `{"fsm": NAME}` or `{"claim": NAME}` — exactly the rendered entry.

Run from repo root: `python3 ide/test_multi_entry.py` (exit non-zero on any failure)."""
import json
import sys
import tempfile

sys.path.insert(0, "ide/web")

from runtime_io import _export                              # noqa: E402

FSM_COUNTER = (
    "    count ∈ Int\n"
    "    is_first_tick ⇒ count = 0\n"
    "    ¬is_first_tick ⇒ Δcount = 1\n"
    "    last_results ∈ Seq(Result)\n"
    "    effects ∈ Seq(Effect) = ⟨⟩\n")


def fsm(name):
    return f"fsm {name}\n{FSM_COUNTER}"


def rendered(src, entry=None):
    """(kind, name) of the entry the export rendered — ('fsm'|'claim', NAME) — or ('error', msg)."""
    with tempfile.TemporaryDirectory() as work:
        ok, prefix, _dropped, msg = _export(src, work, entry)
        if not ok:
            return ("error", msg.splitlines()[0][:80] if msg else "export failed")
        sch = json.load(open(prefix + ".schema.json"))
        if "fsm" in sch:
            return ("fsm", sch["fsm"])
        if "claim" in sch:
            return ("claim", sch["claim"])
        return ("error", "schema has neither fsm nor claim key")


CASES = [
    # (label, source, entry-override, expected (kind, name))
    ("claim-then-fsm → fsm",  "claim helper(x ∈ Int)\n    x > 0\n\n" + fsm("main"), None, ("fsm", "main")),
    ("fsm-then-claim → claim", fsm("main") + "\nclaim test(x ∈ Int)\n    0 < x < 10\n", None, ("claim", "test")),
    ("two fsms → last",        fsm("a") + "\n" + fsm("b"), None, ("fsm", "b")),
    ("two claims → last",      "claim p(x ∈ Int)\n    0 < x < 5\n\nclaim q(y ∈ Int)\n    10 < y < 20\n", None, ("claim", "q")),
    ("single fsm",             fsm("main"), None, ("fsm", "main")),
    ("single claim + helper type", "type Edge(from, to ∈ Int)\n\nclaim solo(x ∈ Int)\n    0 < x < 5\n", None, ("claim", "solo")),
    # explicit entry picker overrides the last-defined default
    ("override earlier fsm",   fsm("a") + "\n" + fsm("b"), "a", ("fsm", "a")),
    ("override fsm over claim", fsm("main") + "\nclaim test(x ∈ Int)\n    0 < x < 10\n", "main", ("fsm", "main")),
]


def main():
    fails = []
    for label, src, entry, want in CASES:
        got = rendered(src, entry)
        if got != want:
            fails.append(f"{label}: expected {want}, got {got}")
    # a bogus picker selection is reported as an error, not silently rendered
    got = rendered(fsm("main") + "\nclaim test(x ∈ Int)\n    0 < x < 10\n", "nope")
    if got[0] != "error" or "not found" not in got[1]:
        fails.append(f"bogus entry: expected a 'not found' error, got {got}")

    if fails:
        print("MULTI-ENTRY ROUTING TEST FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print(f"✓ multi-entry routing: {len(CASES)} cases + override + bogus-entry all render the right entry")
    return 0


if __name__ == "__main__":
    sys.exit(main())
