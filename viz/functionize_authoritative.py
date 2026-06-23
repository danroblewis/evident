#!/usr/bin/env python3
"""functionize_authoritative.py — consume the runtime's AUTHORITATIVE functionizer dump.

`evident functions <file>` emits the REAL `Z3Program` the runtime compiled (see
runtime/src/session/query.rs `export_functions`). This module parses that JSON into the same shape
`functionize.extract_functions` produces, so the function views can render the authoritative
decomposition instead of the Python re-derivation.

Why a separate path (not just `extract_functions`): the re-derivation loads the model via
`z3.parse_smt2_file`, which BREAKS on effect-heavy FSMs — the runtime's SMT-LIB datatype accessors
trip z3's parser ("repeated accessor 'f0'"). `evident functions` never round-trips through SMT-LIB, so
it works where the re-derivation can't. This path uses ONLY JSON (functions dump + schema), no z3 —
so the z3-needing views (guard_analysis totality, behavior sampling) don't run here, but the structural
views (graph / residual / guards-tree / complexity) render fine.
"""
import json
import os
import re

from functionize import _step_deps


def _smt_and_atoms(guard):
    """Split an SMT-LIB `(and A B C)` into its top-level conjuncts [A, B, C] (balanced-paren aware); a
    non-`and` guard is a single atom. Feeds the guard-tree trie on the authoritative path."""
    g = guard.strip()
    if not g.startswith("(and "):
        return [g]
    inner = g[len("(and "):].rstrip()
    if inner.endswith(")"):
        inner = inner[:-1]
    atoms, depth, cur = [], 0, ""
    for ch in inner:
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        if ch == " " and depth == 0:
            if cur.strip():
                atoms.append(cur.strip())
            cur = ""
        else:
            cur += ch
    if cur.strip():
        atoms.append(cur.strip())
    return atoms or [g]


def load_authoritative(functions_json_path, schema_json_path):
    """Parse `evident functions` output into the `extract_functions`-shaped dict, JSON-only (no z3).
    deps come from substring-matching the schema's carried-prev names + the tick selectors against each
    expr string. Returns the same shape as extract_functions, tagged `authoritative`, or None if the
    dump is absent."""
    if not os.path.exists(functions_json_path):
        return None
    prog = json.load(open(functions_json_path))
    schema = json.load(open(schema_json_path))
    drivers = [v["prev"] for v in schema.get("state", []) if v.get("prev")]
    drivers += [schema.get("is_first_tick", "is_first_tick"), "is_second_tick"]
    drivers = [d for d in drivers if d]

    def deps_of(*strs):
        s = " ".join(strs)
        return [d for d in drivers if re.search(r"(?<![\w])" + re.escape(d) + r"(?![\w])", s)]

    steps = []
    for st in prog.get("steps", []):
        k = st["kind"]
        if k == "guarded":
            branches = [{"guard": b["guard"], "guard_atoms": _smt_and_atoms(b["guard"]), "body": b["body"],
                         "deps": deps_of(b["guard"], b["body"])} for b in st["branches"]]
            steps.append({"var": st["var"], "kind": "guarded", "branches": branches})
        elif k == "seq":
            steps.append({"var": st["var"], "kind": "seq", "elem_exprs": st["elems"],
                          "deps": deps_of(*st["elems"])})
        else:                                              # scalar | prebaked
            expr = st.get("expr", st.get("value", ""))
            steps.append({"var": st["var"], "kind": "scalar", "expr": expr, "deps": deps_of(expr)})

    residual = ([{"expr": f"{c[0]} = {c[1]}", "deps": deps_of(c[0], c[1])} for c in prog.get("checks", [])]
                + [{"expr": p, "deps": deps_of(p)} for p in prog.get("predicates", [])])
    outputs = [s["var"] for s in steps]
    referenced = {d for s in steps for d in _step_deps(s)} | {d for r in residual for d in r["deps"]}
    return {"outputs": outputs, "inputs": sorted(referenced - set(outputs)),
            "unfunctionized": [], "steps": steps, "residual": residual, "authoritative": True}
