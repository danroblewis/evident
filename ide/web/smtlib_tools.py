"""SMT-LIB export + ad-hoc query-predicate parsing.

`_ready_to_run` turns the runtime's emitted SMT-LIB into a one-paste z3 session,
picking the tail (`(get-model)` vs `(get-unsat-core)`) that matches the verdict
(`_z3_path` finds z3, `_named_asserts` names assertions so z3's minimal core points
back). `_parse_predicate`/`_coerce_query_value` split a raw `var op value ∧ …`
string the same way the frontend does, for the /api/query path.
"""
import re
import subprocess


def _z3_path():
    for p in ("z3", "/usr/local/bin/z3", "/opt/homebrew/bin/z3"):
        try:
            if subprocess.run([p, "-version"], capture_output=True, timeout=5).returncode == 0:
                return p
        except Exception:
            continue
    return None


def _named_asserts(smt: str):
    """Wrap each one-line top-level `(assert X)` as `(assert (! X :named aK))` so an unsat-core
    names them back — z3's own minimal core then points at specific assertions."""
    out, k = [], 0
    for ln in smt.splitlines():
        s = ln.strip()
        if s.startswith("(assert ") and s.endswith(")"):
            k += 1
            out.append(f"(assert (! {s[len('(assert '):-1]} :named a{k}))")
        else:
            out.append(ln)
    return "\n".join(out)


def _ready_to_run(raw: str):
    """A one-paste z3 session. Pick the tail that MATCHES the verdict: `(get-model)` errors on an
    UNSAT script and `(get-unsat-core)` errors on a SAT one (Ana #203), so check first — and on
    UNSAT hand z3 its own minimal NAMED core (Ana #204). Falls back to a neutral, never-erroring
    tail + a hint when z3 isn't available."""
    if "(check-sat)" in raw:
        return raw
    z3, verdict = _z3_path(), None
    if z3:
        try:
            r = subprocess.run([z3, "-in"], input=raw + "\n(check-sat)\n",
                               capture_output=True, text=True, timeout=10)
            lines = (r.stdout or "").strip().splitlines()
            verdict = lines[0].strip() if lines else None
        except Exception:
            verdict = None
    if verdict == "unsat":
        return "(set-option :produce-unsat-cores true)\n" + _named_asserts(raw) + "\n(check-sat)\n(get-unsat-core)\n"
    if verdict == "sat":
        return raw + "\n(check-sat)\n(get-model)\n"
    return (raw + "\n(check-sat)\n"
            "; SAT → add (get-model)   UNSAT → add (set-option :produce-unsat-cores true) above + (get-unsat-core)\n")


# ad-hoc query: the same `var op value` shape the frontend's _INV_RE parses, so a raw
# predicate string ("light = Green ∧ timer = 2") can be split + parsed server-side too.
_QUERY_TERM_RE = re.compile(r"^\s*([A-Za-z_]\w*(?:\.\w+)?)\s*(<=|>=|!=|<|>|=|≤|≥|≠)\s*(.+?)\s*$")


def _coerce_query_value(s: str):
    """Coerce a raw term value the way the frontend's _coerce does: int, float, bool, else str
    (an enum variant name like 'Green')."""
    s = s.strip()
    if re.fullmatch(r"-?\d+", s):
        return int(s)
    if re.fullmatch(r"-?\d*\.\d+", s):
        return float(s)
    if s in ("true", "false"):
        return s == "true"
    return s


def _parse_predicate(pred: str):
    """Split a raw conjunction `t1 ∧ t2 ∧ …` (also accepts 'and' / '&&') into (var, op, value)
    triples — the server-side mirror of the frontend term split."""
    terms = []
    for part in re.split(r"\s*(?:∧|&&|\band\b)\s*", pred.strip()):
        if not part:
            continue
        m = _QUERY_TERM_RE.match(part)
        if not m:
            raise ValueError(f"bad query term {part!r}; write  var op value  (e.g. timer = 2)")
        terms.append([m.group(1), m.group(2), _coerce_query_value(m.group(3))])
    return terms
