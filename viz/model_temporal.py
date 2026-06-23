"""model_temporal — LIVENESS / temporal-logic checking over a loaded `Model` (a TemporalMixin).

Split out of `model_query.py` along the classic SAFETY-vs-LIVENESS divide. `model_query.py`
keeps the safety/existential queries (□ check_invariant, ∃ query, explore); THIS module
holds the liveness side — does a property eventually / recurrently hold on EVERY run? —
and the stem+cycle lasso + fairness machinery that witnesses a failure:

  - `check_temporal` — ◇ (eventually) / ⤳ (leads-to) / □◇ (infinitely-often)
  - `_liveness_analysis` — the full graph + ¬Q subgraph + trap/avoid/fairness sets
  - `_lasso` / `_backward_reach` / `_bfs_reach` — build the witness lasso + reach closures
  - `_holds_result` / `_fail_result` — package the verdict

Provided as a MIXIN class `Model` inherits; bodies moved VERBATIM (still `self`-based).
The stem path uses `self._bfs_indices` and the predicate builder `self._conj_predicate`
from QueryMixin via `self` — both resolve at call time across the mixins.
"""


class TemporalMixin:
    def _lasso(self, start_idx, bad, sub, g, can_reach_q, edges, states, full_stem=False):
        """Build the STEM + CYCLE lasso witnessing that a run from `start_idx` dodges Q forever,
        plus the FAIRNESS classification (Ana #239). The stem is the shortest path from `start_idx`
        to the entry of a dodging ¬Q cycle/sink; the cycle is the actual ¬Q loop back to that entry
        (or [] when the dodge is a terminal ¬Q sink). `forced` is True iff NO state on the cycle has
        an out-edge in the FULL graph to a state from which Q is reachable — i.e. the run literally
        cannot escape to Q even under weak fairness.

        `full_stem` selects the STEM's graph (the trap itself is always a ¬Q cycle/sink in `sub`):
          - False (◇/⤳): the stem stays INSIDE the ¬Q subgraph — the run dodges Q from start onward.
          - True  (□◇): the stem runs over the FULL graph `g`, so it MAY pass through Q-states before
            settling in the ¬Q trap (Q reached finitely, then never again — the □◇ counterexample).

        Returns (stem_states, cycle_states, forced). `stem` always includes the cycle entry as its
        last element; `cycle` is the entry-to-entry loop (entry repeated at the end) or [] for a sink.
        """
        import networkx as nx
        # Pick the nearest dodging target reachable from start (shortest stem). For ◇ the reach
        # stays inside ¬Q; for □◇ the trap may sit behind Q-states, so reach over the full graph.
        stem_graph = g if full_stem else sub
        restrict = None if full_stem else set(sub.nodes)
        entry, stem_idx = None, None
        seen = self._bfs_reach(start_idx, stem_graph)     # dodging nodes reachable from start, by distance
        for cand in sorted(seen, key=seen.get):
            if cand in bad:
                entry, stem_idx = cand, self._bfs_indices(start_idx, cand, edges, restrict=restrict)
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
    def _backward_reach(seeds, graph):
        """Every node from which some `seed` is reachable in `graph` (reverse reachability over
        `graph.predecessors`), inclusive of the seeds. Used to grow a ¬Q-trap set into the set of
        states that can reach it — over the ¬Q subgraph for ◇ (dodge Q from here on) or the full
        graph for □◇ (fall into the ¬Q trap, even through Q)."""
        reached, stack = set(seeds), list(seeds)
        while stack:
            for w in graph.predecessors(stack.pop()):
                if w not in reached:
                    reached.add(w); stack.append(w)
        return reached

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

    def _fair_check(self, states, edges, a, p_idxs, exhaustive, pred):
        """WEAK-FAIRNESS liveness verdict — a GRAPH-REACHABILITY check that EXCLUDES unfair lassos
        (Ana #269). The lasso search above refutes □◇P / P⤳Q on ANY dodging run, including UNFAIR
        ones that perpetually ignore an always-available path to P; every branching FSM has such a
        lasso, so liveness almost always 'fails'. Under weak fairness those unfair runs are excluded,
        and the formulation collapses to reachability:

          □◇P (and ◇P) holds under fairness  ⟺  from EVERY reachable state, a P-state is reachable.
          P⤳Q holds under fairness            ⟺  from every reachable P-state, a Q-state is reachable.

        A fair run that can always re-reach P does so infinitely often; the ONLY fair counterexample
        is a TRAP — a reachable (P-)state from which P is UNREACHABLE. `a['can_reach_q']` is exactly
        the set of states that can reach a Q/P-state (reverse-reach from the goal over the full graph),
        so the trap is the first `p_idxs` state NOT in it. On a trap we hand back the init→trap run
        (the existing `_trace_to`), not a lasso — there is no escaping cycle to show.

        `p_idxs` is the iterable of indices the goal must be reachable FROM: every reachable state for
        □◇/◇, the reachable P-states for ⤳. Returns the same verdict-dict shape as `_holds_result` /
        `_fail_result`, plus `fair=True` (and `trap=True` on the failing case)."""
        can_reach = a["can_reach_q"]
        n = len(states)
        trap = next((i for i in p_idxs if i not in can_reach), None)
        if trap is None:                                       # every (P-)state can reach the goal
            r = self._holds_result(n, exhaustive, pred)
            r["fair"] = True
            return r
        return {"holds": False, "checked": n, "exhaustive": exhaustive, "fair": True,
                "counterexample": dict(states[trap]),
                "trace": self._trace_to(trap, edges, states),  # init→trap run (no escaping cycle)
                "stem": None, "cycle": None, "cycle_start": None, "forced": True,
                "trap": True, "predicate": pred}

    def check_temporal(self, terms, modality="eventually", p_terms=None, limit=400, fair=False):
        """VERIFY a LIVENESS property over the reachable graph (the model-checker move beyond the
        safety □ that check_invariant does):
          - modality "eventually" (◇Q): does EVERY run from the initial state reach a Q-state?
          - modality "leads_to"   (P ⤳ Q): from every reachable P-state, is Q eventually reached?
          - modality "infinitely_often" (□◇Q): does EVERY run hit Q INFINITELY often — i.e. can no
            run get permanently TRAPPED in ¬Q? Stronger than ◇: ◇ asks Q is reached at least once;
            □◇ asks Q recurs forever. A ¬Q trap reachable even THROUGH Q-states (Q hit finitely,
            then ¬Q forever) breaks □◇ but not ◇ — that's the precise gap this catches.
        A run can AVOID Q forever iff it reaches a ¬Q cycle or gets stuck in a ¬Q terminal. We
        compute that 'avoid' set from ¬Q cycles ∪ ¬Q sinks by backward reachability; the GRAPH that
        backward reach runs over is the discriminator: within the ¬Q subgraph for ◇/⤳ (a run that
        dodges Q from here on), over the FULL graph for □◇ (a run that can fall into the ¬Q trap,
        even after passing through Q). ◇Q holds iff the initial state isn't in the ¬Q-subgraph avoid
        set; P⤳Q iff no P-state is; □◇Q holds iff the initial state isn't in the full-graph avoid set.

        On failure we return a verifiable STEM+CYCLE LASSO (Ana #239), not just a single state:
          - `stem`:  shortest path INSIDE ¬Q from the offending state to the dodging-cycle entry.
          - `cycle`: the actual ¬Q loop back to that entry ([] for a stuck terminal ¬Q sink).
          - `trace`: stem + cycle (one walk), kept for the stepper; `cycle_start` indexes where
                     the cycle begins inside `trace`.
          - `forced`: the FAIRNESS verdict — True iff no cycle state can step (in the FULL graph)
                     into a state from which Q is reachable (a real counterexample even under weak
                     fairness); False (AVOIDABLE) iff some cycle state has a fair successor that
                     escapes to Q — under fairness that successor eventually fires and Q holds.

        `fair=True` switches the whole check into WEAK-FAIRNESS mode (Ana #269): instead of the
        lasso search — which refutes on any dodging run, including UNFAIR ones that perpetually
        ignore an always-available path to Q — it runs the reachability oracle in `_fair_check`:
        □◇/◇ hold iff EVERY reachable state can reach Q; P⤳Q iff every reachable P-state can. The
        only fair counterexample is a TRAP (a reachable state from which Q is unreachable); on a
        trap the verdict carries `trap=True` + the init→trap run (no escaping cycle to show)."""
        qpred, qfn = self._conj_predicate(terms)
        states, edges = self.reachable(limit=limit)
        n = len(states)
        if n == 0:
            return {"holds": True, "checked": 0, "exhaustive": True,
                    "counterexample": None, "predicate": f"◇ {qpred}"}
        exhaustive = n < limit
        a = self._liveness_analysis(states, edges, qfn)        # graphs + trap/avoid/fairness sets
        return self._temporal_verdict(modality, qpred, qfn, p_terms, fair,
                                      states, edges, a, n, exhaustive)

    def _temporal_verdict(self, modality, qpred, qfn, p_terms, fair, states, edges, a, n, exhaustive):
        """Pick the per-modality liveness verdict over the analyzed graph (extracted from
        `check_temporal` so each owns one concern). `fair=True` routes every modality through the
        `_fair_check` reachability oracle (#269); otherwise the lasso/avoid-set search runs:
          - □◇: holds iff init can't fall into a ¬Q trap over the FULL graph (else a full-stem lasso).
          - ⤳:  holds iff no reachable P-state is in the ¬Q avoid set (else a lasso from one).
          - ◇:  holds iff init isn't in the ¬Q avoid set; on holds, `recurrent` flags □◇ (#260)."""
        if modality == "infinitely_often":
            pred = f"□◇ {qpred}"
            if fair:                                           # exclude unfair lassos (#269): every
                return self._fair_check(states, edges, a, range(n), exhaustive, pred)  # state reaches Q?
            if 0 not in a["avoid_full"]:                       # init can't be trapped in ¬Q
                return self._holds_result(n, exhaustive, pred)
            return self._fail_result(0, a["bad"], a["sub"], a["g"], a["can_reach_q"], edges, states,
                                     n, exhaustive, pred, full_stem=True)

        if modality == "leads_to":
            ppred, pfn = self._conj_predicate(p_terms)
            pred = f"{ppred} ⤳ {qpred}"
            if fair:                                           # every reachable P-state reaches Q?
                p_idxs = [i for i in range(n) if pfn(states[i])]
                return self._fair_check(states, edges, a, p_idxs, exhaustive, pred)
            offenders = [i for i in range(n) if pfn(states[i]) and i in a["avoid"]]
            if not offenders:
                return self._holds_result(n, exhaustive, pred)
            return self._fail_result(offenders[0], a["bad"], a["sub"], a["g"], a["can_reach_q"],
                                     edges, states, n, exhaustive, pred)

        pred = f"◇ {qpred}"                                    # eventually (AF), the default
        if fair:                                               # ◇ under fairness ≡ □◇ under fairness:
            return self._fair_check(states, edges, a, range(n), exhaustive, pred)  # every state reaches Q
        if 0 not in a["avoid"]:                                # initial state is index 0
            # ◇Q holds. Distinguish RECURRENT (□◇ also holds — Q hit infinitely often) from
            # TRANSIENT (Q reached but the run can then settle into ¬Q forever): recurrent iff
            # init can't be trapped in ¬Q over the full graph (Ana #260).
            return self._holds_result(n, exhaustive, pred, recurrent=0 not in a["avoid_full"])
        return self._fail_result(0, a["bad"], a["sub"], a["g"], a["can_reach_q"], edges, states,
                                 n, exhaustive, pred)

    def _liveness_analysis(self, states, edges, qfn):
        """Graph-analysis shared by all three temporal modalities. Builds the full graph `g` and
        the ¬Q subgraph `sub`, the `bad` set (¬Q cycles ∪ ¬Q sinks — the traps a run can dodge Q
        in forever), and two backward-reach closures of `bad`: `avoid` (within ¬Q — dodges Q from
        here on, the ◇/⤳ oracle) and `avoid_full` (over the full graph — can FALL into a ¬Q trap
        even through Q, the □◇ oracle). `can_reach_q` is the fairness oracle (reverse reach from Q
        over g). Returns the bundle as a dict."""
        import networkx as nx
        n = len(states)
        g = nx.DiGraph(); g.add_nodes_from(range(n)); g.add_edges_from(edges)
        notq = [i for i in range(n) if not qfn(states[i])]
        sub = g.subgraph(notq)
        bad = {i for i in notq if g.out_degree(i) == 0}        # ¬Q terminal: stuck, never Q
        for comp in nx.strongly_connected_components(sub):     # ¬Q cycle: loop forever in ¬Q
            if len(comp) >= 2 or any(sub.has_edge(c, c) for c in comp):
                bad |= set(comp)
        qset = [i for i in range(n) if qfn(states[i])]
        return {"g": g, "sub": sub, "bad": bad,
                "avoid": self._backward_reach(bad, sub),
                "avoid_full": self._backward_reach(bad, g),
                "can_reach_q": self._backward_reach(qset, g)}

    @staticmethod
    def _holds_result(n, exhaustive, pred, recurrent=None):
        """The 'liveness holds' verdict dict — no counterexample/lasso. `recurrent` (◇ only) flags
        whether □◇ also holds (Q recurs) vs a transient ◇ (Q reached once, then settles in ¬Q)."""
        r = {"holds": True, "checked": n, "exhaustive": exhaustive, "counterexample": None,
             "trace": None, "stem": None, "cycle": None, "cycle_start": None, "forced": None,
             "predicate": pred}
        if recurrent is not None:
            r["recurrent"] = recurrent
        return r

    def _fail_result(self, start_idx, bad, sub, g, can_reach_q, edges, states, n, exhaustive, pred,
                     full_stem=False):
        """Package a failing liveness check as the stem+cycle lasso + fairness flag. `trace`
        (= stem + cycle, the cycle entry not repeated) stays for back-compat with the stepper;
        `cycle_start` is the index into `trace` where the loop begins. `full_stem` (□◇) lets the
        stem pass through Q-states on the way to the ¬Q trap."""
        stem, cycle, forced = self._lasso(start_idx, bad, sub, g, can_reach_q, edges, states,
                                          full_stem=full_stem)
        # trace = stem then the cycle body (drop the repeated entry: stem already ends on it,
        # and the cycle's trailing entry is the same node — so the walk reads init→…→entry→…→entry).
        cycle_body = cycle[1:] if cycle else []                # cycle = [entry, …, entry]; skip lead entry
        trace = stem + cycle_body
        cycle_start = len(stem) - 1 if cycle else None         # the entry (last stem state) opens the loop
        return {"holds": False, "checked": n, "exhaustive": exhaustive,
                "counterexample": dict(states[start_idx]),
                "trace": trace, "stem": stem, "cycle": cycle,
                "cycle_start": cycle_start, "forced": forced, "predicate": pred}
