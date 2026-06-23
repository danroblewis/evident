"""Quantitative / optimization queries over a CLAIM — the z3 Optimize move.

The solve surface answers FEASIBILITY (SAT witness / UNSAT). `_optimize` answers the
QUANTITATIVE question a verification engineer reaches for daily: maximize or minimize a
numeric variable subject to the claim, returning the EXTREMAL value AND the optimizing
assignment. It reuses the claim-loading machinery `render_claim_space._load_claim` already
builds for the solution-space bounds (the `body` z3 And + the name→const map), then runs
`z3.Optimize()` instead of a plain solve.

Scoped to numeric extremals (int/real). A Seq/enum extremal would need a richer model
decode than the numeric-var one; that's a deliberate limit, not built here.
"""
from runtime_io import _export


def _resolve_const(consts, var):
    """The z3 const for `var`: exact full-name match, else short-name (`a.b.c` → `c`) match."""
    if var in consts:
        return consts[var]
    return next((cc for nm, cc in consts.items() if nm.split(".")[-1] == var), None)


def _decode_numeric(model, consts, sch):
    """Decode the claim's NUMERIC vars (kind int/real) from a solved model → {short_name: value}."""
    out = {}
    for v in sch.get("vars", []):
        if v.get("kind") not in ("int", "real") or v["name"] not in consts:
            continue
        z = model.eval(consts[v["name"]], model_completion=True)
        try:
            out[v["name"].split(".")[-1]] = z.as_long() if v["kind"] == "int" else float(z.as_fraction())
        except Exception:
            pass
    return out


def _optimize(source, claim, var, direction, work):
    """Maximize/minimize a numeric var subject to the claim. Returns the extremal value AND the
    optimizing assignment (numeric vars). On unsat/unbounded: satisfied=False (an honest "no finite
    extremum"). Scoped to numeric extremals — a Seq/enum extremal would need a richer decode."""
    import z3
    import render_claim_space as RC
    ok, prefix, dropped, msg = _export(source, work)
    if not ok:
        return {"ok": False, "error": msg}
    try:
        sch, body, consts = RC._load_claim(prefix + ".smt2", prefix + ".schema.json")
    except Exception as e:
        return {"ok": False, "error": f"could not load model: {e}"}
    c = _resolve_const(consts, var)
    if c is None:
        return {"ok": False, "error": f"no variable {var!r} in this claim"}
    maximize = direction != "min"                      # default to max for any non-"min"
    direction = "max" if maximize else "min"
    base = {"ok": True, "claim": sch.get("claim", claim), "var": var, "direction": direction}
    o = z3.Optimize()
    o.add(body)
    h = o.maximize(c) if maximize else o.minimize(c)
    if o.check() != z3.sat:                             # unsatisfiable constraint
        return {**base, "satisfied": False}
    # The objective BOUND (not model().eval) is the extremum — and z3 reports `oo`/`-oo` here for an
    # UNBOUNDED objective even though the model has a finite witness. RC._num returns None for those.
    extremal = RC._num(o.upper(h) if maximize else o.lower(h))
    if extremal is None:                               # unbounded → no finite extremum to report
        return {**base, "satisfied": False}
    return {**base, "satisfied": True, "extremal": extremal,
            "bindings": _decode_numeric(o.model(), consts, sch)}
