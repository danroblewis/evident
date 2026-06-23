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

from model_const import CHANNEL_FITNESS, SOLVE_TIMEOUT_MS  # noqa: F401 (SOLVE_TIMEOUT_MS used below; both re-exported)
from model_ranking import RankingMixin
from model_analysis import AnalysisMixin
from model_query import QueryMixin
from model_temporal import TemporalMixin


_LOAD_CACHE = {}          # (smt2_path, schema_path, mtime) -> Model
_LOAD_ORDER = []          # FIFO of keys, bounding the cache (each Model holds a z3 context)


def load(smt2_path, schema_path):
    """Construct a Model from the exported smt2 + schema, cached by (paths, smt2 mtime) with a tiny
    FIFO. Within one /api/analyze the model is built for the analysis AND re-built inside the renderer;
    caching returns the SAME warm model (whose reachable() is memoized), so the render skips a redundant
    parse + BFS — the analyze's dominant remaining cost on real-valued FSMs. The mtime key invalidates
    on a rewrite; per-request tempdir paths are unique, so a cache hit only happens within one request
    (the renderers are read-only on the model, so sharing is safe)."""
    try:
        key = (smt2_path, schema_path, os.path.getmtime(smt2_path))
    except OSError:
        return Model(smt2_path, schema_path)        # can't stat → build uncached
    m = _LOAD_CACHE.get(key)
    if m is None:
        m = _LOAD_CACHE[key] = Model(smt2_path, schema_path)
        _LOAD_ORDER.append(key)
        while len(_LOAD_ORDER) > 4:
            _LOAD_CACHE.pop(_LOAD_ORDER.pop(0), None)
    return m


class Model(RankingMixin, AnalysisMixin, QueryMixin, TemporalMixin):
    # The analysis methods are inherited from four mixins: RankingMixin
    # (viz/model_ranking.py — variable ranking/selection, axis bounds, independence),
    # AnalysisMixin (viz/model_analysis.py — solver bounds, solution_structure, channel
    # + facet mapping), QueryMixin (viz/model_query.py — safety □ check_invariant, ∃
    # query, explore + predicate/graph helpers), and TemporalMixin (viz/model_temporal.py
    # — liveness ◇/□◇/⤳ check_temporal + the witness lasso). This file keeps the
    # load/decode core: __init__ + z3 setup, the value decoders, successor/successors/
    # reachable/trajectory/initial_state, and _key/state_key.
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
        s.set("timeout", SOLVE_TIMEOUT_MS)
        s.add(self.assertions)
        return s

    def _base_cached(self):
        """A reusable base solver with the transition assertions added ONCE, for the hot
        per-point successor() path (#70: the phase-portrait vector field calls successor()
        over an n×n grid — a fresh _base() per point re-adds the whole transition n² times,
        the dominant cost). Callers push()/pop() around their per-call pins so the base is
        left clean. Safe to share: the assertions are immutable after load and the server
        serves one request at a time over a given Model."""
        s = getattr(self, "_cached_base", None)
        if s is None:
            s = self._cached_base = self._base()
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
        """One next state from `state` (None if the transition is unsat here). Uses the
        cached base solver + push/pop so an n×n successor() grid adds the transition once,
        not n² times (#70)."""
        s = self._base_cached()
        s.push()
        try:
            if self.first_tick is not None:
                s.add(self.first_tick == False)  # noqa: E712
            self._pin_prev(s, state)
            return self._read_state(s.model()) if s.check() == z3.sat else None
        finally:
            s.pop()

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
        so it's capped by `limit`. Memoized by `limit` — the model is immutable
        after load, so a second call (e.g. explore's reachable + _trace_to) is free."""
        cache = self.__dict__.setdefault("_reach_cache", {})
        if limit in cache:
            return cache[limit]
        result = self._reachable_uncached(limit)
        cache[limit] = result
        return result

    def _reachable_uncached(self, limit=5000):
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
