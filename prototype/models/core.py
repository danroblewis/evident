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


class RecModel:
    """A GENUINELY recursive sub-model: its body references ITSELF (unlike a
    transition, which only relates state→state′). Backed by a Z3 recursive
    function, so Z3 owns the unfolding — no Python recursion, no hand-unrolling.

    body(self_fn, *params) -> term, where `self_fn(...)` is the recursive call."""
    def __init__(self, name, params, ret, body):
        self.name, self.params, self.ret = name, params, ret
        sig = [_SORT[t]() for _, t in params] + [_SORT[ret]()]
        self.fn = z3.RecFunction(name, *sig)
        syms = [_const(n, t) for n, t in params]
        self._body = body(self.fn, *syms)          # references self.fn → self-ref
        z3.RecAddDefinition(self.fn, syms, self._body)

    def __call__(self, *args):
        return self.fn(*args)

    def doc(self):
        return pretty.expr(self._body)              # shows the self-reference

    def solve(self, *args):
        r = z3.Const("r", _SORT[self.ret]())
        s = z3.Solver(); s.add(r == self.fn(*args))
        return _py(s.model(), r, self.ret) if s.check() == z3.sat else None


class BoundedRec:
    """The SAME recursive body as RecModel, but the RUNTIME owns the unfolding:
    an explicit work-list expands the self-reference to a depth bound N (no Python
    call stack, no Z3 lazy unfolding). This is 'do Z3's unfolding ourselves, but
    stop at N' — bounded ⇒ always decidable/fast. Beyond N a call is left
    unconstrained ('bottom'), so N must exceed the real recursion depth for an
    exact answer. body(rec, *params) -> term; rec(*args) is the recursive call."""
    def __init__(self, name, params, ret, body):
        self.name, self.params, self.ret, self.body = name, params, ret, body

    def unroll(self, args, depth):
        """Returns (result_term, constraints). The work-list IS the runtime's
        recursion — popping a pending call and expanding its body one level."""
        rs = _SORT[self.ret]()
        ctr = [0]

        def newr():
            ctr[0] += 1
            return z3.Const(f"{self.name}!{ctr[0]}", rs)

        top = newr()
        work, cons = [(top, list(args), depth)], []
        while work:                              # ← the runtime's unroller
            rvar, cargs, d = work.pop()
            if d <= 0:
                continue                         # bound reached: rvar unconstrained

            def rec(*na, _d=d):                  # a recursive call: defer it
                child = newr()
                work.append((child, list(na), _d - 1))
                return child

            cons.append(rvar == self.body(rec, *cargs))
        return top, cons

    def solve(self, arg_vals, depth):
        args = [_LIT[t](v) for (_, t), v in zip(self.params, arg_vals)]
        top, cons = self.unroll(args, depth)
        s = z3.Solver(); s.add(*cons)
        return _py(s.model(), top, self.ret) if s.check() == z3.sat else None


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
    """Step-by-step with a FRESH solver each tick. Constant *variable* footprint
    (same field slots), but it rebuilds the solver and re-asserts the transition
    every tick — so it throws away learned clauses. The crude version; prefer
    `run_persistent` for genuine constraint reuse."""
    vals, trace = dict(init), [dict(init)]
    for _ in range(max_steps):
        cur, nxt = tr.vars(""), tr.vars("′")
        s = z3.Solver()                              # fresh each tick (no reuse)
        s.add(*[cur[n] == _LIT[tr.tags[n]](vals[n]) for n in cur])
        s.add(tr.step(cur, nxt))
        s.check()
        m = s.model()
        vals = {n: _py(m, nxt[n], tr.tags[n]) for n in nxt}
        trace.append(dict(vals))
        if done and done(vals):
            break
    return vals, trace, 2 * len(tr.fields)


def run_persistent(tr, init, max_steps, done=None):
    """The genuine tail-recursion-with-reuse: ONE persistent solver via Z3's
    incremental push/pop. The transition is asserted ONCE and REUSED every tick;
    each tick pushes the current-state pins, solves, reads the next state, pops.
    The held-assertion count stays at 1 (just the transition) and learned clauses
    persist across ticks — constant memory, reused constraints, warm solver."""
    s = z3.Solver()
    cur, nxt = tr.vars(""), tr.vars("′")
    s.add(tr.step(cur, nxt))                         # asserted ONCE, reused forever
    vals, trace = dict(init), [dict(init)]
    for _ in range(max_steps):
        s.push()
        s.add(*[cur[n] == _LIT[tr.tags[n]](vals[n]) for n in cur])
        s.check()
        m = s.model()
        vals = {n: _py(m, nxt[n], tr.tags[n]) for n in nxt}
        s.pop()
        trace.append(dict(vals))
        if done and done(vals):
            break
    return vals, trace, len(s.assertions())          # == 1: the reused transition


# ── the prettified report ─────────────────────────────────────────────────────
def section_md(title, transition, submodels, init, fuel, done=None):
    """Build one example's markdown section (prettified Z3 AST). Returns
    (markdown_text, one_shot_final, incremental_final)."""
    states, cons = unroll_oneshot(transition, init, fuel)
    one_final, one_vars = run_oneshot(transition, init, fuel)
    inc_final, trace, inc_vars = run_incremental(transition, init, fuel, done)
    per_final, _, per_held = run_persistent(transition, init, fuel, done)

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
          f"[{one_vars} vars — grows with depth]",
          f"- **incremental** (fresh solver each tick): final = "
          f"`{inc_final}`  [{inc_vars} vars, but rebuilds + re-asserts each tick]",
          f"- **persistent** (ONE solver, push/pop, transition asserted once): "
          f"final = `{per_final}`  [**{per_held} held assertion — the transition, "
          "reused every tick**]", "",
          "state trace (persistent push/pop loop):", "",
          "```", "\n→ ".join(str(s) for s in trace), "```", ""]
    return "\n".join(L), one_final, inc_final


def rec_section_md(model, calls):
    """Markdown section for a recursive sub-model: its self-referential
    definition (prettified) + sample solves. `calls` = [(args_tuple, expected)]."""
    params = ", ".join(n for n, _ in model.params)
    L = [f"## {model.name}  (recursive — references itself)", "",
         f"A genuinely recursive sub-model: `{model.name}` appears **inside its "
         "own definition** (Z3 owns the unfolding; no transition, no "
         "hand-unrolling).", "",
         "### definition", "", "```", f"{model.name}({params}) =", model.doc(),
         "```", "", "### solves", ""]
    for args, expected in calls:
        got = model.solve(*[z3.IntVal(a) for a in args])
        L.append(f"- `{model.name}{args}` = `{got}`  (expect {expected})")
    L.append("")
    return "\n".join(L)


def bounded_section_md(model, arg_vals, depth):
    """Markdown for the runtime-owned bounded unroll: the explicit constraints
    the work-list emits (prettified) + the solve."""
    args = [_LIT[t](v) for (_, t), v in zip(model.params, arg_vals)]
    top, cons = model.unroll(args, depth)
    got = model.solve(arg_vals, depth)
    L = [f"## {model.name}  (recursive — runtime-owned bounded unroll, N={depth})",
         "",
         "**Same definition**, but the runtime owns the unfolding: an explicit "
         f"work-list expands the self-reference to depth N={depth} (no Python "
         "stack, no Z3 lazy unfolding). Bounded ⇒ always decidable.", "",
         "### what the runtime emits (the unrolled constraints)", "",
         f"`result = {top.decl().name()}`, then:", "",
         "```", pretty.goal(cons), "```", "",
         "### solve", "",
         f"- `{model.name}{tuple(arg_vals)}` (N={depth}) = `{got}`", ""]
    return "\n".join(L)


def write_report(path, title, sections):
    """Write one markdown file (the prettified Z3-AST view) from N sections."""
    head = [f"# {title}", "",
            "Prettified Z3-AST view of each sub-model and the combined unrolled "
            "model. Generated by `python3 -m models.examples`.", "", "---", ""]
    body = []
    for s in sections:
        body += [s, "---", ""]
    open(path, "w").write("\n".join(head + body))
