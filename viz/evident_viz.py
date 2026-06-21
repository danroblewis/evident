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
        self.internal_vars = [v for v in self.carried if v.get("role") == "internal"]
        self._ranked = None          # cached ranked+deduped interface vars (lazy)
        self.variable_groups = []    # [{rep, members, entropy}] redundancy groups
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
        for v in self.carried:
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
            st = self._read_state(s.model())
            out.append(st)
            s.add(z3.Or([self.consts[v["name"]] != self._lit(v, st[v["name"]])
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
        return "(" + ", ".join(str(state[v["name"]]) for v in self.interface_vars) + ")"

    # ---- variable ranking: dedup redundant ('same-graph') vars, rank the rest ----
    @property
    def state_vars(self):
        """Interface variables, deduplicated (informationally-equivalent vars merged)
        and ranked by how much they vary — the recommended axis ORDER. Renderers take
        as many as they need (top 2 for a phase portrait, all for a scatter matrix).
        Falls back to raw interface order if sampling is degenerate. Cached."""
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

        def mi(a, b):
            n = len(states)
            pa, pb, pab = {}, {}, {}
            for i in range(n):
                va, vb = series[a][i], series[b][i]
                pa[va] = pa.get(va, 0) + 1
                pb[vb] = pb.get(vb, 0) + 1
                pab[(va, vb)] = pab.get((va, vb), 0) + 1
            return sum((c / n) * math.log2((c / n) / ((pa[k[0]] / n) * (pb[k[1]] / n)))
                       for k, c in pab.items())

        # Greedy max-relevance / min-redundancy ordering: most informative var first,
        # then each next maximizes entropy while staying least redundant with those
        # already chosen — so state_vars[:2] is the most EXPRESSIVE axis pair and any
        # prefix is a good non-redundant set.
        names = sorted(reps, key=lambda nm: -reps[nm][1])
        order = [names.pop(0)] if names else []
        while names:
            def score(nm):
                red = max((mi(nm, p) / (min(reps[nm][1], reps[p][1]) or 1e-9)
                           for p in order), default=0.0)
                return reps[nm][1] * (1.0 - min(red, 1.0))
            best = max(names, key=score)
            names.remove(best)
            order.append(best)

        self.variable_groups = [{"rep": nm, "members": reps[nm][2],
                                 "entropy": round(reps[nm][1], 3)} for nm in order]
        return [reps[nm][0] for nm in order]

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
