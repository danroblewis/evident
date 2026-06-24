#!/usr/bin/env python3
"""Pure-logic test for the saved-witness gallery's diff/compare helpers (Tasks #235/#170).

The gallery's interesting logic — which leaves differ between two witnesses, which Seq
indices changed, the per-variable compare rows — is pure (no DOM), exported from
app-gallery.js under `module.exports`. This shells out to `node`, requires that module,
and asserts the helpers on hand-built witness pairs. Auto-discovered by test.sh as an
`ide/test_*.py` phase. Skips cleanly (prints ✓) if node isn't on PATH.
"""
import json
import shutil
import subprocess
import sys
from pathlib import Path

GALLERY = Path(__file__).resolve().parent / "web" / "static" / "app-gallery.js"

# Each case: [witnessA, witnessB, expected diffWitnesses (sorted leaf paths),
#             expected seqDiffIndices over the named top-level Seq var (or null),
#             the var name for that seqDiff, expected changed-keys from compareRows]
CASES = [
    # scalar diff: only `y` changes
    [{"x": 1, "y": 2}, {"x": 1, "y": 5}, ["y"], None, None, ["y"]],
    # identical → no diff anywhere
    [{"x": 1}, {"x": 1}, [], None, None, []],
    # Seq diff: subset-sum-style, differs at indices 1 and 2
    [{"take": [3, 1, 4]}, {"take": [3, 9, 8]}, ["take[1]", "take[2]"], [1, 2], "take", ["take"]],
    # record-Seq: nested leaf path, one field of one element changed
    [{"items": [{"w": 2, "t": True}]}, {"items": [{"w": 2, "t": False}]},
     ["items[0].t"], [0], "items", ["items"]],
    # added/removed top-level var counts as a difference
    [{"a": 1}, {"a": 1, "b": 2}, ["b"], None, None, ["b"]],
    # different lengths → trailing index is a difference
    [{"s": [1, 2]}, {"s": [1, 2, 3]}, ["s[2]"], [2], "s", ["s"]],
]

DRIVER = r"""
const G = require(process.argv[1]);
const cases = JSON.parse(process.argv[2]);
let i = 0;
for (const [a, b, expDiff, expSeq, seqVar, expChanged] of cases) {
  const eq = (got, exp, what) => {
    if (JSON.stringify(got) !== JSON.stringify(exp)) {
      console.error(`case ${i} ${what}: got ${JSON.stringify(got)} want ${JSON.stringify(exp)}`);
      process.exit(1);
    }
  };
  eq(G.diffWitnesses(a, b), expDiff, "diffWitnesses");
  // diff is symmetric
  eq(G.diffWitnesses(b, a), expDiff, "diffWitnesses(symmetric)");
  if (seqVar !== null) eq(G.seqDiffIndices(a[seqVar], b[seqVar]), expSeq, "seqDiffIndices");
  const changed = G.compareRows(a, b).filter((r) => r.changed).map((r) => r.key).sort();
  eq(changed, expChanged, "compareRows.changed");
  // compareRows covers the UNION of keys, sorted, no dupes
  const keys = G.compareRows(a, b).map((r) => r.key);
  const union = [...new Set([...Object.keys(a), ...Object.keys(b)])].sort();
  eq(keys, union, "compareRows.keys");
  i++;
}
// seqDiffIndices on a non-array is empty (drives "no cell-level note" in the UI)
if (G.seqDiffIndices(5, 6).length !== 0) { console.error("non-array seqDiff"); process.exit(1); }
console.log(i);
"""


def main() -> int:
    node = shutil.which("node")
    if not node:
        print("✓ witness-gallery diff: skipped (node not on PATH)")
        return 0
    if not GALLERY.exists():
        print(f"✗ app-gallery.js missing at {GALLERY}", file=sys.stderr)
        return 1
    res = subprocess.run(
        [node, "-e", DRIVER, str(GALLERY), json.dumps(CASES)],
        capture_output=True, text=True,
    )
    if res.returncode != 0:
        sys.stderr.write(res.stdout + res.stderr)
        return 1
    n = res.stdout.strip().splitlines()[-1]
    print(f"✓ witness-gallery diff: {n} witness-pair cases "
          f"(diffWitnesses / seqDiffIndices / compareRows) pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
