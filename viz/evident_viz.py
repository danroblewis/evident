"""evident_viz — shared loader + transition-query layer for Evident visualizations.

The Evident runtime exports an FSM's transition relation as a self-contained
SMT-LIB file (`<name>.smt2`) plus a JSON state schema (`<name>.schema.json`),
via `evident export <prog.ev>`. This module loads that pair and exposes the
queries every visualizer needs — all backed by z3, so the dynamics come from
*solving the transition*, never from hardcoded logic.

A `Model` is the difference equation `state = f(_state)` (possibly relational /
set-valued). Renderers should depend ONLY on this module, so they work for ANY
Evident program: load(smt2, schema) -> Model, then query.

    from evident_viz import load
    m = load("dungeon.smt2", "dungeon.schema.json")
    m.state_vars          # [{'name','prev','kind'}, ...]
    m.initial_state()     # {var: value} on the first tick (or None)
    m.successor(s)        # one next state (None if unsat)
    m.successors(s)       # ALL next states (set-valued transitions / "fans")
    m.trajectory(steps=N) # follow one successor chain from the initial state
    m.reachable()         # BFS of all reachable distinct states (discrete)

Values: int -> python int, bool -> bool, enum -> variant name (str),
real -> float, string -> str.
"""
import json
import os
import z3


# Visual-channel effectiveness by variable class (Cleveland & McGill 1984 /
# Mackinlay 1986): POSITION decodes best for everything; SIZE is good for
# quantitative but poor for categorical; COLOR (hue) and FACET are excellent for
# categorical but weak for quantitative. importance(var) x this table decides which
# variable lands on which channel. Color/size/facet are SECONDARY — a good plot
# reads from its axes alone.
CHANNEL_FITNESS = {
    "x":       {"quant": 1.00, "cat": 0.90},
    "y":       {"quant": 1.00, "cat": 0.90},
    "size":    {"quant": 0.70, "cat": 0.25},
    "opacity": {"quant": 0.60, "cat": 0.25},
    "color":   {"quant": 0.40, "cat": 0.85},
    "facet":   {"quant": 0.20, "cat": 0.80},
    "shape":   {"quant": 0.10, "cat": 0.60},
}


def load(smt2_path, schema_path):
    return Model(smt2_path, schema_path)


class Model:
    def __init__(self, smt2_path, schema_path):
        with open(schema_path) as fh:
            schema = json.load(fh)
        self.fsm = schema["fsm"]
        # All carried leaves drive the transition; the INTERFACE subset (the fsm's
        # first-line params — the model's observable contract) is the default axis
        # set. Renderers see `state_vars` = interface; the query layer pins/reads
        # the full `carried` set. (See docs/design/portrait-axes.md.)
        self.carried = schema["state"]                     # [{name, prev, kind, role}]
        self.interface_vars = [v for v in self.carried
                               if v.get("role", "interface") == "interface"]
        if not self.interface_vars and self.carried:
            # a bare-body-item fsm (`fsm counter` with `count`/`done` as body items, not
            # a first-line state param) has no role=interface leaf — its carried state IS
            # the observable interface. Without this, the selector / independence / banner
            # see nothing. Only fires when the interface would otherwise be empty.
            self.interface_vars = list(self.carried)
        self.internal_vars = [v for v in self.carried if v.get("role") == "internal"]
        self._ranked = None          # cached ranked+deduped interface vars (lazy)
        self.variable_groups = []    # [{rep, members, entropy}] redundancy groups
        self._change_rates_cache = None
        self._first_tick_name = schema["is_first_tick"]

        # Parse the self-contained SMT-LIB (datatype decls + transition asserts).
        self.assertions = z3.parse_smt2_file(smt2_path)

        # Collect every declared (uninterpreted) constant by name: d.room, _d.room,
        # is_first_tick, state.x, ... — NOT enum value constructors.
        self.consts = {}
        seen = set()

        def walk(e):
            eid = e.get_id()
            if eid in seen:
                return
            seen.add(eid)
            if z3.is_const(e) and e.decl().kind() == z3.Z3_OP_UNINTERPRETED:
                self.consts[e.decl().name()] = e
            for ch in e.children():
                walk(ch)

        for a in self.assertions:
            walk(a)

        # Some carried leaves are DECLARED but unused in the transition (e.g. a
        # bool whose next value ignores its previous value), so they never appear
        # in an assertion and z3's parser drops them. Synthesize by name, using a
        # sibling's sort, so the pin/read API stays uniform.
        for v in self.carried:
            present = self.consts.get(v["name"])
            if present is None:
                present = self.consts.get(v["prev"])
            sort = present.sort() if present is not None else self._basic_sort(v["kind"])
            for nm in (v["name"], v["prev"]):
                if nm not in self.consts:
                    self.consts[nm] = z3.Const(nm, sort)

        self.first_tick = self.consts.get(self._first_tick_name)

        # For each enum state var, map variant-name -> z3 value (nullary ctor).
        self.enum_variants = {}            # state-var name -> [variant names]
        self._enum_lit = {}                # state-var name -> {variant: z3 value}
        for v in self.carried:
            if v["kind"] == "enum":
                sort = self.consts[v["name"]].sort()
                names, lits = [], {}
                for i in range(sort.num_constructors()):
                    ctor = sort.constructor(i)
                    names.append(ctor.name())
                    lits[ctor.name()] = ctor()
                self.enum_variants[v["name"]] = names
                self._enum_lit[v["name"]] = lits

    # ---- value <-> z3 -------------------------------------------------------
    def _lit(self, var, value):
        k = var["kind"]
        if k == "int":
            return z3.IntVal(int(value))
        if k == "bool":
            return z3.BoolVal(bool(value))
        if k == "real":
            return z3.RealVal(value)
        if k == "string":
            return z3.StringVal(value)
        if k == "enum":
            return self._enum_lit[var["name"]][value]
        raise ValueError(f"unknown kind {k}")

    def _read(self, model, var):
        c = self.consts[var["name"]]
        mv = model.eval(c, model_completion=True)
        k = var["kind"]
        if k == "int":
            return mv.as_long()
        if k == "bool":
            return z3.is_true(mv)
        if k == "real":
            return float(mv.as_fraction())
        if k == "string":
            return mv.as_string()
        if k == "enum":
            return mv.decl().name()
        raise ValueError(f"unknown kind {k}")

    def _base(self):
        s = z3.Solver()
        s.add(self.assertions)
        return s

    def _read_state(self, model):
        return {v["name"]: self._read(model, v) for v in self.carried}

    def _pin_prev(self, solver, state):
        # Pin only the leaves the caller supplied; a renderer may pass a PARTIAL
        # state (e.g. just the deduped axis vars), leaving the rest free. Pinning
        # all of self.carried would KeyError on a leaf the caller omitted.
        for v in self.carried:
            if v["name"] in state:
                solver.add(self.consts[v["prev"]] == self._lit(v, state[v["name"]]))

    # ---- queries ------------------------------------------------------------
    def initial_state(self):
        """The state on the first tick (is_first_tick = true), or None."""
        s = self._base()
        if self.first_tick is not None:
            s.add(self.first_tick == True)  # noqa: E712
        return self._read_state(s.model()) if s.check() == z3.sat else None

    def successor(self, state):
        """One next state from `state` (None if the transition is unsat here)."""
        s = self._base()
        if self.first_tick is not None:
            s.add(self.first_tick == False)  # noqa: E712
        self._pin_prev(s, state)
        return self._read_state(s.model()) if s.check() == z3.sat else None

    def successors(self, state, limit=64):
        """ALL distinct next states (the set-valued image / fan). Blocks each
        found assignment and re-solves until unsat or `limit`."""
        s = self._base()
        if self.first_tick is not None:
            s.add(self.first_tick == False)  # noqa: E712
        self._pin_prev(s, state)
        out = []
        while len(out) < limit and s.check() == z3.sat:
            mod = s.model()
            st = self._read_state(mod)
            out.append(st)
            # Block against the model's EXACT value of each const, not a re-literal of
            # the decoded Python value. For reals, _read collapses an exact rational
            # (175/3) to a lossy float (58.333…); re-blocking with RealVal(float) never
            # excludes the true 175/3, so a DETERMINISTIC real FSM reports the one true
            # successor over and over as 'distinct' (the 64-fan mislabel). model.eval is
            # exact for every kind, so a single next state blocks to UNSAT as it should.
            s.add(z3.Or([self.consts[v["name"]] != mod.eval(self.consts[v["name"]],
                                                            model_completion=True)
                         for v in self.carried]))
        return out

    def trajectory(self, start=None, steps=400):
        """Follow ONE successor chain (deterministic-ish path) from `start`
        (default: the initial state). Stops at a fixed point, a revisit, or
        `steps`."""
        cur = start if start is not None else self.initial_state()
        if cur is None:
            return []
        path = [cur]
        seen = {self._key(cur)}
        for _ in range(steps):
            nxt = self.successor(cur)
            if nxt is None:
                break
            path.append(nxt)
            k = self._key(nxt)
            if k in seen:
                break
            seen.add(k)
            cur = nxt
        return path

    def reachable(self, limit=5000):
        """All reachable distinct states from the initial state, with the edge
        relation. Returns (states, edges) where states is a list of dicts and
        edges is a list of (from_index, to_index). For discrete programs this is
        the exact reachable state graph; for numeric ones it may not terminate,
        so it's capped by `limit`."""
        init = self.initial_state()
        if init is None:
            return [], []
        states = [init]
        index = {self._key(init): 0}
        edges = []
        frontier = [0]
        while frontier and len(states) < limit:
            i = frontier.pop()
            for nxt in self.successors(states[i]):
                k = self._key(nxt)
                if k not in index:
                    index[k] = len(states)
                    states.append(nxt)
                    frontier.append(index[k])
                edges.append((i, index[k]))
        return states, edges

    # ---- helpers ------------------------------------------------------------
    @staticmethod
    def _key(state):
        return tuple(sorted(state.items()))

    @staticmethod
    def _basic_sort(kind):
        return {"int": z3.IntSort(), "bool": z3.BoolSort(),
                "real": z3.RealSort(), "string": z3.StringSort()}.get(kind, z3.IntSort())

    def is_discrete(self):
        return all(v["kind"] in ("bool", "enum", "string") for v in self.interface_vars)

    def label(self, state):
        # Robust to a state that doesn't carry every interface var (some renderers' BFS
        # track only a subset) — show "?" rather than KeyError-ing the whole render.
        return "(" + ", ".join(str(state.get(v["name"], "?")) for v in self.interface_vars) + ")"

    # ---- variable ranking: dedup redundant ('same-graph') vars, rank the rest ----
    @property
    def state_vars(self):
        """Interface variables, deduplicated (informationally-equivalent vars merged)
        and ranked by how much they vary — the recommended axis ORDER. Renderers take
        as many as they need (top 2 for a phase portrait, all for a scatter matrix).
        Falls back to raw interface order if sampling is degenerate. Cached.

        Override (for the selector-evaluation sweep): if the env var
        EVIDENT_VIZ_VARS is set to a comma-separated list of leaf names, return
        exactly those (in that order) instead of the ranked selection — this lets a
        driver force any variable combination onto a renderer to compare against the
        selector's own pick."""
        ov = os.environ.get("EVIDENT_VIZ_VARS")
        if ov:
            by_name = {v["name"]: v for v in self.carried}
            chosen = [by_name[n] for n in ov.split(",") if n in by_name]
            if chosen:
                return chosen
        if self._ranked is None:
            try:
                self._ranked = self._rank_and_dedup()
            except Exception:
                self._ranked = list(self.interface_vars)
        return self._ranked

    def ranked_vars(self):
        return self.state_vars

    def _sample_states(self, limit=1500):
        states, _ = self.reachable(limit=limit)
        return states if len(states) >= 2 else self.trajectory(steps=400)

    def axis_bounds(self, name, pad=0.08):
        """(lo, hi) of a NUMERIC variable over the REACHABLE sample — the real domain a
        renderer should grid / scale / seed within, INSTEAD of a hardcoded ±3000 box
        (the 'fabrication' bug: gridding a guessed range invents cycles/basins the
        program never enters). Returns None for a non-numeric var or an empty sample;
        callers fall back to a default box only for genuinely unbounded continuous
        dynamics. bool is excluded (it's categorical, encode ordinally elsewhere).

        ROBUST to sentinel values: programs mark 'empty / uninitialized' with extreme
        seeds — -1 for 'none' (an empty cache slot, no node popped yet) and ±1e6 for a
        fold initializer (max seeded low, min seeded high so the first real value wins).
        A single such point would otherwise blow the axis out to ±1e6 and crush the real
        data. We (1) reject points outside an IQR fence, killing the ±1e6 sentinels, and
        (2) floor at 0 when the only remaining sub-zero value is a unit -1 'none' marker
        — while PRESERVING genuinely-negative data (a balance that really overdrafts,
        whose bulk sits below 0, keeps its negative range)."""
        states = self._sample_states()
        vals = sorted(s[name] for s in states if type(s.get(name)) in (int, float))
        if not vals:
            return None
        n = len(vals)
        q1, q3 = vals[n // 4], vals[(3 * n) // 4]
        iqr = q3 - q1
        if iqr > 0:                                  # reject ±1e6 / far-out sentinels
            lof, hif = q1 - 3 * iqr, q3 + 3 * iqr
            vals = [v for v in vals if lof <= v <= hif] or vals
        lo, hi = float(min(vals)), float(max(vals))
        if lo == hi:
            return (lo - 1.0, hi + 1.0)
        m = (hi - lo) * pad
        lo_out, hi_out = lo - m, hi + m
        if lo >= 0:                                  # non-negative-domain var: never pad below 0
            lo_out = max(0.0, lo_out)
        elif -1.0 <= lo < 0 < hi:                    # only sub-zero is a '-1 = none' sentinel: drop it
            lo_out = 0.0
        return (lo_out, hi_out)

    def _rank_and_dedup(self):
        import math
        vs = list(self.interface_vars)
        if len(vs) <= 1:
            self.variable_groups = [{"rep": v["name"], "members": [v["name"]],
                                     "entropy": 0.0} for v in vs]
            return vs
        states = self._sample_states()
        if len({self._key(s) for s in states}) < 2:   # degenerate sample (e.g. a
            return vs                                   # fixed-point seed) — don't dedup
        series = {v["name"]: [s[v["name"]] for s in states] for v in vs}

        def partition(name):
            g = {}
            for i, val in enumerate(series[name]):
                g.setdefault(val, []).append(i)
            return frozenset(frozenset(idxs) for idxs in g.values())

        def entropy(name):
            n = len(series[name])
            c = {}
            for val in series[name]:
                c[val] = c.get(val, 0) + 1
            return -sum((k / n) * math.log2(k / n) for k in c.values())

        # Group by identical partition signature = informationally equivalent
        # ('same graph'); keep one representative per group, ranked by entropy.
        groups = {}
        for v in vs:
            groups.setdefault(partition(v["name"]), []).append(v)
        reps = {}              # name -> (var, entropy, members)
        for members in groups.values():
            rep = self._pick_rep(members)
            reps[rep["name"]] = (rep, entropy(rep["name"]), [m["name"] for m in members])

        # --- structure-based axis-pair selection (replaces mRMR) ---------------
        # mRMR (max-entropy + min-redundancy) is a feature-SELECTION criterion and is
        # the wrong tool for choosing PLOT AXES: entropy over-rewards trivial tick
        # counters, and the redundancy penalty avoids exactly the *correlated* pairs (a
        # cursor-vs-accumulator staircase, two co-moving counters) that make the best
        # picture. Instead: score each pair by a structure measure that REWARDS both
        # axes meaningfully varying and DISCOUNTS unit counters — and never penalizes
        # correlation. (See viz/reviews/selector-eval.md for the audit that motivated
        # this.)
        INDEX_DISCOUNT = float(os.environ.get("EVIDENT_VIZ_DISCOUNT", "0.8"))

        def is_unit_counter(name):              # tick proxy: a loop index / clock
            return self._is_unit_counter(reps[name][0]["kind"], series[name])

        H = {nm: reps[nm][1] for nm in reps}                       # marginal entropy
        W = {nm: (INDEX_DISCOUNT if is_unit_counter(nm) else 1.0) for nm in reps}
        relevance = {nm: W[nm] * H[nm] for nm in reps}             # discounted single-var score

        # functional-redundancy guard: a pair where one axis is a DETERMINISTIC function
        # of the other AND that dependent has tiny cardinality is a degenerate "flat line
        # with a step" (e.g. grep `done = line_no >= 4`). Demote it. A RICH functional
        # curve (a staircase, `sum = f(cursor)`) keeps full score — high cardinality means
        # it traces a real shape, not a step. (This is the targeted alternative to mRMR's
        # blanket redundancy penalty, which wrongly punished staircases too.)
        def determines(a, b):                  # does a determine b? (b is a function of a)
            m = {}
            for va, vb in zip(series[a], series[b]):
                if m.get(va, vb) != vb:
                    return False
                m[va] = vb
            return True

        def low_card(nm):
            return len(set(series[nm])) <= 3

        names = list(reps)
        best_pair, best_score = None, -1.0
        for i in range(len(names)):
            for j in range(i + 1, len(names)):
                a, b = names[i], names[j]
                if H[a] <= 0 or H[b] <= 0:                         # a constant axis = a flat line
                    continue
                s = relevance[a] * relevance[b]                    # both axes vary; counters discounted
                if (determines(a, b) and low_card(b)) or (determines(b, a) and low_card(a)):
                    s *= 0.4                                        # degenerate functional step (not a rich curve)
                if s > best_score:
                    best_pair, best_score = (a, b), s

        # state_vars = the structure-optimal axis pair first, then the rest by
        # discounted relevance (so the color/facet channels also avoid trivial counters).
        # Within the pair, the more-INDEPENDENT variable (it determines the other without
        # being determined — a driver/clock) goes FIRST = on X, matching the math
        # convention (independent → X, dependent → Y). When neither drives, order is kept.
        def indep_score(nm):
            return (sum(determines(nm, x) for x in names if x != nm)
                    - sum(determines(x, nm) for x in names if x != nm))

        if best_pair:
            a, b = best_pair
            ia, ib = indep_score(a), indep_score(b)
            # The driver (higher net-determination) goes on X. If NEITHER drives (a
            # relational/cyclic model, ia == ib), put the higher-information variable on
            # X — so a rich enum (dungeon's 7-room d.room) beats a 2-value bool (d.has_key)
            # rather than collapsing the scatter onto one or two vertical lines.
            if ib > ia or (ib == ia and relevance[b] > relevance[a]):
                a, b = b, a
            best_pair = (a, b)
            rest = sorted((nm for nm in names if nm not in best_pair),
                          key=lambda nm: -relevance[nm])
            order = list(best_pair) + rest
        else:
            order = sorted(names, key=lambda nm: -relevance[nm])

        self.variable_groups = [{"rep": nm, "members": reps[nm][2],
                                 "entropy": round(H[nm], 3)} for nm in order]
        return [reps[nm][0] for nm in order]

    @staticmethod
    def _is_unit_counter(kind, vals):
        """An integer that takes a DISTINCT, CONSECUTIVE value in every sampled state — a
        loop index / clock. Injective (one value per state) so a cyclic sawtooth isn't
        flagged; consecutive so an accumulator (monotone with gaps) isn't either."""
        if kind != "int":
            return False
        d = sorted(set(vals))
        return len(d) >= 3 and len(d) == len(vals) and (d[-1] - d[0] + 1) == len(d)

    def independence(self):
        """Functional-dependency analysis of the interface variables: which behave as an
        INDEPENDENT variable — a driver/clock that determines the others without being
        determined by them — vs DEPENDENT (computed from the drivers).

        Evident models are written to be relational (solve for any variable by leaving it
        unbound), but a difference-equation model usually smuggles in a driver: a cursor
        that advances on its own, with everything else computed from it. This surfaces it.
        A model with NO driver (a cycle, a nondeterministic graph — every variable
        co-determines) is reported as 'genuinely relational'.

        Returns {'verdict': 'driven'|'relational', 'driver': name|None,
                 'drivers': [names], 'dependents': [names], 'score': {name: net}} where
        net = #(vars it determines) − #(vars that determine it): positive = a driver,
        negative = a pure dependent, ~0 = mutual / cyclic."""
        vs = [v["name"] for v in self.interface_vars]
        states = self._sample_states()
        if not vs or len({self._key(s) for s in states}) < 2:
            # nothing varies across the sample (constant / degenerate) — no driver to find
            return {"verdict": "relational", "driver": None, "drivers": [],
                    "dependents": [], "score": {n: 0 for n in vs}}
        if len(vs) == 1:
            # A lone carried variable that VARIES is its own clock: a deterministic
            # self-recurrence x_t = f(x_{t-1}). That is the MOST driven shape, not a
            # relational cycle — every FSM reads its own _prev, so self-carry must not be
            # mistaken for co-determination. (A nondeterministic lone var is caught by the
            # branching override in the banner upstream.)
            return {"verdict": "driven", "driver": vs[0], "drivers": list(vs),
                    "dependents": [], "score": {vs[0]: 0}}
        series = {n: [s[n] for s in states] for n in vs}

        def determines(a, b):                  # each a-value maps to a unique b-value
            m = {}
            for va, vb in zip(series[a], series[b]):
                if m.get(va, vb) != vb:
                    return False
                m[va] = vb
            return True

        det = {a: set(b for b in vs if a != b and determines(a, b)) for a in vs}
        score = {a: len(det[a]) - sum(1 for b in vs if a in det[b]) for a in vs}
        top = max(score.values())
        drivers = [a for a in vs if score[a] == top] if top > 0 else []
        dependents = sorted((a for a in vs if score[a] < 0), key=lambda a: score[a])
        driver = None
        if drivers:                            # canonical driver: prefer the unit counter
            kind = {v["name"]: v["kind"] for v in self.interface_vars}
            driver = sorted(drivers, key=lambda a: (
                not self._is_unit_counter(kind[a], series[a]), -len(det[a]), len(a)))[0]
        return {"verdict": "driven" if drivers else "relational", "driver": driver,
                "drivers": drivers, "dependents": dependents, "score": score}

    def solution_structure(self, limit=300, states=None, edges=None):
        """Solver-computed structure of the WHOLE model — not a single sampled run.

        The diagrams elsewhere show one forward trajectory; this asks the solver structural
        questions about the transition RELATION directly:
          - **fixed points** — states s with `T(s, s)` (the state maps to itself),
            enumerated rigorously by solving the relation, not by spotting a self-loop in a
            sampled orbit. THIS is a real equilibrium; an empty list means the model truly
            has none (a pure cycle).
          - **verdict** — does the forward orbit converge to a fixed point (`terminates`),
            revisit states without one (`cyclic`), have an equilibrium it runs AWAY from
            (`unstable` — a fixed point exists but the orbit diverges), or grow without
            bound (`unbounded`).
          - **bounds** — the exact min..max each numeric carried variable spans over the
            reachable set: the boundary of the solution space (exact when the set is finite,
            a `≥` floor when the exploration was capped).
        """
        short = lambda n: n.split(".")[-1]
        fmt = lambda x: (x.as_long() if hasattr(x, "as_long") else str(x))
        nf = (z3.Not(self.consts[self._first_tick_name])
              if self.consts.get(self._first_tick_name) is not None else z3.BoolVal(True))

        # (1) reachable set FIRST — its self-loops ARE the reachable fixed points, and it
        # gives the exact bounds (reuse the caller's reachable set if provided).
        if states is None:
            try:
                states, edges = self.reachable(limit=limit)
            except Exception:
                states, edges = [], []
        edges = edges or []
        n = len(states)
        capped = n >= limit
        terminal = any(i == j for (i, j) in edges)        # an absorbing self-loop

        # (2) reachable fixed points: a reachable state that maps to ITSELF (a self-loop edge).
        # Reading them off the reachable graph keeps them TRUE — never an unreachable state
        # (Marek's #50) — and DETERMINISTIC once sorted (Marek's #51), unlike enumerating raw
        # T(s,s) solutions, which also returns unreachable equilibria in non-deterministic order.
        fp_idx = sorted({i for (i, j) in edges if i == j})
        fps = sorted(
            ({short(v["name"]): states[i][v["name"]]
              for v in self.carried if v["name"] in states[i]} for i in fp_idx),
            key=lambda d: [(k, str(d[k])) for k in sorted(d)])[:8]

        # Does ANY equilibrium exist (T(s,s)), reachable or not? A reachable one already shows
        # up above; an UNREACHABLE one — the oscillator's origin, which the orbit diverges from
        # — yields no fixed point but flips the verdict to 'unstable'.
        self_eqs = [self.consts[v["name"]] == self.consts[v["prev"]] for v in self.carried
                    if self.consts.get(v["name"]) is not None and self.consts.get(v["prev"]) is not None]
        equilibria_exist = False
        if self_eqs:
            sfp = z3.Solver()
            for a in self.assertions:
                sfp.add(a)
            sfp.add(nf)
            sfp.add(z3.And(*self_eqs))
            equilibria_exist = sfp.check() == z3.sat

        # (3) exact bounds over the reachable set (floats rounded — Marek's #39).
        rnd = lambda x: round(x, 3) if isinstance(x, float) else x
        bounds = {}
        for v in self.carried:
            if v.get("kind") in ("int", "real"):
                nums = [s[v["name"]] for s in states
                        if isinstance(s.get(v["name"]), (int, float))]
                if nums:
                    bounds[short(v["name"])] = [rnd(min(nums)), rnd(max(nums))]

        out_deg = {}
        for (i, j) in edges:
            out_deg[i] = out_deg.get(i, 0) + 1
        max_branch = max(out_deg.values()) if out_deg else 1

        has_fp = bool(fps)                    # rests at a REACHABLE fixed point
        if max_branch >= 2:
            verdict = "nondeterministic"      # a free choice fans out
        elif has_fp and terminal and not capped:
            verdict = "terminates"            # the orbit converges to a reachable fixed point
        elif equilibria_exist and capped:
            verdict = "unstable"              # an equilibrium exists but the orbit diverges from it
        elif not equilibria_exist and not capped:
            verdict = "cyclic"                # revisits states, no fixed point
        elif capped:
            verdict = "unbounded"             # grows without bound
        else:
            verdict = "settles"

        return {"fixed_points": fps, "has_fixed_point": has_fp, "verdict": verdict,
                "bounds": bounds, "reachable": n, "capped": capped, "branching": max_branch}

    def independence_structural(self, seeds=4, alts_per_field=2):
        """Directed dependency by solver SENSITIVITY — the RIGOROUS form of
        `independence()`. Probes the transition RELATION rather than the sampled
        trajectory: `state.X` depends on `_state.Y` iff perturbing `_Y` (holding the rest
        of the previous state) CHANGES `state.X`. This reads off the actual computational
        form (`state.sum = _sum + val[_cursor]` responds to `_cursor`) regardless of
        whether the sample happens to expose it, so it can't be fooled by trajectory
        coincidences the way the reachable-behavior version can.

        Driver = a SOURCE of the dependency DAG: high out-degree (many fields computed
        from it), low in-degree (its own next value depends on no OTHER field — a
        self-running clock). Same return shape as `independence()`.

        score(v) = out_degree(v) − in_degree(v): positive = driver, negative = pure
        dependent, ~0 across the board = mutual / cyclic = genuinely relational."""
        fields = [v["name"] for v in self.interface_vars]
        states = self._sample_states()
        if len(fields) < 2 or not states:
            return {"verdict": "relational", "driver": None, "drivers": [],
                    "dependents": [], "score": {n: 0 for n in fields}}
        # Structural sensitivity needs a DETERMINISTIC transition: a nondeterministic
        # successor() returns ONE arbitrary choice, so perturb-vs-base would conflate
        # dependency with that choice. For a nondeterministic model the 'independent
        # variable' is the nondeterministic CHOICE itself (the free input) — not a state
        # field — so report that honestly rather than inventing a driver.
        probe = states[:seeds]
        if any(len(self.successors(s)) > 1 for s in probe):
            return {"verdict": "nondeterministic", "driver": None, "drivers": [],
                    "dependents": [], "score": {n: 0 for n in fields}}
        alts = {f: sorted({s[f] for s in states}) for f in fields}
        dep = {x: set() for x in fields}          # dep[x] = {y : state.x depends on _state.y}
        for _s in states[:seeds]:
            base = self.successor(_s)             # the next state for this exact previous
            if base is None:
                continue
            for y in fields:
                tried = 0
                for yv in alts[y]:
                    if yv == _s[y] or tried >= alts_per_field:
                        continue
                    tried += 1
                    pert = self.successor({**_s, y: yv})    # perturb ONLY _state.y
                    if pert is None:
                        continue
                    for x in fields:                        # which next-fields moved?
                        if x != y and base.get(x) != pert.get(x):
                            dep[x].add(y)
        out_deg = {y: sum(1 for x in fields if y in dep[x]) for y in fields}
        in_deg = {x: len(dep[x]) for x in fields}
        score = {f: out_deg[f] - in_deg[f] for f in fields}
        top = max(score.values())
        drivers = [f for f in fields if score[f] == top] if top > 0 else []
        dependents = sorted((f for f in fields if score[f] < 0), key=lambda f: score[f])
        driver = None
        if drivers:
            kind = {v["name"]: v["kind"] for v in self.interface_vars}
            ser = {f: [s[f] for s in states] for f in drivers}
            driver = sorted(drivers, key=lambda a: (
                not self._is_unit_counter(kind[a], ser[a]), -out_deg[a], len(a)))[0]
        return {"verdict": "driven" if drivers else "relational", "driver": driver,
                "drivers": drivers, "dependents": dependents, "score": score}

    @staticmethod
    def _pick_rep(members):
        # most interpretable kind first, then shortest name
        order = {"enum": 0, "string": 1, "int": 2, "real": 3, "bool": 4}
        return sorted(members, key=lambda v: (order.get(v["kind"], 9), len(v["name"])))[0]

    # ---- channel mapping: ranked vars -> visual channels by type-effectiveness ----
    @staticmethod
    def var_class(v):
        """'quant' (int/real — position/size) or 'cat' (bool/enum/string — color/facet)."""
        return "cat" if v["kind"] in ("bool", "enum", "string") else "quant"

    @property
    def numeric_vars(self):
        """Ranked numeric (quantitative) interface vars — for axes/size."""
        return [v for v in self.state_vars if self.var_class(v) == "quant"]

    @property
    def categorical_vars(self):
        """Ranked categorical (enum/bool/string) interface vars — for color/facet."""
        return [v for v in self.state_vars if self.var_class(v) == "cat"]

    def assign_channels(self, channels, min_fit=0.3):
        """Map the ranked+deduped variables onto a renderer's declared visual
        `channels` (names from CHANNEL_FITNESS) by importance x type-effectiveness:
        position for the top vars, color/facet for categoricals, size for secondary
        numerics. Returns {channel: var | None}. A channel stays None if no remaining
        variable suits it. Color/size/facet are SECONDARY — keep the plot readable
        from the axes alone."""
        assignment = {ch: None for ch in channels}
        free = [ch for ch in channels if ch in CHANNEL_FITNESS]
        for v in self.state_vars:
            cls = self.var_class(v)
            # only channels where this var's type is decoded well enough
            cand = [ch for ch in free if CHANNEL_FITNESS[ch][cls] >= min_fit]
            if not cand:
                continue                       # skip this var; a later one may fit
            # best fitness, ties broken toward the earlier-declared channel (x < y)
            best = max(cand, key=lambda ch: (CHANNEL_FITNESS[ch][cls], -channels.index(ch)))
            assignment[best] = v
            free.remove(best)
        return assignment

    # ---- faceting guard: only facet by a var that stays ~constant within a run ----
    @property
    def change_rates(self):
        """Per-interface-var fraction of transitions where the variable CHANGES
        value (within-run 'dynamism'). A good FACET variable has a LOW rate — it's a
        config/regime set once, not something on the trajectory. Cached."""
        if self._change_rates_cache is None:
            states, edges = self.reachable(limit=1500)
            if len(edges) < 1:                       # numeric / degenerate
                states = self.trajectory(steps=200)
                edges = [(i, i + 1) for i in range(len(states) - 1)]
            n = max(len(edges), 1)
            self._change_rates_cache = {
                v["name"]: sum(1 for i, j in edges
                               if states[i][v["name"]] != states[j][v["name"]]) / n
                for v in self.interface_vars} if edges else {}
        return self._change_rates_cache

    def facet_var(self, max_card=6, max_change=0.25):
        """The variable to FACET by (small multiples), or None — then DON'T facet.
        Must be a low-cardinality CATEGORICAL that stays ~constant within a run, so
        the dynamics live INSIDE a panel rather than being cut across panels — but it
        must ACTUALLY PARTITION the run (>=2 distinct reachable values). Faceting by a
        var that is constant across the whole reachable set (e.g. find's s5, always
        Unseen) gives one populated panel and the rest empty — worse than not faceting."""
        rates = self.change_rates
        states = self._sample_states()
        cands = []
        for v in self.categorical_vars:
            card = len(self.enum_variants.get(v["name"], [])) or 2   # bool -> 2
            distinct = len({s[v["name"]] for s in states})           # actual reachable values
            if (2 <= distinct <= max_card and card <= max_card
                    and rates.get(v["name"], 1.0) <= max_change):
                cands.append(v)
        cands.sort(key=lambda v: rates[v["name"]])      # most static (but varying) first
        return cands[0] if cands else None
