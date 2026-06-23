"""model_analysis — solver-computed BOUNDS + STRUCTURE over a loaded `Model` (an AnalysisMixin).

Split out of `evident_viz.py`; the variable-ranking/selection concern lives in
`model_ranking.py`. This module holds the rigorous z3 questions + the channel/facet
mapping that consumes the ranking:

  - `solved_bounds` / `_inductive` / `_num` — PROVABLY-exact per-var bounds via a k-step
    unrolling + the inductive-invariant (closed-box) check
  - `solution_structure` — whole-model fixed points / equilibria / verdict over the
    reachable relation
  - `independence_structural` — directed dependency by solver sensitivity (the rigorous
    `independence`)
  - `var_class` / `numeric_vars` / `categorical_vars` / `assign_channels` — map ranked
    vars onto visual channels by type-effectiveness
  - `change_rates` / `facet_var` — faceting suitability

Provided as a MIXIN class `Model` inherits; bodies moved VERBATIM (still `self`-based),
a behavior-preserving relocation. `numeric_vars` / `categorical_vars` / `facet_var`
read the ranking (`self.state_vars`, `self._sample_states`) from RankingMixin via `self`.
"""
import z3

from model_const import CHANNEL_FITNESS, SOLVE_TIMEOUT_MS


class AnalysisMixin:

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
            opt.set("timeout", SOLVE_TIMEOUT_MS)
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

    def unroll_smt2(self, k=8):
        """The k-step UNROLLED transition as SMT-LIB (#259/#19): the single-tick relation composed
        with itself k times — a fresh copy of every variable per tick (`x@s`), each tick's `_prev`
        wired to the prior tick's value, is_first_tick true only at tick 0. For bounded model
        checking in z3 (add a property over the `@s` vars, then check-sat). Returns the SMT-LIB text,
        or None if there's no transition. Faithful: ALL carried prevs are wired (unlike solved_bounds,
        which wires only the numeric ones for its bounds Optimize).

        Prepends a COMPLETENESS CERTIFICATION (Ana #270): the reachable set's CLOSING DEPTH turns this
        bounded check into a PROOF when the set closes within scope. If the BFS reaches a level that
        adds no new state at depth d ≤ k and the set was fully enumerated (discrete, not capped), the
        unroll to k covers EVERY reachable state — so an unsat property check is a proof, not a bound.
        Otherwise the comment says BOUNDED honestly (still growing at the cap, or real-valued)."""
        ft = self.consts.get(self._first_tick_name)
        if ft is None or not self.assertions:
            return None
        body = z3.And(*self.assertions) if len(self.assertions) != 1 else self.assertions[0]
        prev_to_cur = {self.consts[v["prev"]].get_id(): v["name"] for v in self.carried
                       if self.consts.get(v["prev"]) is not None and self.consts.get(v["name"]) is not None}
        non_ft = [(n, c) for n, c in self.consts.items() if n != self._first_tick_name]

        def fresh(c, tag):
            return z3.Const(f"{c.decl().name()}@{tag}", c.sort())

        stepv = [{n: fresh(c, s) for n, c in non_ft if c.get_id() not in prev_to_cur}
                 for s in range(k + 1)]
        initprev = {cur: fresh(self.consts[cur], "init") for cur in set(prev_to_cur.values())}
        s = z3.Solver()
        for step in range(k + 1):
            subs = [(ft, z3.BoolVal(step == 0))]
            for n, c in non_ft:
                if c.get_id() in prev_to_cur:
                    cur = prev_to_cur[c.get_id()]
                    subs.append((c, stepv[step - 1][cur] if step >= 1 else initprev[cur]))
                else:
                    subs.append((c, stepv[step][n]))
            s.add(z3.substitute(body, *subs))
        # to_smt2() ends with (check-sat); append (get-model) so the BMC workflow surfaces the
        # WITNESS TRACE — add a property over the @k vars, check-sat, and read the violating
        # assignment across all ticks (Ana #265: a bare check-sat gives sat/unsat with no trace).
        return self._completeness_comment(k) + s.to_smt2() + "(get-model)\n"

    def _completeness_comment(self, k):
        """The completeness certification prepended to the unroll export (Ana #270). Returns the
        comment lines: COMPLETE when the reachable set closes at a depth d ≤ k within scope (k-step
        unroll covers EVERY reachable state — an unsat check is a PROOF), else BOUNDED (k is a lower
        bound; an unsat check only rules out violations within k steps). Continuous/real models and
        capped explorations are NEVER certified complete — the closing_depth gate forces that."""
        try:
            d, complete = self.closing_depth()
        except Exception:
            d, complete = None, False
        if complete and d is not None and d <= k:
            return (f"; COMPLETE at depth k={d} — the reachable set closed (no new states beyond "
                    f"depth {d}); this k-step unroll covers EVERY reachable state, so an unsat "
                    f"property check here is a PROOF, not a bound.\n")
        if complete and d is not None:
            # closed, but deeper than this unroll — honest: raise k to k≥d to make it a proof.
            return (f"; BOUNDED — reachable set closes at depth {d}, but this unroll only reaches "
                    f"k={k} (< {d}); k is a lower bound. Re-export with k≥{d} for a completeness "
                    f"PROOF; as-is an unsat check only rules out violations within {k} steps.\n")
        return (f"; BOUNDED — reachable set still growing at the scope cap; k={k} is a lower bound, "
                f"an unsat check only rules out violations within k steps.\n")

    def _inductive(self, box):
        """Is the box {var ∈ [lo,hi]} closed under one transition from a ¬first state? If it's
        UNSAT for 'prev ∈ box ∧ T ∧ some current var escapes box', the box is an inductive
        invariant — so the reachable range is PROVABLY exactly the box (attained + inescapable)."""
        ft = self.consts.get(self._first_tick_name)
        if ft is None or not box:
            return False
        s = z3.Solver()
        s.set("timeout", SOLVE_TIMEOUT_MS)
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
            sfp.set("timeout", SOLVE_TIMEOUT_MS)
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
        """Ranked numeric (quantitative) interface vars — for axes/size. Excludes Seq
        vars: a Seq is a VECTOR, not a scalar position/size channel, so it can't drive a
        continuous axis (the channel-mapping + phase-portrait renderers would feed a
        Python list into arithmetic). The time-series renderer plots its elements as
        parallel tracks instead, reading the Seq directly off the state dict."""
        return [v for v in self.state_vars
                if self.var_class(v) == "quant" and v["kind"] != "seq"]

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
            if v["kind"] == "seq":
                continue                       # a Seq is a vector, not a scalar channel
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
