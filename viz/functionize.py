#!/usr/bin/env python3
"""functionize.py — surface the functionizer's per-variable decomposition, for visualization.

The Rust runtime's functionizer (runtime/src/functionize/extract_program.rs) splits the solved,
simplified transition relation into per-output-variable FUNCTIONS — Scalar (a pure expr of other
vars), Guarded (a piecewise `guard ⇒ body`), Seq (element-wise) — plus the residual CONSTRAINTS
that did NOT reduce to a function (the genuinely-relational part). That structure is exactly what's
interesting to SEE: the compiled data-flow DAG, the functionized-vs-residual boundary, and the guard
decision-trees.

This module re-derives the same decomposition in Python from the model's already-loaded z3 assertions
(the viz layer has them in `model.assertions`), so the renderers can draw it with no runtime export
round-trip. It mirrors extract_program's logic: peel `Implies(guard, var = body)` → Guarded, `var =
expr` → Scalar, everything else → residual. (If this proves out, the authoritative path is a
`evident functions` JSON dump from the Rust functionizer itself.)
"""
import z3


def _free_vars(e):
    """Ordered, de-duplicated uninterpreted-constant (variable) names referenced in a z3 expr."""
    seen, out = set(), []

    def walk(x):
        xid = x.get_id()
        if xid in seen:
            return
        seen.add(xid)
        if z3.is_const(x) and x.decl().kind() == z3.Z3_OP_UNINTERPRETED:
            out.append(x.decl().name())
        for c in x.children():
            walk(c)
    walk(e)
    return out


def _bare_var(e):
    """The name of `e` if it is a bare uninterpreted constant (a variable leaf), else None."""
    if z3.is_const(e) and e.decl().kind() == z3.Z3_OP_UNINTERPRETED:
        return e.decl().name()
    return None


def _solve_for_output(lhs, rhs, output_set):
    """Express an output var as a function `var = body` from `lhs == rhs`. Handles a bare var on
    either side (var = the other side) AND the Δ-FORM `var - rest == rhs` → `var = rhs + rest` — the
    difference-equation shape the FSM lowering emits for carried state (Δx). Returns (var, body) | None."""
    for x, y in ((lhs, rhs), (rhs, lhs)):
        v = _bare_var(x)
        if v and v in output_set and v not in _free_vars(y):
            return v, y
    for x, y in ((lhs, rhs), (rhs, lhs)):          # var - rest == y  ⇒  var = y + rest
        if z3.is_app(x) and x.decl().kind() == z3.Z3_OP_SUB and x.num_args() == 2:
            v, rest = _bare_var(x.arg(0)), x.arg(1)
            if v and v in output_set and v not in _free_vars(rest) and v not in _free_vars(y):
                return v, y + rest
    return None


def _consequent_assignments(cons, output_set):
    """A guarded branch's consequent assigns output vars: `var = body`, possibly several inside an
    And (`light = Green ∧ timer = 0`). Return [(var, body), …] for every such equality (a branch can
    set more than one variable — each gets its own guarded entry)."""
    out, todo = [], [cons]
    while todo:
        e = todo.pop()
        if z3.is_and(e):
            todo.extend(reversed(e.children()))
            continue
        if z3.is_eq(e):
            asg = _solve_for_output(e.arg(0), e.arg(1), output_set)
            if asg is not None:
                out.append(asg)
    return out


def extract_functions(model):
    """Decompose `model.assertions` (the transition relation) into per-output-variable functions +
    residual constraints. Returns a JSON-able dict:

      { "outputs": [...], "inputs": [...],
        "steps":   [ {var, kind: scalar|guarded, expr|branches, deps:[...]} ... ],
        "residual":[ {expr, deps:[...]} ... ] }

    `inputs` are the INDEPENDENT variables (referenced but not themselves an output of a step) — the
    drivers; `steps`' vars are the DEPENDENT variables (computed). That split is the principled axis
    signal: dependent → Y, independent → X.

    Outputs include both CARRIED state (a function of the prev tick) and DERIVED vars (a same-tick
    function of the current state, e.g. `done = count ≥ 5`). Both are genuine functions the runtime
    functionizer compiles — a derived var is the *purest* function, so it belongs in `steps`, not
    miscounted as a residual constraint."""
    derived_set = {v["name"] for v in getattr(model, "derived", [])}
    outputs = [v["name"] for v in model.carried] + sorted(derived_set)
    output_set = set(outputs)
    steps = {}                  # var -> step dict (insertion-ordered)
    residual = []

    for a in model.assertions:
        # Guarded:  Implies(guard, <assignment(s)>)
        if z3.is_app(a) and a.decl().kind() == z3.Z3_OP_IMPLIES and a.num_args() == 2:
            guard, cons = a.arg(0), a.arg(1)
            asgs = _consequent_assignments(cons, output_set)
            if asgs:
                gdeps = _free_vars(guard)
                gatoms = [_pretty(c) for c in guard.children()] if z3.is_and(guard) else [_pretty(guard)]
                for var, body in asgs:
                    st = steps.get(var)
                    if st is None or st["kind"] != "guarded":
                        st = steps[var] = {"var": var, "kind": "guarded", "branches": []}
                    st["branches"].append({
                        "guard": _pretty(guard), "guard_atoms": gatoms, "body": _pretty(body),
                        "_guard_z3": guard,                # in-process only (for guard_analysis); not JSON
                        "deps": gdeps + [d for d in _free_vars(body) if d not in gdeps],
                    })
                continue
        # Scalar:  var == expr (or the Δ-form var - rest == expr), var an output not yet bound.
        if z3.is_eq(a):
            asg = _solve_for_output(a.arg(0), a.arg(1), output_set)
            if asg is not None and asg[0] not in steps:
                v, body = asg
                steps[v] = {"var": v, "kind": "scalar", "expr": _pretty(body), "deps": _free_vars(body)}
                continue
        # Residual: a genuine constraint the solver did NOT reduce to a function.
        residual.append({"expr": _pretty(a), "_z3": a, "deps": _free_vars(a)})

    step_list = list(steps.values())
    for st in step_list:                               # tag derived (same-tick) functions
        if st["var"] in derived_set:
            st["derived"] = True
    referenced = {d for st in step_list for d in _step_deps(st)}
    referenced |= {d for r in residual for d in r["deps"]}
    # INDEPENDENT = referenced but NOT an output (the prev-tick reads _x + is_first_tick — the true
    # drivers). An output that isn't a step is "constrained, not functionized" — not a driver.
    inputs = sorted(v for v in referenced if v not in output_set)
    unfunctionized = [v for v in outputs if v not in steps]
    return {"outputs": outputs, "inputs": inputs, "unfunctionized": unfunctionized,
            "steps": step_list, "residual": residual}


def _step_deps(st):
    if st["kind"] == "guarded":
        return [d for b in st.get("branches", []) for d in b["deps"]]
    return st.get("deps", [])              # scalar | seq carry deps at the top level


def _pretty(e):
    """A compact one-line z3-expr string (z3's default repr already drops newlines for small exprs)."""
    return " ".join(str(e).split())


def function_summary(model):
    """High-level metrics over the functionizer decomposition: how much of the program reduced to
    COMPUTATION (functionized vars) vs stayed a CONSTRAINT (residual), and the COUPLING shape read off
    the data-flow DAG — a feedback cycle (pos↔vel) ⇒ coupled dynamics, acyclic ⇒ a driven pipeline.
    A reusable structural classifier (distinct from the independence-sampling the model-shape banner
    uses). Returns {functionized, residual, pct, coupling, cycles}."""
    import networkx as nx
    f = extract_functions(model)
    # HONEST denominator (Ana #305): carried vars that have an update law / total carried vars. A
    # residual type-bound invariant (0≤floor≤3) is a STANDING CONSTRAINT, not un-computed work — so it
    # must NOT count against "% computed". Elevator with both vars functionized is 100%, not 50%.
    step_vars = {s["var"] for s in f["steps"]}
    carried = [v["name"] for v in model.carried]
    n_carried = len(carried)
    n_func = sum(1 for v in carried if v in step_vars)
    prev_to_var = {v["prev"]: v["name"] for v in model.carried if v.get("prev")}
    g = nx.DiGraph()
    for s in f["steps"]:
        deps = {d for b in s.get("branches", []) for d in b["deps"]} | set(s.get("deps", []))
        for d in deps:
            src = prev_to_var.get(d)
            if src and src != s["var"]:                # cross-var coupling (ignore self-recurrence)
                g.add_edge(src, s["var"])
    cycles = [c for c in nx.simple_cycles(g) if len(c) >= 2]
    # Three classes (Ana #307): a cross-var feedback cycle ⇒ coupled; an acyclic cross-edge cascade ⇒
    # driven (a real driver feeds it); NO cross-edges (only self-recurrences x'=f(_x)) ⇒ autonomous —
    # a closed self-map with no driver, NOT a "driven pipeline".
    if cycles:
        coupling = "coupled"
    elif g.number_of_edges() > 0:
        coupling = "driven"
    else:
        coupling = "autonomous"
    return {"functionized": len(f["steps"]), "residual": len(f["residual"]),
            "n_carried": n_carried, "n_func_carried": n_func,
            "pct": (n_func / n_carried * 100) if n_carried else 0,
            "coupling": coupling, "cycles": cycles}


def _step_sig(step):
    """A canonical comparable string of a step's function — scalar expr, or its guarded branches sorted
    (order-independent) so a reordering isn't reported as a change."""
    if step["kind"] == "scalar":
        return step["expr"]
    if step["kind"] == "guarded":
        return "  |  ".join(sorted(f"{b['guard']} ⇒ {b['body']}" for b in step["branches"]))
    return repr(step.get("expr") or step.get("value"))


def function_diff(ma, mb):
    """The compiled-structure DELTA between two programs (Ana #318): which per-variable functions
    APPEARED, VANISHED, or CHANGED when the source was edited — the functionizer's view of a diff,
    deeper than a text diff or a reachable-state diff. Aligns by variable name; also reports the
    coupling-class and %-computed shift."""
    fa, fb = extract_functions(ma), extract_functions(mb)
    sa = {s["var"]: s for s in fa["steps"]}
    sb = {s["var"]: s for s in fb["steps"]}
    rows = []
    for v in sorted(set(sa) | set(sb)):
        a, b = sa.get(v), sb.get(v)
        if a and not b:
            status = "vanished"
        elif b and not a:
            status = "appeared"
        elif _step_sig(a) != _step_sig(b):
            status = "changed"
        else:
            status = "unchanged"
        rows.append({"var": v, "status": status,
                     "before": _step_sig(a) if a else None, "after": _step_sig(b) if b else None})
    ca, cb = function_summary(ma), function_summary(mb)
    return {"vars": rows, "changed": [r for r in rows if r["status"] != "unchanged"],
            "coupling_before": ca["coupling"], "coupling_after": cb["coupling"],
            "pct_before": round(ca["pct"]), "pct_after": round(cb["pct"])}


def guard_analysis(model, steps, residual):
    """For each guarded step, check whether the piecewise DISPATCH is a TOTAL function — the guards
    cover the whole valid INPUT space — and UNAMBIGUOUS — no two guards are simultaneously satisfiable.
    Both z3-decidable. Returns {var: {complete, disjoint, overlap:(i,j)|None}}. A gap ⇒ a partial
    function (undefined behaviour for some valid input); an overlap ⇒ ambiguous dispatch.

    The input domain is the type-invariants on the PREV variables the guards read — obtained by
    substituting each next-var → its prev-var in the residual (0≤timer≤2 ⇒ 0≤_timer≤2); without that
    the prev reads are unconstrained and z3 reports a spurious gap at _timer=5."""
    rez_next = [r["_z3"] for r in residual if "_z3" in r]
    consts = getattr(model, "consts", {})
    subs = [(consts[v["name"]], consts[v["prev"]]) for v in model.carried
            if v.get("prev") and v["name"] in consts and v["prev"] in consts]
    domain = [z3.substitute(r, subs) for r in rez_next] if subs else rez_next
    out = {}
    for s in steps:
        if s["kind"] != "guarded":
            continue
        guards = [b["_guard_z3"] for b in s["branches"] if "_guard_z3" in b]
        if not guards:
            continue
        overlap = overlap_witness = None
        for i in range(len(guards)):
            for j in range(i + 1, len(guards)):
                sv = z3.Solver(); sv.add(domain); sv.add(guards[i]); sv.add(guards[j])
                if sv.check() == z3.sat:
                    overlap, overlap_witness = (i, j), _witness(sv.model()); break
            if overlap:
                break
        sv = z3.Solver(); sv.add(domain); sv.add(z3.Not(z3.Or(guards)))
        res = sv.check()
        complete = res == z3.unsat
        # The Z3 WITNESS is already in hand (Ana #303): the input that hits no branch (gap) / where two
        # branches both fire (overlap). A failed property must yield its counterexample — surface it.
        gap_witness = _witness(sv.model()) if res == z3.sat else None
        out[s["var"]] = {"disjoint": overlap is None, "complete": complete, "overlap": overlap,
                         "gap_witness": gap_witness, "overlap_witness": overlap_witness}
    return out


def _witness(mdl):
    """A compact input assignment from a z3 model — the prev-var values (+ is_first_tick) that witness
    a gap or overlap. The decls are the guard inputs, so this is exactly the counterexample input."""
    parts = [f"{d.name()} = {mdl[d]}" for d in mdl.decls()]
    return ", ".join(sorted(parts)) or "(any input)"


if __name__ == "__main__":
    import sys
    sys.path.insert(0, "viz")
    from evident_viz import load
    if len(sys.argv) != 3:
        print("usage: functionize.py <smt2> <schema>", file=sys.stderr)
        sys.exit(2)
    m = load(sys.argv[1], sys.argv[2])
    f = extract_functions(m)
    print(f"OUTPUTS (dependent): {[s['var'] for s in f['steps']]}")
    print(f"INPUTS  (independent/drivers): {f['inputs']}")
    print("STEPS:")
    for s in f["steps"]:
        if s["kind"] == "scalar":
            print(f"  {s['var']}  =  {s['expr']}          [deps: {', '.join(s['deps'])}]")
        else:
            print(f"  {s['var']}  (guarded, {len(s['branches'])} branches):")
            for b in s["branches"]:
                print(f"      {b['guard']}  ⇒  {s['var']} = {b['body']}")
    if f["residual"]:
        print(f"RESIDUAL ({len(f['residual'])} un-functionized constraints):")
        for r in f["residual"][:8]:
            print(f"  {r['expr']}")
