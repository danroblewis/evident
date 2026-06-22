"""Claim-interrogation helpers: witness enumeration and minimal unsat cores.

`_enumerate` walks distinct witnesses by iterated source-level blocking (solve,
append a ¬(witness) constraint, re-solve until UNSAT or the limit). `_unsat_core`
does deletion-based minimization over the source's constraint lines for ONE minimal
core; `_all_unsat_cores` block-and-recurses over that machinery to enumerate EVERY
minimal core (MUS), so an over-constrained model shows all its independent
contradictions at once. Both drive `runtime_io._run_query`; `_block_term`/
`_block_clause` assemble the blocking constraints from a witness's bindings.
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


def _candidate_lines(lines):
    """Indices of constraint lines eligible for the core: not header/comment/blank, and not a
    PURE declaration (removing `x ∈ Int` un-declares the var and cascades to drop its constraints,
    which would flip the claim SAT and falsely mark the decl a core member)."""
    cand = []
    for i, ln in enumerate(lines):
        s = ln.strip()
        if (not s or s.startswith("--") or s.split(" ", 1)[0] in _HEADER_KW
                or _PURE_DECL.match(s)):
            continue
        cand.append(i)
    return cand


def _is_unsat(lines, cand_set, keep, claim, work):
    """True iff the program with exactly `keep` of the candidate lines retained is UNSAT."""
    trial = "\n".join(ln for j, ln in enumerate(lines)
                      if j not in cand_set or j in keep)
    r = _run_query(trial, claim, None, work)
    return r.get("ok") and r.get("satisfied") is False


def _minimize(lines, cand_set, candidates, claim, work):
    """Deletion-based minimization restricted to `candidates` (a subset of the eligible lines).

    Returns a frozenset of line indices forming a genuine MINIMAL core (every member necessary,
    the set itself UNSAT) when `candidates` is unsatisfiable, else None. Starts from the whole
    candidate set and drops a line only when the program stays UNSAT without it — sound under
    redundant constraints, where "individual removal flips to SAT" would not be."""
    if not _is_unsat(lines, cand_set, set(candidates), claim, work):
        return None                                       # this candidate set is satisfiable
    keep = set(candidates)
    for i in list(candidates):
        if i not in keep:
            continue
        trial_keep = keep - {i}
        if _is_unsat(lines, cand_set, trial_keep, claim, work):
            keep = trial_keep                             # still UNSAT without i → redundant
    return frozenset(keep)


def _fmt_core(lines, core):
    return [{"line": i + 1, "text": lines[i].strip()} for i in sorted(core)]


def _ordered(cores):
    """Stable ordering of cores: by first line, then size — so the panel reads top-to-bottom."""
    return sorted(cores, key=lambda c: (min(c) if c else -1, len(c), sorted(c)))


def _unsat_core(source, claim, work):
    """A single MINIMAL unsat core by deletion-based minimization over the source's constraint
    lines. Header/decl/comment lines are never candidates. Line granularity; multi-line ∀ blocks
    may be missed."""
    lines = source.split("\n")
    cand = _candidate_lines(lines)
    core = _minimize(lines, set(cand), cand, claim, work)
    return _fmt_core(lines, core if core is not None else cand)


def _all_unsat_cores(source, claim, work, cap=8):
    """Enumerate ALL minimal unsat cores (MUSes), not just the one the solver returns first.

    An over-constrained model has independent contradictions; a user must fix ONE constraint from
    EACH to make it solvable. `x ∈ Int; x>10; x<5; x=7` has THREE: {x>10,x<5}, {x>10,x=7},
    {x<5,x=7} — show one and they fix it, only to have the next surface on re-solve.

    Block-and-recurse, bounded for the IDE's small claims (≤~12 constraints): find one MUS over the
    full candidate set; record it; then for each constraint in it, search for another MUS in the
    candidates with that line EXCLUDED (so any MUS independent of that line surfaces). Recurse on
    each new MUS the same way. Dedup by frozenset identity; cap the count. Each returned core is
    genuinely minimal (`_minimize` proves it). Returns (cores, complete): cores is a list of
    line-groups, complete is False when the cap was hit (more may exist)."""
    lines = source.split("\n")
    cand = _candidate_lines(lines)
    cand_set = set(cand)
    root = _minimize(lines, cand_set, cand, claim, work)
    if root is None:
        return [], True                                   # satisfiable → no cores
    found, stack = {root}, [(root, frozenset(cand))]
    while stack:
        if len(found) >= cap:
            return [_fmt_core(lines, c) for c in _ordered(found)], False
        core, pool = stack.pop()
        for i in core:                                    # exclude one member, hunt the rest
            sub = pool - {i}
            mus = _minimize(lines, cand_set, sorted(sub), claim, work)
            if mus is not None and mus not in found:
                found.add(mus)
                stack.append((mus, sub))
    return [_fmt_core(lines, c) for c in _ordered(found)], True
