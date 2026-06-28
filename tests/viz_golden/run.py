#!/usr/bin/env python3
"""run.py — the viz golden-standard runner. NON-BLOCKING by design.

Collects every golden case (each test_*.py module exposes a `case()` returning a case record),
renders + checks them, prints a per-(example, view, expectation) pass/fail table, and ALWAYS exits 0
— the suite is a STANDARD that reports which diagrams meet expert expectations, NOT a CI gate. A red
row is the signal that drives a fix; it must never break the build.

    python3 tests/viz_golden/run.py            # run all golden cases
    python3 tests/viz_golden/run.py --strict   # exit nonzero on any unmet expectation (opt-in, for
                                               #   a future dedicated lane — still OUT of ./test.sh)

Add a case: drop tests/viz_golden/test_<example>_<view>.py exposing `case()`; it is auto-discovered.
"""
import glob
import importlib
import os
import sys

_HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, _HERE)

from golden import report                                      # noqa: E402


def _discover():
    cases = []
    for path in sorted(glob.glob(os.path.join(_HERE, "test_*.py"))):
        mod = importlib.import_module(os.path.splitext(os.path.basename(path))[0])
        if hasattr(mod, "case"):
            cases.append(mod)
    return cases


def main(argv):
    strict = "--strict" in argv
    records = [m.case() for m in _discover()]
    failed = report(records)
    if strict:
        return 1 if failed else 0
    return 0                                                    # default: never gate CI


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
