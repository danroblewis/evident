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
        # DERIVED vars: scalars the transition determines as a pure function of the
        # carried state (e.g. `done ∈ Bool = (count ≥ 5)`) but does NOT carry. Read for
        # DISPLAY only and kept OUT of `self.carried` so they never widen the
        # reachable-graph identity (_key / dedup / pin all key on carried). The
        # time_series renderer plots their bool/enum/int values as extra tracks. See
        # export_transition's "derived" array (query.rs).
        self.derived = schema.get("derived", [])           # [{name, kind, role, variants?}]
        self._ranked = None          # cached ranked+deduped interface vars (lazy)
        self.variable_groups = []    # [{rep, members, entropy}] redundancy groups
        self._change_rates_cache = None
        self._first_tick_name = schema["is_first_tick"]

        # Two-tick (ΔΔ / second-difference) models read TWO ticks back: a carried
        # leaf with hist==2 has a `__x` (two-ticks-ago) twin bound in the transition.
        # Such a model's "state" for reachability is the PAIR (cur, prev) — the next
        # tick depends on both _x=cur AND __x=prev — and tick 1 is bootstrapped by an
        # is_second_tick flag. One-tick models (every existing demo) have no hist-2
        # leaf, no is_second_tick field, and take the unchanged single-snapshot path.
        self.two_tick_vars = [v for v in self.carried if v.get("hist", 1) == 2]
        self.has_two_tick = bool(self.two_tick_vars)
        self._second_tick_name = schema.get("is_second_tick")

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
        self.second_tick = (self.consts.get(self._second_tick_name)
                            if self._second_tick_name else None)

        # For each enum state var (carried OR derived), map variant-name -> z3 value
        # (nullary ctor). Derived enums are included so a derived enum track can be
        # rendered/coerced the same way a carried one is.
        self.enum_variants = {}            # var name -> [variant names]
        self._enum_lit = {}                # var name -> {variant: z3 value}
        for v in self.carried + self.derived:
            if v["kind"] == "enum" and v["name"] in self.consts:
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
        # Carried leaves define the state; derived vars are read too (for DISPLAY) but
        # never enter `_key` — a derived var is a function of carried state, so it must
        # not change the reachable-graph identity. `_read_derived` swallows any var the
        # solved model doesn't bind (e.g. a derived var dropped by z3's parser).
        st = {v["name"]: self._read(model, v) for v in self.carried}
        for v in self.derived:
            val = self._read_derived(model, v)
            if val is not None:
                st[v["name"]] = val
        return st

    def _read_derived(self, model, var):
        """Read a derived var's value from the solved model for display. Returns None
        if the var isn't in the parsed smt2 (so it never fabricates a value)."""
        c = self.consts.get(var["name"])
        if c is None:
            return None
        return self._read(model, var)

    def _pin_prev(self, solver, state):
        # Pin only the leaves the caller supplied; a renderer may pass a PARTIAL
        # state (e.g. just the deduped axis vars), leaving the rest free. Pinning
        # all of self.carried would KeyError on a leaf the caller omitted.
        for v in self.carried:
            if v["name"] in state:
                solver.add(self.consts[v["prev"]] == self._lit(v, state[v["name"]]))

    def _pin_prev2(self, solver, prev_state):
        # Pin the TWO-ticks-ago twin (`__x`) for the hist-2 leaves. Only the two-tick
        # vars have a `__x` const; one-tick vars have nothing two ticks back.
        for v in self.two_tick_vars:
            if v["name"] in prev_state:
                c = self.consts.get("__" + v["name"])
                if c is not None:
                    solver.add(c == self._lit(v, prev_state[v["name"]]))

    def _successors_two(self, cur, prev, limit=64):
        """ALL distinct next CURRENT-snapshots from the (cur, prev) pair of a two-tick
        model: pin `_x = cur` AND `__x = prev`, with is_first_tick = is_second_tick =
        false. When `prev` is None (the step OUT of the first tick) we pin only `_x =
        cur` and set is_second_tick = true — the bootstrap tick the model handles
        without a two-ago value. Returns a list of current-snapshot state dicts."""
        s = self._base()
        if self.first_tick is not None:
            s.add(self.first_tick == False)  # noqa: E712
        self._pin_prev(s, cur)
        if prev is None:
            if self.second_tick is not None:
                s.add(self.second_tick == True)  # noqa: E712
        else:
            if self.second_tick is not None:
                s.add(self.second_tick == False)  # noqa: E712
            self._pin_prev2(s, prev)
        out = []
        while len(out) < limit and s.check() == z3.sat:
            mod = s.model()
            out.append(self._read_state(mod))
            s.add(z3.Or([self.consts[v["name"]] != mod.eval(self.consts[v["name"]],
                                                            model_completion=True)
                         for v in self.carried]))
        return out

    def _reachable_two(self, limit=5000):
        """Reachable set for a two-tick (ΔΔ) model. A NODE is the pair (cur, prev):
        the transition depends on both. We BFS over pairs, but the returned `states`
        are the CURRENT snapshots only (and `edges` index into them) so every
        downstream consumer — phase_portrait / solution_space / solved_bounds /
        check_invariant / check_temporal — sees ordinary single-snapshot states and
        works unchanged. Dedup is on the (cur, prev) pair."""
        init = self.initial_state()
        if init is None:
            return [], []
        # The pair-graph: each node carries (cur, prev); states[] holds the cur dicts.
        states = [init]
        pairs = [(init, None)]                       # tick-0 node: no prev yet
        pair_index = {(self._key(init), None): 0}
        edges = []
        frontier = [0]
        while frontier and len(states) < limit:
            i = frontier.pop()
            cur, prev = pairs[i]
            for nxt in self._successors_two(cur, prev):
                pk = (self._key(nxt), self._key(cur))
                if pk not in pair_index:
                    pair_index[pk] = len(states)
                    states.append(nxt)
                    pairs.append((nxt, cur))
                    frontier.append(pair_index[pk])
                edges.append((i, pair_index[pk]))
        return states, edges

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
        if self.has_two_tick:
            return self._reachable_two(limit)
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

    def check_invariant(self, var, op, value, limit=400):
        """VERIFY a safety property over the reachable set: does `var op value` hold
        on EVERY reachable state? If not, return the first reachable counterexample.

        This is the *verification* counterpart to the dynamics queries above: instead
        of watching the orbit, it asks "is this predicate an invariant of the whole
        reachable set?" — the question a model-checker answers.

        Predicate spec — a single comparison over ONE carried var (by short or full
        name, e.g. "balance" or "state.balance"):
          op ∈ {"<=", "<", ">=", ">", "=", "!="}  (aliases: "==", "≤", "≥", "≠")
          value: a python int/float for numeric vars, a bool for bool vars
                 (true/false, "true"/"false", 1/0), a variant NAME (str) for enums.
        Examples: check_invariant("count", "<=", 5), ("balance", ">=", 0),
                  ("done", "=", True), ("mode", "!=", "Vending").

        SOUNDNESS: this checks the predicate on the BFS-enumerated reachable set
        (`reachable(limit)`). For a finite reachable set the BFS exhausts
        (`exhaustive=True`), HOLDS is a genuine proof — every reachable state was
        tested. When the BFS hit `limit` (`exhaustive=False`), a 'holds' is only
        "holds on the states explored so far"; a counterexample is ALWAYS real (the
        violating state IS reachable). For enum/bool/`=`/`!=` predicates the
        reachable set is the exact thing to check; for unbounded numeric dynamics
        raise `limit` or read HOLDS as bounded.

        DETERMINISM: states are tested in BFS-discovery order (the order `reachable`
        appends them), so the returned counterexample is stable run to run — the
        first reachable violator, not an arbitrary z3 model.

        Returns:
          {
            "holds": bool,                       # predicate true on every checked state
            "checked": int,                      # number of reachable states tested
            "exhaustive": bool,                  # did the BFS exhaust the reachable set?
            "counterexample": {var: value, ...}  # first violating FULL state, or None
                              | None,
            "violating_value": <value> | None,   # the var's value in the counterexample
            "predicate": "balance ≥ 0",          # human-readable, with the short var name
          }
        """
        OPS = {"<=": "<=", "≤": "<=", "<": "<", ">=": ">=", "≥": ">=", ">": ">",
               "=": "=", "==": "=", "!=": "!=", "≠": "!="}
        canon = OPS.get(op)
        if canon is None:
            raise ValueError(f"unknown op {op!r}; use one of <= < >= > = !=")

        # Resolve the var by full name ("state.balance") or short name ("balance").
        v = self._resolve_carried(var)
        if v is None:
            known = ", ".join(sorted({w["name"] for w in self.carried}))
            raise ValueError(f"unknown carried var {var!r}; known: {known}")
        name = v["name"]

        target = self._coerce_predicate_value(v, value, canon)
        pretty = {"<=": "≤", "<": "<", ">=": "≥", ">": ">", "=": "=", "!=": "≠"}[canon]
        predicate = f"{name.split('.')[-1]} {pretty} {self._fmt_val(target)}"

        def ok(sv):
            # A state that doesn't carry this leaf can't be judged — treat as
            # vacuously satisfying so a partial state never fabricates a violation.
            if name not in sv:
                return True
            x = sv[name]
            if canon == "=":
                return x == target
            if canon == "!=":
                return x != target
            if canon == "<=":
                return x <= target
            if canon == "<":
                return x < target
            if canon == ">=":
                return x >= target
            return x > target  # ">"

        states, edges = self.reachable(limit=limit)
        exhaustive = len(states) < limit          # BFS stopped on its own, not capped
        checked = 0
        for idx, sv in enumerate(states):          # BFS-discovery order = deterministic
            checked += 1
            if not ok(sv):
                return {"holds": False, "checked": checked, "exhaustive": exhaustive,
                        "counterexample": dict(sv), "violating_value": sv.get(name),
                        "trace": self._trace_to(idx, edges, states),  # the path init→violation
                        "predicate": predicate}
        return {"holds": True, "checked": checked, "exhaustive": exhaustive,
                "counterexample": None, "violating_value": None, "trace": None,
                "predicate": predicate}

    def _trace_to(self, target, edges, states):
        """The shortest path of STATES from the initial state (index 0) to `target`, via BFS over
        the reachable graph — so a counterexample comes with the trajectory that reaches it
        (Ana #173/#175), not just the offending state."""
        path = self._bfs_indices(0, target, edges, restrict=None)
        if path is None:
            return [dict(states[target])]
        return [dict(states[k]) for k in path]

    @staticmethod
    def _bfs_indices(source, target, edges, restrict=None):
        """Shortest path of node INDICES from `source` to `target` via BFS over `edges`.
        `restrict`, if given, is a node set the path must stay inside (every visited node
        must be in it) — used to keep the stem inside the ¬Q subgraph so it genuinely
        dodges Q. Returns the index list (inclusive of both ends) or None if unreachable."""
        import collections
        if restrict is not None and (source not in restrict or target not in restrict):
            return None
        if source == target:
            return [source]
        adj = collections.defaultdict(list)
        for a, b in edges:
            if restrict is None or (a in restrict and b in restrict):
                adj[a].append(b)
        prev = {source: None}
        q = collections.deque([source])
        while q:
            u = q.popleft()
            if u == target:
                break
            for w in adj[u]:
                if w not in prev:
                    prev[w] = u
                    q.append(w)
        if target not in prev:
            return None
        path, u = [], target
        while u is not None:
            path.append(u)
            u = prev[u]
        return list(reversed(path))

    def _lasso(self, start_idx, bad, sub, g, can_reach_q, edges, states):
        """Build the STEM + CYCLE lasso witnessing that a run from `start_idx` dodges Q forever,
        plus the FAIRNESS classification (Ana #239). The stem is the shortest path INSIDE the ¬Q
        subgraph from `start_idx` to the entry of a dodging cycle/sink; the cycle is the actual ¬Q
        loop back to that entry (or [] when the dodge is a terminal ¬Q sink). `forced` is True iff
        NO state on the cycle has an out-edge in the FULL graph to a state from which Q is reachable
        — i.e. the run literally cannot escape to Q even under weak fairness.

        Returns (stem_states, cycle_states, forced). `stem` always includes the cycle entry as its
        last element; `cycle` is the entry-to-entry loop (entry repeated at the end) or [] for a sink.
        """
        import networkx as nx
        # Pick the nearest dodging target inside ¬Q reachable from start (shortest stem).
        entry, stem_idx = None, None
        seen = self._bfs_reach(start_idx, sub)            # ¬Q nodes reachable from start, by distance
        for cand in sorted(seen, key=seen.get):
            if cand in bad:
                entry, stem_idx = cand, self._bfs_indices(start_idx, cand, edges, restrict=set(sub.nodes))
                if stem_idx is not None:
                    break
        if entry is None or stem_idx is None:             # defensive: fall back to a plain trace
            return [dict(states[start_idx])], [], True
        stem = [dict(states[k]) for k in stem_idx]

        # The cycle: a ¬Q loop from entry back to entry. A terminal ¬Q sink has none.
        cycle_idx = []
        if g.out_degree(entry) > 0:                       # not a stuck terminal
            if sub.has_edge(entry, entry):                # self-loop ¬Q cycle
                cycle_idx = [entry, entry]
            else:
                try:                                       # a directed cycle through entry within ¬Q
                    found = nx.find_cycle(sub, source=entry)
                    cyc_nodes = [u for (u, _v) in found]
                    cycle_idx = cyc_nodes + [cyc_nodes[0]]
                except nx.NetworkXNoCycle:
                    cycle_idx = []
        cycle = [dict(states[k]) for k in cycle_idx]

        # FAIRNESS: forced iff no cycle state can step (in the FULL graph) into can_reach_q.
        cyc_states = set(cycle_idx[:-1]) if cycle_idx else set()
        forced = True
        for u in cyc_states:
            for w in g.successors(u):
                if w in can_reach_q:
                    forced = False
                    break
            if not forced:
                break
        return stem, cycle, forced

    @staticmethod
    def _bfs_reach(source, graph):
        """{node: distance} for every node reachable from `source` within `graph` (BFS)."""
        import collections
        dist, q = {source: 0}, collections.deque([source])
        while q:
            u = q.popleft()
            for w in graph.successors(u):
                if w not in dist:
                    dist[w] = dist[u] + 1
                    q.append(w)
        return dist

    def _predicate(self, var, op, value):
        """Build (full_name, short_pretty_predicate, fn) for a `var op value` comparison —
        the shared core of check_invariant/check_temporal. fn(state)→bool, vacuously true on a
        state that doesn't carry the leaf."""
        OPS = {"<=": "<=", "≤": "<=", "<": "<", ">=": ">=", "≥": ">=", ">": ">",
               "=": "=", "==": "=", "!=": "!=", "≠": "!="}
        canon = OPS.get(op)
        if canon is None:
            raise ValueError(f"unknown op {op!r}; use one of <= < >= > = !=")
        v = self._resolve_carried(var)
        if v is None:
            known = ", ".join(sorted({w["name"] for w in self.carried}))
            raise ValueError(f"unknown carried var {var!r}; known: {known}")
        name = v["name"]
        target = self._coerce_predicate_value(v, value, canon)
        pretty = {"<=": "≤", "<": "<", ">=": "≥", ">": ">", "=": "=", "!=": "≠"}[canon]
        cmp = {"=": lambda x: x == target, "!=": lambda x: x != target,
               "<=": lambda x: x <= target, "<": lambda x: x < target,
               ">=": lambda x: x >= target, ">": lambda x: x > target}[canon]
        def fn(sv):
            return True if name not in sv else cmp(sv[name])
        return name, f"{name.split('.')[-1]} {pretty} {self._fmt_val(target)}", fn

    def query(self, terms, limit=400):
        """SOLVE an existential query over the reachable set: is there a reachable state
        satisfying the CONJUNCTION of `terms`? This is the dual of check_invariant —
        instead of "does P hold on EVERY reachable state (□)", it asks "does ANY reachable
        state satisfy P₁ ∧ P₂ ∧ … (◇/∃)" — the Z3/Alloy `(assert)(check-sat)` move, run
        against the loaded model without editing the source.

        `terms` is a list of `(var, op, value)` triples (each in the same spec as
        check_invariant); the query is their conjunction. An empty `terms` matches every
        reachable state (so `count` == #reachable).

        Returns:
          {
            "satisfiable": bool,                 # some reachable state satisfies all terms
            "witness": {var: value, ...} | None, # first such FULL state (BFS order), or None
            "count": int,                        # how many reachable states satisfy it
            "checked": int,                      # number of reachable states scanned
            "exhaustive": bool,                  # did the BFS exhaust the reachable set?
            "trace": [state, ...] | None,        # path init→witness, or None
            "predicate": "light = Green ∧ timer = 2",  # human-readable conjunction
          }
        """
        # Build a (full_name, pretty, fn) per term, reusing _predicate (no duplication).
        built = [self._predicate(var, op, value) for (var, op, value) in terms]
        predicate = " ∧ ".join(p for (_, p, _) in built) if built else "true"

        def sat(sv):
            return all(fn(sv) for (_, _, fn) in built)

        states, edges = self.reachable(limit=limit)
        exhaustive = len(states) < limit          # BFS stopped on its own, not capped
        match_idxs = [idx for idx, sv in enumerate(states) if sat(sv)]   # BFS order = deterministic
        count = len(match_idxs)
        if not match_idxs:
            return {"satisfiable": False, "witness": None, "count": 0, "matches": [],
                    "checked": len(states), "exhaustive": exhaustive,
                    "trace": None, "predicate": predicate}
        first_idx = match_idxs[0]
        # Enumerate ALL matching reachable states so the caller can WALK them (the SAT dual of the
        # all-cores enumeration — Alloy's "every instance", Ana #241). Capped; `matches_capped` flags more.
        MATCH_CAP = 40
        matches = [dict(states[i]) for i in match_idxs[:MATCH_CAP]]
        return {"satisfiable": True, "witness": dict(states[first_idx]), "count": count,
                "matches": matches, "matches_capped": count > MATCH_CAP,
                "checked": len(states), "exhaustive": exhaustive,
                "trace": self._trace_to(first_idx, edges, states),  # path init→witness
                "predicate": predicate}

    def check_temporal(self, var, op, value, modality="eventually",
                       p_var=None, p_op=None, p_value=None, limit=400):
        """VERIFY a LIVENESS property over the reachable graph (the model-checker move beyond the
        safety □ that check_invariant does):
          - modality "eventually" (◇Q): does EVERY run from the initial state reach a Q-state?
          - modality "leads_to"   (P ⤳ Q): from every reachable P-state, is Q eventually reached?
        A run can AVOID Q forever iff it reaches a ¬Q cycle or gets stuck in a ¬Q terminal. We
        compute that 'avoid' set (backward reachability, within the ¬Q subgraph, from ¬Q cycles ∪
        ¬Q sinks); ◇Q holds iff the initial state isn't in it, P⤳Q iff no P-state is.

        On failure we return a verifiable STEM+CYCLE LASSO (Ana #239), not just a single state:
          - `stem`:  shortest path INSIDE ¬Q from the offending state to the dodging-cycle entry.
          - `cycle`: the actual ¬Q loop back to that entry ([] for a stuck terminal ¬Q sink).
          - `trace`: stem + cycle (one walk), kept for the stepper; `cycle_start` indexes where
                     the cycle begins inside `trace`.
          - `forced`: the FAIRNESS verdict — True iff no cycle state can step (in the FULL graph)
                     into a state from which Q is reachable (a real counterexample even under weak
                     fairness); False (AVOIDABLE) iff some cycle state has a fair successor that
                     escapes to Q — under fairness that successor eventually fires and Q holds."""
        import networkx as nx
        qname, qpred, qfn = self._predicate(var, op, value)
        states, edges = self.reachable(limit=limit)
        n = len(states)
        if n == 0:
            return {"holds": True, "checked": 0, "exhaustive": True,
                    "counterexample": None, "predicate": f"◇ {qpred}"}
        exhaustive = n < limit
        g = nx.DiGraph(); g.add_nodes_from(range(n)); g.add_edges_from(edges)
        notq = [i for i in range(n) if not qfn(states[i])]
        sub = g.subgraph(notq)
        bad = {i for i in notq if g.out_degree(i) == 0}        # ¬Q terminal: stuck, never Q
        for comp in nx.strongly_connected_components(sub):     # ¬Q cycle: loop forever in ¬Q
            if len(comp) >= 2 or any(sub.has_edge(c, c) for c in comp):
                bad |= set(comp)
        avoid, stack = set(bad), list(bad)                     # states that can dodge Q forever
        while stack:                                           # backward reach within the ¬Q subgraph
            for w in sub.predecessors(stack.pop()):            # w → (a bad/avoiding state), staying ¬Q
                if w not in avoid:
                    avoid.add(w); stack.append(w)

        # can_reach_q: every node from which a Q-state is reachable in the FULL graph (reverse
        # reachability from Q over g) — the fairness oracle, computed once.
        qset = [i for i in range(n) if qfn(states[i])]
        can_reach_q, qstack = set(qset), list(qset)
        while qstack:
            for w in g.predecessors(qstack.pop()):
                if w not in can_reach_q:
                    can_reach_q.add(w); qstack.append(w)

        if modality == "leads_to":
            _, ppred, pfn = self._predicate(p_var, p_op, p_value)
            offenders = [i for i in range(n) if pfn(states[i]) and i in avoid]
            holds = not offenders
            pred = f"{ppred} ⤳ {qpred}"
            if holds:
                return {"holds": True, "checked": n, "exhaustive": exhaustive,
                        "counterexample": None, "trace": None, "stem": None, "cycle": None,
                        "cycle_start": None, "forced": None, "predicate": pred}
            return self._fail_result(offenders[0], bad, sub, g, can_reach_q, edges, states,
                                     n, exhaustive, pred)

        holds = 0 not in avoid                                 # initial state is index 0
        pred = f"◇ {qpred}"
        if holds:
            return {"holds": True, "checked": n, "exhaustive": exhaustive,
                    "counterexample": None, "trace": None, "stem": None, "cycle": None,
                    "cycle_start": None, "forced": None, "predicate": pred}
        return self._fail_result(0, bad, sub, g, can_reach_q, edges, states,
                                 n, exhaustive, pred)

    def _fail_result(self, start_idx, bad, sub, g, can_reach_q, edges, states, n, exhaustive, pred):
        """Package a failing liveness check as the stem+cycle lasso + fairness flag. `trace`
        (= stem + cycle, the cycle entry not repeated) stays for back-compat with the stepper;
        `cycle_start` is the index into `trace` where the loop begins."""
        stem, cycle, forced = self._lasso(start_idx, bad, sub, g, can_reach_q, edges, states)
        # trace = stem then the cycle body (drop the repeated entry: stem already ends on it,
        # and the cycle's trailing entry is the same node — so the walk reads init→…→entry→…→entry).
        cycle_body = cycle[1:] if cycle else []                # cycle = [entry, …, entry]; skip lead entry
        trace = stem + cycle_body
        cycle_start = len(stem) - 1 if cycle else None         # the entry (last stem state) opens the loop
        return {"holds": False, "checked": n, "exhaustive": exhaustive,
                "counterexample": dict(states[start_idx]),
                "trace": trace, "stem": stem, "cycle": cycle,
                "cycle_start": cycle_start, "forced": forced, "predicate": pred}

    def _resolve_carried(self, var):
        for w in self.carried:                     # exact full-name match first
            if w["name"] == var:
                return w
        for w in self.carried:                     # then short-name (leaf) match
            if w["name"].split(".")[-1] == var:
                return w
        return None

    def _coerce_predicate_value(self, v, value, canon):
        """Turn the caller's predicate `value` into the same python type `_read`
        decodes this var into, so == / <= compare like-with-like."""
        kind = v["kind"]
        if kind == "int":
            return int(value)
        if kind == "real":
            return float(value)
        if kind == "bool":
            if isinstance(value, str):
                s = value.strip().lower()
                if s in ("true", "1", "yes"):
                    return True
                if s in ("false", "0", "no"):
                    return False
                raise ValueError(f"bad bool literal {value!r}")
            return bool(value)
        if kind == "enum":
            variants = self.enum_variants.get(v["name"], [])
            val = str(value)
            if variants and val not in variants:
                raise ValueError(f"{val!r} is not a variant of {v['name']} "
                                 f"({', '.join(variants)})")
            if canon in ("<=", "<", ">=", ">"):
                raise ValueError(f"ordered op {canon!r} is undefined on enum "
                                 f"{v['name'].split('.')[-1]}; use = or !=")
            return val
        # string / fallback
        return str(value) if kind == "string" else value

    @staticmethod
    def _fmt_val(x):
        if isinstance(x, bool):
            return "true" if x else "false"
        return str(x)

    # ---- helpers ------------------------------------------------------------
    def state_key(self, state):
        """Public wrapper over `_key`: the identity tuple a reachable state keys on
        (sorted (name, value) pairs over the carried leaves). The model-diff aligns
        states across two programs by this key, so they must share the carried set."""
        return self._key(state)

    def carried_names(self):
        """The set of carried-leaf names — the var set the diff requires A and B to
        share. Excludes derived vars (never part of the reachable-graph identity)."""
        return {v["name"] for v in self.carried}

    def _key(self, state):
        # Identity keys on CARRIED leaves only. Derived vars live in the state dict for
        # display but are a pure function of carried state, so including them in the key
        # would be redundant at best and could split nodes if a derived value were ever
        # read inconsistently — exclude them so the reachable graph is unchanged whether
        # or not the model has derived vars.
        carried_names = {v["name"] for v in self.carried}
        return tuple(sorted((k, val) for k, val in state.items()
                            if k in carried_names))

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
        # Cached: independence/axis_bounds/change_rates/ranked_vars/facet all sample the same
        # reachable set every analyze — re-running reachable(1500) per caller was most of the
        # real-FSM latency (#171). Compute once per Model, reuse. (Keyed on the default limit; a
        # non-default limit bypasses the cache.)
        if limit != 1500:
            states, _ = self.reachable(limit=limit)
            return states if len(states) >= 2 else self.trajectory(steps=400)
        if getattr(self, "_sample_cache", None) is None:
            states, _ = self.reachable(limit=limit)
            self._sample_cache = states if len(states) >= 2 else self.trajectory(steps=400)
        return self._sample_cache

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

    def independence(self, states=None):
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
        # Reuse a caller-supplied reachable sample when available (the server already has reachable(400)
        # for the stats) — recomputing _sample_states' reachable(1500) here was ~0.6-0.9s of the
        # real-FSM analyze latency for nothing (Ana/Marek #217). 400 states is ample for a
        # functional-dependency check.
        if states is None:
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

    def solved_bounds(self, k=16):
        """PROVABLY-exact per-variable bounds via z3 Optimize over a k-step UNROLLING of the
        transition relation — not the BFS sample. We compose T with itself k times from the
        initial tick: every variable gets a fresh copy per tick, and each carried var's `_prev`
        at tick s is wired to its value at tick s-1. Then for each numeric carried var we maximize
        and minimize its value over ALL ticks (a single Optimize call each, via an Or-selector).

        Returns {short_name: {"lo", "hi", "exact", "k"}}. `exact` is True when the k-step and
        2k-step bounds agree — the unrolled reachable set has CLOSED, so the bound is the true
        reachable extent (the 'compose with itself until 2-run == k-run' fixpoint). When they
        disagree (a diverging/unbounded var) the bound is proven only over the k-step horizon.
        Returns None if there's nothing numeric/carried to bound."""
        ft = self.consts.get(self._first_tick_name)
        carried = [v for v in self.carried
                   if v.get("kind") in ("int", "real")
                   and self.consts.get(v["name"]) is not None
                   and self.consts.get(v["prev"]) is not None]
        if ft is None or not carried:
            return None
        body = z3.And(*self.assertions) if len(self.assertions) != 1 else self.assertions[0]
        ft_name = self._first_tick_name
        prev_to_cur = {self.consts[v["prev"]].get_id(): v["name"] for v in carried}
        non_ft = [(n, c) for n, c in self.consts.items() if n != ft_name]

        def fresh(c, tag):
            return z3.Const(f"{c.decl().name()}@{tag}", c.sort())

        def bounds_at(k):
            opt = z3.Optimize()
            # fresh per-tick copy of every non-prev variable; a pre-initial fresh for tick-0 prevs
            stepv = [{n: fresh(c, s) for n, c in non_ft if c.get_id() not in prev_to_cur}
                     for s in range(k + 1)]
            initprev = {v["name"]: fresh(self.consts[v["prev"]], "init") for v in carried}
            for s in range(k + 1):
                subs = [(ft, z3.BoolVal(s == 0))]
                for n, c in non_ft:
                    if c.get_id() in prev_to_cur:                  # a carried _prev
                        cur = prev_to_cur[c.get_id()]
                        subs.append((c, stepv[s - 1][cur] if s >= 1 else initprev[cur]))
                    else:
                        subs.append((c, stepv[s][n]))
                opt.add(z3.substitute(body, *subs))
            out = {}
            for v in carried:
                cn = v["name"]
                vals = [stepv[s][cn] for s in range(k + 1)]
                sel = z3.Const(f"sel@{cn}", self.consts[cn].sort())
                lo = hi = None
                opt.push(); opt.add(z3.Or([sel == x for x in vals]))
                opt.maximize(sel)
                if opt.check() == z3.sat:
                    hi = self._num(opt.model().eval(sel, model_completion=True))
                opt.pop()
                opt.push(); opt.add(z3.Or([sel == x for x in vals]))
                opt.minimize(sel)
                if opt.check() == z3.sat:
                    lo = self._num(opt.model().eval(sel, model_completion=True))
                opt.pop()
                out[cn.split(".")[-1]] = (lo, hi)
            return out

        try:
            b1 = bounds_at(k)
            b2 = bounds_at(2 * k)
        except Exception:
            return None
        # Inductive-invariant check (Ana #138): k-vs-2k agreement is strong evidence but a finite
        # horizon, not a proof. The 2k box is PROVABLY the exact reachable range iff it's closed
        # under one transition — every value in it is attained by the unroll (so reachable) AND no
        # reachable state escapes it (the inductive check). Then "exact" is a genuine invariant.
        box = {nm: (lo, hi) for nm, (lo, hi) in b2.items() if lo is not None and hi is not None}
        try:
            inductive = self._inductive(box)
        except Exception:
            inductive = False
        result = {}
        for nm, (lo, hi) in b2.items():
            tight = b1.get(nm) == (lo, hi) and lo is not None and hi is not None
            result[nm] = {"lo": lo, "hi": hi, "exact": bool(inductive and tight),
                          "tight": tight, "inductive": bool(inductive), "k": 2 * k}
        return result

    def _inductive(self, box):
        """Is the box {var ∈ [lo,hi]} closed under one transition from a ¬first state? If it's
        UNSAT for 'prev ∈ box ∧ T ∧ some current var escapes box', the box is an inductive
        invariant — so the reachable range is PROVABLY exactly the box (attained + inescapable)."""
        ft = self.consts.get(self._first_tick_name)
        if ft is None or not box:
            return False
        s = z3.Solver()
        for a in self.assertions:
            s.add(a)
        s.add(z3.Not(ft))
        escape = []
        for v in self.carried:
            nm = v["name"].split(".")[-1]
            if nm not in box:
                continue
            lo, hi = box[nm]
            cp, cc = self.consts.get(v["prev"]), self.consts.get(v["name"])
            if cp is None or cc is None:
                continue
            s.add(cp >= lo, cp <= hi)                 # the previous state sits in the box
            escape.append(z3.Or(cc < lo, cc > hi))    # …can the next state leave it?
        if not escape:
            return False
        s.add(z3.Or(escape))
        return s.check() == z3.unsat

    @staticmethod
    def _num(z):
        """A z3 numeral → python int/float. as_long() raises on a non-integer rational, so try
        it first (ints stay ints) and fall back to the exact rational as a float."""
        try:
            return z.as_long()
        except Exception:
            pass
        try:
            return round(float(z.as_fraction()), 3)
        except Exception:
            try:
                return round(float(z.as_decimal(8).rstrip("?")), 3)
            except Exception:
                return None

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
