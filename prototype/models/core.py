"""The composition + unrolling primitive, and the prettified-report generator.

  • fresh()/reset_fresh — mint scoped internal consts (so a sub-model used twice
    doesn't alias its internals).
  • Model      — a named predicate/function template over an interface.
  • Transition — a one-step state-transition sub-model over named fields.
  • unroll_oneshot — build s0..s_fuel in ONE solve (no memory reuse).
  • run_incremental — solve one step at a time, reusing the SAME field slots
    (memory reuse; the tail-recursion runtime).
  • report_md — write a markdown file: each sub-model prettified on its own, the
    transition, and the combined unrolled model the runtime actually solves.

The recursion is OWNED here (the explicit loops), not borrowed from Python's
call stack — so it transfers to a real runtime unchanged.
"""
import z3
from benchsuite import pretty

_SORT = {"Int": z3.IntSort, "Bool": z3.BoolSort, "Real": z3.RealSort,
         "String": z3.StringSort}
_LIT = {"Int": z3.IntVal, "Bool": z3.BoolVal, "Real": z3.RealVal,
        "String": z3.StringVal}

_uid = [0]


def fresh(base, sort="Int"):
    _uid[0] += 1
    s = _SORT[sort]() if isinstance(sort, str) else sort
    return z3.Const(f"{base}!{_uid[0]}", s)


def reset_fresh():
    _uid[0] = 0


def _const(name, tag):
    return z3.Const(name, _SORT[tag]())


def _py(m, c, tag):
    e = m.eval(c, model_completion=True)
    if tag == "Int":
        return e.as_long()
    if tag == "Bool":
        return z3.is_true(e)
    if tag == "String":
        return e.as_string()
    return str(e)


class Model:
    """A named sub-model: `body(*interface) -> BoolRef (predicate) or term`.
    params is [(name, sort_tag)]. Internals are minted with fresh()."""
    def __init__(self, name, params, body):
        self.name, self.params, self.body = name, params, body

    def __call__(self, *args):
        return self.body(*args)

    def doc(self):
        reset_fresh()
        return pretty.expr(self.body(*[_const(n, t) for n, t in self.params]))


class Transition:
    """A one-step transition sub-model over `fields` = [(name, sort_tag)].
    step(cur, nxt) -> BoolRef, where cur/nxt are dicts field_name -> Z3 const.
    `uses` names the helper sub-models the step composes (for the report)."""
    def __init__(self, name, fields, step, uses=()):
        self.name, self.fields, self.step, self.uses = name, fields, step, tuple(uses)
        self.tags = dict(fields)

    def vars(self, suffix):
        return {n: _const(f"{n}{suffix}", t) for n, t in self.fields}

    def doc(self):
        reset_fresh()
        cur = {n: _const(n, t) for n, t in self.fields}
        nxt = {n: _const(n + "′", t) for n, t in self.fields}
        return pretty.expr(self.step(cur, nxt))


# ── execution strategies ──────────────────────────────────────────────────────
def unroll_oneshot(tr, init, fuel):
    """All states s0..s_fuel exist in ONE solve. Variable count grows with fuel."""
    states = [tr.vars(str(t)) for t in range(fuel + 1)]
    cons = [states[0][n] == _LIT[tr.tags[n]](init[n]) for n in init]
    for t in range(fuel):
        cons.append(tr.step(states[t], states[t + 1]))
    return states, cons


def run_oneshot(tr, init, fuel):
    states, cons = unroll_oneshot(tr, init, fuel)
    s = z3.Solver(); s.add(*cons)
    if s.check() != z3.sat:
        return None, len({v for st in states for v in st.values()})
    m = s.model()
    final = {n: _py(m, states[-1][n], tr.tags[n]) for n in tr.tags}
    nvars = sum(len(st) for st in states)
    return final, nvars


def run_incremental(tr, init, max_steps, done=None):
    """Solve ONE step at a time, reusing the SAME field slots each step (memory
    reuse). Variable footprint is constant (2 × #fields) regardless of steps."""
    vals, trace = dict(init), [dict(init)]
    for _ in range(max_steps):
        cur, nxt = tr.vars(""), tr.vars("′")
        s = z3.Solver()
        s.add(*[cur[n] == _LIT[tr.tags[n]](vals[n]) for n in cur])
        s.add(tr.step(cur, nxt))
        s.check()
        m = s.model()
        vals = {n: _py(m, nxt[n], tr.tags[n]) for n in nxt}
        trace.append(dict(vals))
        if done and done(vals):
            break
    return vals, trace, 2 * len(tr.fields)


# ── the prettified report ─────────────────────────────────────────────────────
def section_md(title, transition, submodels, init, fuel, done=None):
    """Build one example's markdown section (prettified Z3 AST). Returns
    (markdown_text, one_shot_final, incremental_final)."""
    states, cons = unroll_oneshot(transition, init, fuel)
    one_final, one_vars = run_oneshot(transition, init, fuel)
    inc_final, trace, inc_vars = run_incremental(transition, init, fuel, done)

    L = [f"## {title}", "",
         "Each sub-model on its own (symbolic interface), then the **combined** "
         "model the runtime solves. Recursion is owned by the unroller, not "
         "Python's stack.", ""]
    for m in submodels:
        params = ", ".join(f"`{n}: {t}`" for n, t in m.params)
        L += [f"### sub-model `{m.name}`  ({params})", "", "```", m.doc(), "```", ""]
    fields = ", ".join(f"`{n}: {t}`" for n, t in transition.fields)
    uses = (" — composes " + ", ".join(f"`{u}`" for u in transition.uses)
            if transition.uses else "")
    L += [f"### transition `{transition.name}`  (one step: state → state′){uses}", "",
          f"fields: {fields}", "", "```", transition.doc(), "```", "",
          f"### combined model — `{transition.name}` unrolled to {fuel} steps", "",
          f"What the **one-shot** strategy solves ({one_vars} variables — grows "
          "with depth).", "", "```", pretty.goal(cons), "```", "",
          "### run result", "",
          f"- **one-shot** (unroll all, one solve): final = `{one_final}`  "
          f"[{one_vars} vars]",
          f"- **incremental** (one step at a time, memory reuse): final = "
          f"`{inc_final}`  [{inc_vars} vars, constant]", "",
          "state trace (incremental):", "",
          "```", "\n→ ".join(str(s) for s in trace), "```", ""]
    return "\n".join(L), one_final, inc_final


def write_report(path, title, sections):
    """Write one markdown file (the prettified Z3-AST view) from N sections."""
    head = [f"# {title}", "",
            "Prettified Z3-AST view of each sub-model and the combined unrolled "
            "model. Generated by `python3 -m models.examples`.", "", "---", ""]
    body = []
    for s in sections:
        body += [s, "---", ""]
    open(path, "w").write("\n".join(head + body))
