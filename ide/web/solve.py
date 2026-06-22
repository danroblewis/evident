"""Claim-interrogation helpers: witness enumeration and minimal unsat cores.

`_enumerate` walks distinct witnesses by iterated source-level blocking (solve,
append a ¬(witness) constraint, re-solve until UNSAT or the limit). `_unsat_core`
does deletion-based minimization over the source's constraint lines. Both drive
`runtime_io._run_query`; `_block_term`/`_block_clause` assemble the blocking
constraints from a witness's bindings.
"""
import re

from runtime_io import _run_query

_HEADER_KW = ("claim", "type", "enum", "fsm", "schema", "import", "assert")

# A pure declaration: `names ∈ Type` with NO constraining comparison. Removing one un-declares
# its variable, which silently DROPS the constraints that referenced it — flipping the claim to
# SAT and making the declaration falsely look like a core member ("remove any one makes it
# solvable" is false for `x ∈ Int`). Exclude these from the delta-debug. A chained-membership
# that carries a bound (`0 ≤ x ∈ Int ≤ 5`) does NOT match (it has `≤`), so its bound stays a
# candidate.
_PURE_DECL = re.compile(r'^[A-Za-z_][\w, ]*∈\s*[A-Za-z_]\w*(\([^)]*\))?$')


def _block_term(name, val):
    """An Evident expression true for THIS witness value — assembled into a ¬(…) blocking
    constraint so enumeration can ask for a *different* solution."""
    if isinstance(val, bool):
        return f"{name} = {'true' if val else 'false'}"
    if isinstance(val, (int, float)):
        return f"{name} = {val}"
    if isinstance(val, str):
        # an enum-variant label (Idle) compares bare; a quoted string compares quoted.
        ident = val and (val[0].isalpha() or val[0] == "_") and "(" not in val
        return f"{name} = {val}" if ident else f'{name} = "{val}"'
    if isinstance(val, list):
        terms = []
        for i, el in enumerate(val):
            t = _block_term(f"{name}[{i}]", el)
            if t is None:
                return None
            terms.append(t)
        return "(" + " ∧ ".join(terms) + ")" if terms else None
    if isinstance(val, dict):                      # a record witness (e.g. sudoku's boxes elements,
        terms = []                                 # toposort edges) — block each field by dotted name
        for fld, fv in sorted(val.items()):
            t = _block_term(f"{name}.{fld}", fv)
            if t is None:
                return None
            terms.append(t)
        return "(" + " ∧ ".join(terms) + ")" if terms else None
    return None                                    # genuinely unsupported → can't block


def _block_clause(bindings):
    terms = []
    for k, v in sorted(bindings.items()):
        t = _block_term(k, v)
        if t is None:
            return None
        terms.append(t)
    return "¬(" + " ∧ ".join(terms) + ")" if terms else None


def _enumerate(source, claim, given, limit, work):
    """Walk distinct witnesses by iterated source-level blocking: solve, append a ¬(witness)
    constraint to the claim body, re-solve, until UNSAT (complete) or the limit (≥limit)."""
    sols, blocks, resolved_claim = [], [], claim
    for _ in range(limit):
        src = source if not blocks else source.rstrip() + "\n" + "\n".join("    " + b for b in blocks) + "\n"
        r = _run_query(src, claim, given, work)
        if not r.get("ok"):
            return resolved_claim, sols, len(sols) > 0, r.get("error")  # blocking broke parse → stop
        resolved_claim = r.get("claim") or resolved_claim
        if not r.get("satisfied"):
            return resolved_claim, sols, True, None                     # exhausted → complete
        b = r.get("bindings", {})
        sols.append(b)
        clause = _block_clause(b)
        if clause is None:
            return resolved_claim, sols, False, None                    # can't block → incomplete
        blocks.append(clause)
    return resolved_claim, sols, False, None                            # hit limit → ≥limit


def _unsat_core(source, claim, work):
    """A MINIMAL unsat core by deletion-based minimization over the source's constraint lines.

    The naive "a line whose individual removal flips to SAT is in the core" is UNSOUND when
    constraints are redundant: for {x>3, x>5, y>5, y<100, x+y<10} it drops x>5 (removing it still
    leaves x>3 ⇒ SAT) yet reports a SATISFIABLE set as 'the core'. Instead: start with every
    constraint line, and drop a line ONLY when the program stays UNSAT without it. The residual is
    a genuine minimal core — every member is necessary AND the set itself is unsatisfiable.

    Header/decl/comment lines are never candidates (a pure decl's removal un-declares a var and
    cascades to drop its constraints). Line granularity; multi-line ∀ blocks may be missed."""
    lines = source.split("\n")
    cand = []
    for i, ln in enumerate(lines):
        s = ln.strip()
        if (not s or s.startswith("--") or s.split(" ", 1)[0] in _HEADER_KW
                or _PURE_DECL.match(s)):
            continue
        cand.append(i)
    cand_set = set(cand)
    keep = set(cand)
    for i in cand:
        trial_keep = keep - {i}
        trial = "\n".join(ln for j, ln in enumerate(lines)
                          if j not in cand_set or j in trial_keep)
        r = _run_query(trial, claim, None, work)
        if r.get("ok") and r.get("satisfied") is False:   # still UNSAT without line i → redundant
            keep = trial_keep
    return [{"line": i + 1, "text": lines[i].strip()} for i in sorted(keep)]
