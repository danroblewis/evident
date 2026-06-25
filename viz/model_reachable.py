"""model_reachable — the REACHABLE-GRAPH machinery over a loaded `Model` (a ReachabilityMixin).

Split out of `evident_viz.py`'s load/decode core along the seam between SINGLE-STEP
transition primitives and the WHOLE-GRAPH BFS over them. `evident_viz.py` keeps the
per-step queries (initial_state / successor / successors / trajectory — one solve, one
hop); THIS module holds the graph the renderers actually plot — the BFS that closes the
reachable set, plus the two-tick (ΔΔ) pair-graph variant and the closing-depth level
counter:

  - `reachable` / `_reachable_uncached` — BFS of all reachable distinct states (+ edges),
    memoized; dispatches to the two-tick variant when the model reads two ticks back.
  - `_reachable_two` / `_successors_two` / `_initial_prev` — the ΔΔ pair-graph: a node is
    (cur, prev), the seeded tick-0 `_x` bootstraps `__x`, returned states are the current
    snapshots so every downstream consumer sees ordinary single-snapshot states.
  - `closing_depth` / `_closing_depth_bfs` — the depth at which the reachable set CLOSES,
    with the real-valued honesty gate (never certify a continuous model complete).

Provided as a MIXIN class `Model` inherits; bodies moved VERBATIM (still `self`-based).
Every dependency — `self._base`, `self._pin_prev`/`_pin_prev2`, `self._read`/`_read_state`,
`self._lit`/`_block_clause`/`_key`, `self.initial_state`/`self.successors`, and the
`first_tick`/`second_tick`/`has_two_tick`/`carried`/`consts` attributes — resolves at call
time across the mixins + the load core via `self`.
"""
import z3


class ReachabilityMixin:
    def _successors_two(self, cur, prev, limit=64, second=False):
        """ALL distinct next CURRENT-snapshots from the (cur, prev) pair of a two-tick
        model: pin `_x = cur`, and `__x = prev` whenever a two-ago value exists. `second`
        is the tick-1 bootstrap flag (is_second_tick): on the bootstrap step we STILL pin
        `__x = prev` — the SEEDED tick-0 `_x` (e.g. from `_x := x - 3`), which the runtime's
        shift register carries into `__x` — so a second-order model stays deterministic
        instead of fanning out on a free `__x`. Returns current-snapshot state dicts."""
        s = self._base()
        if self.first_tick is not None:
            s.add(self.first_tick == False)  # noqa: E712
        self._pin_prev(s, cur)
        if self.second_tick is not None:
            s.add(self.second_tick == (second or prev is None))  # noqa: E712
        if prev is not None:
            self._pin_prev2(s, prev)
        out = []
        while len(out) < limit and s.check() == z3.sat:
            mod = s.model()
            out.append(self._read_state(mod))
            s.add(self._block_clause(mod))
        return out

    def _initial_prev(self):
        """The tick-0 `_var` values, but ONLY when EVERY carried `_x` is UNIQUELY FORCED on
        tick 0 — a `_x := …` seed, which the runtime's shift register carries into `__x` on
        tick 1. Returns the full forced prev-state, or None when any `_var` is free (e.g.
        fib's `_n`, whose tick-1 bootstrap is the existing is_second_tick path — pinning its
        `__x` to an arbitrary Z3 pick would collapse the reachable graph)."""
        s = self._base()
        if self.first_tick is not None:
            s.add(self.first_tick == True)  # noqa: E712
        if s.check() != z3.sat:
            return None
        mod = s.model()
        out = {}
        for v in self.carried:
            pn = v.get("prev")
            if not pn or pn not in self.consts or v["kind"] == "seq":
                return None
            val = self._read(mod, {"name": pn, "kind": v["kind"]})
            s.push()
            s.add(self.consts[pn] != self._lit(v, val))     # forced ⇔ no other tick-0 value
            forced = (s.check() == z3.unsat)
            s.pop()
            if not forced:
                return None
            out[v["name"]] = val
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
        # The seeded tick-0 `_x` (e.g. `_x := x - 3`) is the two-ago value on tick 1 — the
        # runtime carries it into `__x`. Capture it as the tick-0 node's prev so the bootstrap
        # step pins `__x` and the second-order transition is deterministic, not a free fan.
        init_prev = self._initial_prev()
        # The pair-graph: each node carries (cur, prev); states[] holds the cur dicts.
        states = [init]
        pairs = [(init, init_prev)]                  # tick-0 node: prev = the seeded `_x`
        pair_index = {(self._key(init), self._key(init_prev) if init_prev else None): 0}
        edges = []
        frontier = [0]
        while frontier and len(states) < limit:
            i = frontier.pop()
            cur, prev = pairs[i]
            for nxt in self._successors_two(cur, prev, second=(i == 0)):
                pk = (self._key(nxt), self._key(cur))
                if pk not in pair_index:
                    pair_index[pk] = len(states)
                    states.append(nxt)
                    pairs.append((nxt, cur))
                    frontier.append(pair_index[pk])
                edges.append((i, pair_index[pk]))
        return states, edges

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

    def closing_depth(self, limit=5000):
        """The BFS depth at which the reachable set CLOSES — the level whose expansion
        adds no new state. Returns (k, complete):

          - `k`        — the depth (distance from the initial state) at which the LAST new
                         state was discovered; one more level of BFS adds nothing reachable.
          - `complete` — True iff the reachable set was fully enumerated within `limit` (the
                         frontier emptied before the cap). When the BFS hits the cap the set
                         is still growing, so `complete` is False and `k` is only a lower bound.

        For a CONTINUOUS model (any real-valued carried var) the reachable set isn't a finite
        enumerable graph; `complete` is forced False so no caller can mistake a capped
        real-valued exploration for a proof (Ana's honesty bar — `is_discrete()` / the real gate).

        Runs its OWN level-order BFS rather than reusing reachable()'s LIFO frontier (which has
        no per-level structure) — same successor relation, same dedup key, so the reachable set
        is identical; the only addition is the per-level counter. The two-tick (ΔΔ) graph is
        handled by the pair-keyed variant, matching _reachable_two."""
        if any(v.get("kind") == "real" for v in self.carried):
            k, _ = self._closing_depth_bfs(limit)
            return k, False           # never certify a real-valued model complete
        return self._closing_depth_bfs(limit)

    def _closing_depth_bfs(self, limit):
        init = self.initial_state()
        if init is None:
            return 0, True
        two = self.has_two_tick
        if two:
            seen = {(self._key(init), None)}
            level = [(init, None)]
        else:
            seen = {self._key(init)}
            level = [init]
        depth = 0
        closing = 0
        capped = False
        while level:
            nxt_level = []
            for node in level:
                if two:
                    cur, prev = node
                    succs = self._successors_two(cur, prev)
                else:
                    cur = node
                    succs = self.successors(cur)
                for s in succs:
                    key = (self._key(s), self._key(cur)) if two else self._key(s)
                    if key in seen:
                        continue
                    if len(seen) >= limit:
                        capped = True
                        continue
                    seen.add(key)
                    nxt_level.append((s, cur) if two else s)
            if nxt_level:
                depth += 1
                closing = depth        # this level introduced new reachable states
            level = nxt_level
            if capped:
                break
        return closing, not capped
